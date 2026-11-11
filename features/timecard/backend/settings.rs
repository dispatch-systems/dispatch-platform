//! Timecard's settings, on its page of Settings: which employees it lists and how Meal Breaks
//! names them, the departments that drive, and when a DA is late.
use crate::api::{
    assessment::LateRule,
    settings::{NameOrder, PaycomSettings},
};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Options, Section, choice, many, pick, section, time, toggle},
    settings::read,
};
use serde::Deserialize;

const MANAGE: &str = "timecard.manage";

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
