//! What each source knows about the people in it: the IDs, the names it writes and the
//! days it saw them. Read afresh each time; Driver Match stores only what was decided.
use crate::{
    Result,
    collectors::Provider,
    contracts::{DriverData, DriverSource},
    db::{Db, Store, n, s},
    scorecard,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// An ID as one source knows it.
pub(crate) type Key = (DriverSource, String);

/// One ID as its sources describe it. Amazon's sources each write a driver's name their
/// own way; `names` keeps every spelling once, the most readable first.
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub source: DriverSource,
    pub id: String,
    pub names: Vec<String>,
    pub department: Option<String>,
    pub position: Option<String>,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On Paycom's current roster as an active employee.
    pub listed: bool,
}
impl Identity {
    fn new(source: DriverSource, id: &str) -> Self {
        Self {
            source,
            id: id.to_owned(),
            names: vec![],
            department: None,
            position: None,
            first_seen: None,
            last_seen: None,
            listed: false,
        }
    }
    fn named(&mut self, name: &str) {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if !name.is_empty() && !self.names.contains(&name) {
            self.names.push(name);
        }
    }
    fn seen(&mut self, first: &str, last: &str) {
        if !first.is_empty() && self.first_seen.as_deref().is_none_or(|f| first < f) {
            self.first_seen = Some(first.to_owned());
        }
        if !last.is_empty() && self.last_seen.as_deref().is_none_or(|l| last > l) {
            self.last_seen = Some(last.to_owned());
        }
    }
    /// The name to show and store: the first, most readable spelling.
    pub fn name(&self) -> &str {
        self.names.first().map_or("", String::as_str)
    }
}

/// The Saturday a scorecard week ends, or the week as written when it does not parse.
fn week_end(week: &str) -> String {
    scorecard::week_days(week).map_or_else(|_| week.to_owned(), |(_, end)| end.to_string())
}
fn week_start(week: &str) -> String {
    scorecard::week_days(week).map_or_else(|_| week.to_owned(), |(start, _)| start.to_string())
}

/// Every ID the DSP's collections hold: Paycom's employees, then every driver Amazon's
/// sources name. Routes come first among Amazon's names, as they spell names most plainly;
/// the scorecard often adds a middle name and DVIC sometimes only a last initial.
pub(crate) fn identities(store: &Store, dsp: &str) -> Result<Vec<Identity>> {
    let mut found: BTreeMap<Key, Identity> = BTreeMap::new();
    let paycom = store.collector(dsp, Provider::Paycom)?;
    // The newest roster names an employee as Paycom lists them now.
    for row in paycom.all(
        "SELECT e.code,e.name,e.department,e.position,substr(p.collected_at,1,10) day,\
         p.active=1 AND e.active=1 listed \
         FROM employees e JOIN publications p ON p.id=e.publication_id \
         ORDER BY p.collected_at,p.id",
        [],
    )? {
        let id = s(&row, "code").trim();
        if id.is_empty() {
            continue;
        }
        let entry = found
            .entry((DriverSource::Paycom, id.to_owned()))
            .or_insert_with(|| Identity::new(DriverSource::Paycom, id));
        entry.names.clear();
        entry.named(s(&row, "name"));
        entry.department = Some(s(&row, "department").to_owned()).filter(|d| !d.is_empty());
        entry.position = Some(s(&row, "position").to_owned()).filter(|p| !p.is_empty());
        let day = s(&row, "day");
        entry.seen(day, day);
        entry.listed |= n(&row, "listed") == 1;
    }
    // Days worked say when an employee was really there; the roster only that they were
    // listed. Worked days replace the roster's wherever there are any.
    for row in paycom.all(
        "SELECT employee_code code,min(date) first,max(date) last FROM timecards \
         WHERE hours>0 GROUP BY employee_code",
        [],
    )? {
        if let Some(entry) = found.get_mut(&(DriverSource::Paycom, s(&row, "code").to_owned())) {
            entry.first_seen = Some(s(&row, "first").to_owned());
            entry.last_seen = Some(s(&row, "last").to_owned());
        }
    }
    drop(paycom);
    let mut amazon = |rows: Vec<Value>, name: &dyn Fn(&Value) -> String, week: bool| {
        for row in rows {
            let id = s(&row, "id").trim();
            if id.is_empty() {
                continue;
            }
            let entry = found
                .entry((DriverSource::Amazon, id.to_owned()))
                .or_insert_with(|| Identity::new(DriverSource::Amazon, id));
            entry.named(&name(&row));
            if week {
                entry.seen(&week_start(s(&row, "first")), &week_end(s(&row, "last")));
            } else {
                entry.seen(s(&row, "first"), s(&row, "last"));
            }
        }
    };
    amazon(
        store.routedata(dsp)?.all(
            "SELECT transporter_id id,first_name,last_name,first_seen_day first,\
             last_seen_day last FROM drivers",
            [],
        )?,
        &|row| format!("{} {}", s(row, "first_name"), s(row, "last_name")),
        false,
    );
    let plain = |row: &Value| s(row, "name").to_owned();
    amazon(
        store.collector(dsp, Provider::Cortex)?.all(
            "SELECT i.transporter_id id,i.driver_name name,min(p.report_date) first,\
             max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id GROUP BY i.transporter_id,i.driver_name \
             ORDER BY last DESC",
            [],
        )?,
        &plain,
        false,
    );
    amazon(
        store.scorecard(dsp)?.all(
            "SELECT d.transporter_id id,json_extract(d.row,'$.da_name') name,\
             min(p.week) first,max(p.week) last FROM driver_scorecards d \
             JOIN scorecard_publications p ON p.id=d.publication_id \
             WHERE COALESCE(d.transporter_id,'')<>'' GROUP BY 1,2 ORDER BY last DESC",
            [],
        )?,
        &plain,
        true,
    );
    amazon(
        store.dvic(dsp)?.all(
            "SELECT transporter_id id,transporter_name name,min(start_date) first,\
             max(start_date) last FROM dvic_inspections GROUP BY 1,2 ORDER BY last DESC",
            [],
        )?,
        &plain,
        false,
    );
    Ok(found
        .into_values()
        .filter(|identity| !identity.names.is_empty())
        .collect())
}

