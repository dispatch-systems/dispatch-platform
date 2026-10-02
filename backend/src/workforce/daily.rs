use super::{compare, display_name, preferences::preferences};
use crate::{
    Result,
    collectors::Provider,
    contracts::DailyTimecards,
    db::{Db, Store, s},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn visible(row: &Value, p: &Value, drivers: bool) -> bool {
    ["department", "station"]
        .iter()
        .all(|key| p[*key].is_null() || s(p, key).is_empty() || row[*key] == p[*key])
        && (!drivers
            || p["driver_departments"].is_null()
            || p["driver_departments"]
                .as_array()
                .is_some_and(|a| a.contains(&row["department"])))
}
pub(crate) fn cards(db: &Db, sql: &str, p: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut rows = db.all(sql, p)?;
    for row in &mut rows {
        row["punches"] = serde_json::from_str(s(row, "punches"))?;
    }
    Ok(rows)
}
impl Store {
    /// Overlay completed employee pages from the current guarded attempt.
    pub fn daily_source(
        &self,
        id: &str,
        date: &str,
    ) -> Result<(Option<Value>, Vec<Value>, Vec<Value>)> {
        let source = self
            .daily_sources(id, date, date, None)?
            .remove(date)
            .unwrap();
        Ok((
            source.publication,
            source.roster.values().cloned().collect(),
            source.rows.into_values().collect(),
        ))
    }
    pub fn daily(&self, id: &str, date: &str, sort: &str, desc: bool) -> Result<DailyTimecards> {
        Ok(self
            .daily_range(id, date, date, sort, desc, None)?
            .remove(date)
            .unwrap())
    }
    /// Loads publications, employee syncs and guarded live pages once for the
    /// period, filtering requested employees before parsing their cards.
    pub fn daily_range(
        &self,
        id: &str,
        from: &str,
        to: &str,
        sort: &str,
        desc: bool,
        codes: Option<&[String]>,
    ) -> Result<BTreeMap<String, DailyTimecards>> {
        let db = self.collector(id, Provider::Paycom)?;
        let settings = preferences(&db)?;
        let p = &settings["values"];
        self.daily_sources(id, from, to, codes)?
            .into_iter()
            .map(|(date, source)| Ok((date, display_daily(source, p, sort, desc)?)))
            .collect()
    }
}
fn display_daily(
    source: super::range::DailySource,
    p: &Value,
    sort: &str,
    desc: bool,
) -> Result<DailyTimecards> {
    let publication = source.publication;
    let available = source.available;
    let mut rows: Vec<Value> = source.rows.into_values().collect();
    rows.retain(|r| visible(r, p, true));
    for row in &mut rows {
        row["name"] = json!(display_name(s(row, "name"), s(p, "name_order")));
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
