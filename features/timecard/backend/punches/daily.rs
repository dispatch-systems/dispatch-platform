use crate::{
    api::types::DailyTimecards,
    backend::{
        TimecardStore,
        settings::{Listing, ROSTER, driver_departments},
    },
};
use dispatch_core::{
    Result,
    db::{Db, Store, s},
    foundation::names::{compare, display},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) const CACHED: &str = "timecard.daily";
/// Who the Timecard tab lists, as the DSP's settings say: the employees its roster settings
/// list, of the departments that drive.
struct Listed {
    roster: Listing,
    drivers: Option<Vec<String>>,
}
impl Listed {
    fn read(store: &Store, id: &str) -> Result<Self> {
        Ok(Self {
            roster: dispatch_core::settings::read(store, id, &ROSTER)?,
            drivers: driver_departments(store, id)?,
        })
    }
    fn lists(&self, row: &Value) -> bool {
        let department = s(row, "department");
        self.roster.lists(department, s(row, "station"))
            && (self.drivers.as_ref()).is_none_or(|drivers| drivers.iter().any(|d| d == department))
    }
}
pub(crate) fn cards(db: &Db, sql: &str, p: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut rows = db.all(sql, p)?;
    for row in &mut rows {
        row["punches"] = serde_json::from_str(s(row, "punches"))?;
    }
    Ok(rows)
}
/// Overlay completed employee pages from the current guarded attempt.
pub(crate) fn daily_timecards_source(
    store: &Store,
    id: &str,
    date: &str,
) -> Result<(Option<Value>, Vec<Value>, Vec<Value>)> {
    let source = super::range::daily_sources(store, id, date, date, None)?
        .remove(date)
        .unwrap();
    Ok((
        source.publication,
        source.roster.values().cloned().collect(),
        source.rows.into_values().collect(),
    ))
}
pub(crate) fn daily_timecards(
    store: &Store,
    id: &str,
    date: &str,
    sort: &str,
    desc: bool,
) -> Result<DailyTimecards> {
    Ok(store
        .daily_timecards_range(id, date, date, sort, desc, None)?
        .remove(date)
        .unwrap())
}
/// Loads publications, employee syncs and guarded live pages once for the
/// period, filtering requested employees before parsing their cards.
pub(crate) fn daily_timecards_range(
    store: &Store,
    id: &str,
    from: &str,
    to: &str,
    sort: &str,
    desc: bool,
    codes: Option<&[String]>,
) -> Result<BTreeMap<String, DailyTimecards>> {
    let listed = Listed::read(store, id)?;
    super::range::daily_sources(store, id, from, to, codes)?
        .into_iter()
        .map(|(date, source)| Ok((date, display_daily(source, &listed, sort, desc)?)))
        .collect()
}
fn display_daily(
    source: super::range::DailySource,
    listed: &Listed,
    sort: &str,
    desc: bool,
) -> Result<DailyTimecards> {
    let publication = source.publication;
    let available = source.available;
    let mut rows: Vec<Value> = source.rows.into_values().collect();
    rows.retain(|row| listed.lists(row));
    for row in &mut rows {
        row["name"] = json!(display(s(row, "name")));
        row.as_object_mut().unwrap().remove("department");
        row.as_object_mut().unwrap().remove("station");
    }
    rows.sort_by(|a, b| {
        let x = sort_key(a, sort);
        let y = sort_key(b, sort);
        let ord = if x.is_number() {
            x.as_f64()
                .unwrap_or(0.)
                .total_cmp(&y.as_f64().unwrap_or(0.))
        } else {
            compare(x.as_str().unwrap_or(""), y.as_str().unwrap_or(""))
        };
        (if desc { ord.reverse() } else { ord })
            .then_with(|| compare(s(a, "employeeCode"), s(b, "employeeCode")))
    });
    Ok(serde_json::from_value(
        json!({"rows":rows,"collectedAt":publication.map(|p|p["collected_at"].clone()),"available":available}),
    )?)
}
fn sort_key<'a>(row: &'a Value, sort: &str) -> &'a Value {
    let p = row["punches"].as_array();
    match sort {
        "name" => &row["name"],
        "hours" | "totalHours" => &row["hours"],
        "condition" => &row["status"],
        "inDay" => p
            .and_then(|p| p.first())
            .map(|r| &r["in"])
            .unwrap_or(&Value::Null),
        "outDay" => p
            .and_then(|p| p.last())
            .map(|r| &r["out"])
            .unwrap_or(&Value::Null),
        "outLunch" if p.is_some_and(|p| p.len() > 1) => &row["punches"][0]["out"],
        "inLunch" if p.is_some_and(|p| p.len() > 1) => &row["punches"][1]["in"],
        _ => &Value::Null,
    }
}
