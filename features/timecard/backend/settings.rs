//! Timecard's settings, on its page of Settings: the schedules that collect it, which
//! employees it lists and how Meal Breaks names them, the departments that drive, and when a
//! DA is late; and the preferences a DSP kept in Paycom's database before Settings kept them,
//! moved in once.
use crate::api::{
    assessment::LateRule,
    settings::{NameOrder, PaycomSettings},
};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Options, Section, choice, many, pick, section, time, toggle},
    settings::{adopt, read},
};
use dispatch_paycom as paycom;
use serde::Deserialize;
use serde_json::{Map, Value, json};

const MANAGE: &str = "timecard.manage";

/// Its sync schedules, drawn by its own component: each runs Paycom's timecards, Cortex's meal
/// breaks, or both at once, which core's schedule editor doesn't offer.
pub const SCHEDULES: Section = section("timecard.schedules", "Sync schedules", MANAGE, &[])
    .custom()
    .keywords(&["collection", "schedule"])
    .order(10);

/// Which of the newest roster's employees Timecard lists, and how Meal Breaks writes their
/// names.
pub const ROSTER: Section = section(
    "timecard.roster",
    "Employees",
    MANAGE,
    &[
        pick("department", "Department", filters::departments, None).none("All departments"),
        pick("station", "Delivery station", filters::stations, None).none("All stations"),
        choice(
            "name_order",
            "Name order",
            &[
                (NameOrder::FirstLast.as_str(), "First Last"),
                (NameOrder::LastFirst.as_str(), "Last, First"),
            ],
            NameOrder::FirstLast.as_str(),
        ),
    ],
)
.order(20);
/// `ROSTER`'s values, as Timecard reads them: none lists every department or station.
#[derive(Deserialize)]
pub(crate) struct Listing {
    pub department: Option<String>,
    pub station: Option<String>,
    pub name_order: NameOrder,
}
impl Listing {
    /// Whether it lists an employee of `department` at `station`.
    pub(crate) fn lists(&self, department: &str, station: &str) -> bool {
        self.department
            .as_deref()
            .is_none_or(|own| own == department)
            && self.station.as_deref().is_none_or(|own| own == station)
    }
}

/// The departments whose employees drive: the Timecard tab lists only them, and Driver Match
/// counts only them as drivers.
pub const DRIVERS: Section = section(
    "timecard.drivers",
    "Driver departments",
    MANAGE,
    &[
        toggle("all_departments", "All departments", true),
        many(
            "departments",
            "Departments",
            Options::Listed(filters::every_department),
            &[],
        ),
    ],
)
.order(30);
#[derive(Deserialize)]
struct Drivers {
    all_departments: bool,
    departments: Vec<String>,
}
/// The departments whose employees drive, as the DSP chose them: none when they all do.
pub(crate) fn driver_departments(db: &Store, dsp: &str) -> Result<Option<Vec<String>>> {
    let drivers: Drivers = read(db, dsp, &DRIVERS)?;
    Ok((!drivers.all_departments).then_some(drivers.departments))
}

/// When a DA is late on Meal Breaks: a Paycom IN DAY punch at or after `time`, in one of
/// `departments`, or in any when none is chosen. Its values read as a `LateRule`.
pub const LATE_DAS: Section = section(
    "timecard.late_das",
    "Late DAs",
    MANAGE,
    &[
        time("time", "Late at or after", "10:01"),
        many(
            "departments",
            "Departments",
            Options::Listed(filters::every_department),
            &[],
        ),
    ],
)
.part("timecard.meal_breaks")
.order(40);

/// What the Timecard page shows by, of the DSP's settings, for everyone who sees it.
pub(crate) fn shown(db: &Store, dsp: &str) -> Result<PaycomSettings> {
    let listing: Listing = read(db, dsp, &ROSTER)?;
    Ok(PaycomSettings {
        name_order: listing.name_order,
        driver_departments: driver_departments(db, dsp)?,
        late: read::<LateRule>(db, dsp, &LATE_DAS)?,
    })
}

/// The options its fields list as Settings is read: what the DSP's newest Paycom roster names,
/// in one indexed read of its employees, `dsp`'s alone.
mod filters {
    use dispatch_core::{Result, db::Store};
    use dispatch_paycom as paycom;

    const DEPARTMENTS: &str = "SELECT DISTINCT department FROM employees \
        WHERE publication_id=(SELECT id FROM publications WHERE active=1) ORDER BY department";
    const STATIONS: &str = "SELECT DISTINCT station FROM employees \
        WHERE publication_id=(SELECT id FROM publications WHERE active=1) ORDER BY station";

    fn listed(store: &Store, dsp: &str, sql: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = store.collector(dsp, paycom::PROVIDER)?.query_as(sql, [])?;
        Ok(rows.into_iter().map(|(value,)| value).collect())
    }
    /// Every department, employees listed under none among them.
    pub(super) fn every_department(store: &Store, dsp: &str) -> Result<Vec<(String, String)>> {
        Ok(listed(store, dsp, DEPARTMENTS)?
            .into_iter()
            .map(|department| {
                let label = match department.as_str() {
                    "" => "No department".to_owned(),
                    named => named.to_owned(),
                };
                (department, label)
            })
            .collect())
    }
    /// The departments a filter may name: none of them empty, as a filter of none lists all.
    pub(super) fn departments(store: &Store, dsp: &str) -> Result<Vec<(String, String)>> {
        named(listed(store, dsp, DEPARTMENTS)?)
    }
    pub(super) fn stations(store: &Store, dsp: &str) -> Result<Vec<(String, String)>> {
        named(listed(store, dsp, STATIONS)?)
    }
    fn named(values: Vec<String>) -> Result<Vec<(String, String)>> {
        Ok(values
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(|value| (value.clone(), value))
            .collect())
    }
}

/// Moves the preferences a DSP kept in Paycom's database before Settings kept them,
/// `paycom.preferences`, into its sections as their first values, at startup. A section that
/// holds values, saved or moved, keeps them; one whose values its fields refuse now keeps its
/// defaults, rather than the DSP failing to open. What the page no longer reads (its opening
/// tab, rows per page, sort and columns) stays behind; the record stays as it was, read only
/// here.
pub(crate) fn upgrade(store: &Store, dsp: &str) -> Result<()> {
    let adopt = |section: &Section, values: Value| match adopt(store, dsp, section, &values) {
        Err(error) if error.code == "invalid_input" => Ok(()),
        moved => moved.map(drop),
    };
    let kept = store
        .collector(dsp, paycom::PROVIDER)?
        .setting("paycom.preferences", Value::Null)?;
    let Some(kept) = kept.get("values").and_then(Value::as_object) else {
        return Ok(());
    };
    let mut roster = Map::new();
    // An empty filter listed everyone, as none does now.
    for key in ["department", "station"] {
        if let Some(value) = kept
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        {
            roster.insert(key.to_owned(), json!(value));
        }
    }
    if let Some(order) = kept.get("name_order") {
        roster.insert("name_order".to_owned(), order.clone());
    }
    adopt(&ROSTER, Value::Object(roster))?;
    // None drove every department; a list, only those, an empty one none.
    if let Some(departments) = kept.get("driver_departments").filter(|v| v.is_array()) {
        adopt(
            &DRIVERS,
            json!({"all_departments": false, "departments": departments}),
        )?;
    }
    let mut late = Map::new();
    for (from, to) in [
        ("late_da_time", "time"),
        ("late_da_departments", "departments"),
    ] {
        if let Some(value) = kept.get(from) {
            late.insert(to.to_owned(), value.clone());
        }
    }
    adopt(&LATE_DAS, Value::Object(late))
}