/// The name one ID's sources write now, as `identities` would read it first: Paycom's
/// newest roster, or Amazon's routes, then meal breaks, the scorecard and DVIC.
pub(crate) fn name_of(store: &Store, dsp: &str, (source, id): &Key) -> Result<Option<String>> {
    let first = |db: &Db, sql: &str| -> Result<Option<String>> {
        Ok(db
            .query_as::<(Option<String>,)>(sql, [id])?
            .into_iter()
            .filter_map(|(name,)| name)
            .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|name| !name.is_empty()))
    };
    if *source == DriverSource::Paycom {
        return first(
            &*store.collector(dsp, Provider::Paycom)?,
            "SELECT e.name FROM employees e JOIN publications p ON p.id=e.publication_id \
             WHERE e.code=? ORDER BY p.collected_at DESC,p.id DESC LIMIT 1",
        );
    }
    let routes = first(
        &*store.routedata(dsp)?,
        "SELECT first_name||' '||last_name FROM drivers WHERE transporter_id=?",
    )?;
    if routes.is_some() {
        return Ok(routes);
    }
    let meals = first(
        &*store.collector(dsp, Provider::Cortex)?,
        "SELECT i.driver_name FROM meal_itineraries i JOIN meal_publications p \
         ON p.id=i.publication_id WHERE i.transporter_id=? ORDER BY p.report_date DESC",
    )?;
    if meals.is_some() {
        return Ok(meals);
    }
    let scorecard = first(
        &*store.scorecard(dsp)?,
        "SELECT json_extract(d.row,'$.da_name') FROM driver_scorecards d \
         JOIN scorecard_publications p ON p.id=d.publication_id \
         WHERE d.transporter_id=? ORDER BY p.week DESC",
    )?;
    if scorecard.is_some() {
        return Ok(scorecard);
    }
    first(
        &*store.dvic(dsp)?,
        "SELECT transporter_name FROM dvic_inspections WHERE transporter_id=? \
         ORDER BY start_date DESC",
    )
}

