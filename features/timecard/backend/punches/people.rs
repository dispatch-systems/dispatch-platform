//! Paycom's employees, as Driver Match reads them: each as the newest roster lists them,
//! seen on the days they worked.
use super::preferences::preferences;
use dispatch_core::{
    Result,
    db::{Store, n, s},
    manifest::people::{self, Appearances, Named, People, Workdays},
    mcp::api::types::{DriverData, DriverSource},
};
use dispatch_paycom as paycom;
use std::collections::{BTreeMap, BTreeSet};

pub struct Employees;
impl People for Employees {
    fn data(&self) -> DriverData {
        DriverData::Timecards
    }
    fn source(&self) -> DriverSource {
        DriverSource::Paycom
    }
    fn order(&self) -> u16 {
        10
    }
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>> {
        let paycom = store.collector(dsp, paycom::PROVIDER)?;
        let mut found: BTreeMap<String, Named> = BTreeMap::new();
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
            let entry = found.entry(id.to_owned()).or_insert_with(|| Named::new(id));
            entry.name = s(&row, "name").to_owned();
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
            if let Some(entry) = found.get_mut(s(&row, "code")) {
                entry.first_seen = Some(s(&row, "first").to_owned());
                entry.last_seen = Some(s(&row, "last").to_owned());
            }
        }
        Ok(found.into_values().collect())
    }
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>> {
        people::first_name(
            &*store.collector(dsp, paycom::PROVIDER)?,
            "SELECT e.name FROM employees e JOIN publications p ON p.id=e.publication_id \
             WHERE e.code=? ORDER BY p.collected_at DESC,p.id DESC LIMIT 1",
            id,
        )
    }
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>> {
        let rows = store.collector(dsp, paycom::PROVIDER)?.all(
            "SELECT employee_code id,count(DISTINCT date) count,max(date) last \
             FROM timecards WHERE hours>0 GROUP BY employee_code",
            [],
        )?;
        Ok(rows
            .iter()
            .map(|row| Appearances {
                id: s(row, "id").to_owned(),
                count: n(row, "count") as usize,
                last: s(row, "last").to_owned(),
            })
            .collect())
    }
    /// When Paycom shows hours, and the days its periods were collected for.
    fn workdays(&self, store: &Store, dsp: &str) -> Result<Workdays> {
        let paycom = store.collector(dsp, paycom::PROVIDER)?;
        let worked = paycom.query_as::<(String, String)>(
            "SELECT DISTINCT employee_code,date FROM timecards WHERE hours>0",
            [],
        )?;
        // A period lists every day in it; only the days before it was collected are real.
        let collected = paycom
            .query_as::<(String,)>(
                "SELECT DISTINCT t.date FROM timecards t JOIN publications p ON p.id=t.publication_id \
                 WHERE t.date<substr(p.collected_at,1,10)",
                [],
            )?
            .into_iter()
            .map(|(date,)| date)
            .collect();
        Ok(Workdays { worked, collected })
    }
    fn driver_departments(&self, store: &Store, dsp: &str) -> Result<Option<BTreeSet<String>>> {
        let paycom = store.collector(dsp, paycom::PROVIDER)?;
        Ok(preferences(&paycom)?["values"]["driver_departments"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|d| d.as_str().map(str::to_owned))
                    .collect()
            }))
    }
}