/// How much a source has of one ID, and the last day it has.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seen {
    pub count: usize,
    pub last: Option<String>,
}
/// Everything each ID appears in. Only current publications count, as every page reads.
pub(crate) fn activity(
    store: &Store,
    dsp: &str,
) -> Result<BTreeMap<Key, BTreeMap<DriverData, Seen>>> {
    let mut out: BTreeMap<Key, BTreeMap<DriverData, Seen>> = BTreeMap::new();
    let mut add = |rows: Vec<Value>, source: DriverSource, data: DriverData, week: bool| {
        for row in rows {
            let last = s(&row, "last");
            out.entry((source, s(&row, "id").to_owned()))
                .or_default()
                .insert(
                    data,
                    Seen {
                        count: n(&row, "count") as usize,
                        last: Some(if week {
                            week_end(last)
                        } else {
                            last.to_owned()
                        }),
                    },
                );
        }
    };
    add(
        store.collector(dsp, Provider::Paycom)?.all(
            "SELECT employee_code id,count(DISTINCT date) count,max(date) last \
             FROM timecards WHERE hours>0 GROUP BY employee_code",
            [],
        )?,
        DriverSource::Paycom,
        DriverData::Timecards,
        false,
    );
    add(
        store.routedata(dsp)?.all(
            "SELECT i.transporter_id id,count(DISTINCT i.day) count,max(i.day) last \
             FROM itineraries i JOIN route_publications p ON p.id=i.publication_id \
             AND p.active=1 GROUP BY i.transporter_id",
            [],
        )?,
        DriverSource::Amazon,
        DriverData::Routes,
        false,
    );
    add(
        store.collector(dsp, Provider::Cortex)?.all(
            "SELECT i.transporter_id id,count(DISTINCT p.report_date) count,\
             max(p.report_date) last FROM meal_itineraries i JOIN meal_publications p \
             ON p.id=i.publication_id AND p.active=1 GROUP BY i.transporter_id",
            [],
        )?,
        DriverSource::Amazon,
        DriverData::MealBreaks,
        false,
    );
    add(
        store.dvic(dsp)?.all(
            "SELECT transporter_id id,count(*) count,max(start_date) last \
             FROM dvic_inspections GROUP BY transporter_id",
            [],
        )?,
        DriverSource::Amazon,
        DriverData::Dvic,
        false,
    );
    add(
        store.scorecard(dsp)?.all(
            "SELECT d.transporter_id id,count(DISTINCT p.week) count,max(p.week) last \
             FROM driver_scorecards d JOIN scorecard_publications p \
             ON p.id=d.publication_id AND p.active=1 \
             WHERE COALESCE(d.transporter_id,'')<>'' GROUP BY d.transporter_id",
            [],
        )?,
        DriverSource::Amazon,
        DriverData::Scorecard,
        true,
    );
    Ok(out)
}

/// The days each ID worked, and the days its source was collected for at all: a day
/// outside them says nothing either way.
#[derive(Debug, Default)]
pub(crate) struct Days {
    pub worked: BTreeMap<Key, BTreeSet<String>>,
    pub paycom: BTreeSet<String>,
    pub amazon: BTreeSet<String>,
}
impl Days {
    pub fn of(&self, key: &Key) -> Option<&BTreeSet<String>> {
        self.worked.get(key)
    }
}
/// When Paycom shows hours and when Amazon shows a route, a meal break or an inspection.
pub(crate) fn days(store: &Store, dsp: &str) -> Result<Days> {
    let mut out = Days::default();
    let paycom = store.collector(dsp, Provider::Paycom)?;
    for (code, date) in paycom.query_as::<(String, String)>(
        "SELECT DISTINCT employee_code,date FROM timecards WHERE hours>0",
        [],
    )? {
        out.worked
            .entry((DriverSource::Paycom, code))
            .or_default()
            .insert(date);
    }
    // A period lists every day in it; only the days before it was collected are real.
    for (date,) in paycom.query_as::<(String,)>(
        "SELECT DISTINCT t.date FROM timecards t JOIN publications p ON p.id=t.publication_id \
         WHERE t.date<substr(p.collected_at,1,10)",
        [],
    )? {
        out.paycom.insert(date);
    }
    drop(paycom);
    let routes = store.routedata(dsp)?;
    for (id, day) in routes.query_as::<(String, String)>(
        "SELECT DISTINCT i.transporter_id,i.day FROM itineraries i \
         JOIN route_publications p ON p.id=i.publication_id AND p.active=1",
        [],
    )? {
        out.amazon.insert(day.clone());
        out.worked
            .entry((DriverSource::Amazon, id))
            .or_default()
            .insert(day);
    }
    drop(routes);
    let cortex = store.collector(dsp, Provider::Cortex)?;
    for (id, day) in cortex.query_as::<(String, String)>(
        "SELECT DISTINCT i.transporter_id,p.report_date FROM meal_itineraries i \
         JOIN meal_publications p ON p.id=i.publication_id AND p.active=1",
        [],
    )? {
        out.amazon.insert(day.clone());
        out.worked
            .entry((DriverSource::Amazon, id))
            .or_default()
            .insert(day);
    }
    drop(cortex);
    // An inspection means the driver was out that day, though DVIC alone does not say
    // which days were collected.
    for (id, day) in store.dvic(dsp)?.query_as::<(String, String)>(
        "SELECT DISTINCT transporter_id,start_date FROM dvic_inspections",
        [],
    )? {
        out.worked
            .entry((DriverSource::Amazon, id))
            .or_default()
            .insert(day);
    }
    Ok(out)
}
