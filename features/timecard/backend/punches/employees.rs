use super::preferences::preferences;
use crate::api::types::{Employee, EmployeesResponse};
use dispatch_core::{
    Result,
    db::{Store, boolean, s},
    foundation::names::{compare, display},
    manifest::people,
};
use dispatch_paycom as paycom;
use rusqlite::params;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
pub(crate) fn timecard_employees(
    store: &Store,
    id: &str,
    query: &str,
    offset: usize,
    limit: Option<usize>,
    desc: bool,
    active: Option<bool>,
) -> Result<EmployeesResponse> {
    Ok(roster(store, id)?.page(query, offset, limit, desc, active))
}

/// Normalized names and their stable Unicode ordering, shared by every page and search.
#[derive(Serialize)]
pub(crate) struct Roster {
    employees: Vec<Employee>,
    search: Vec<String>,
    collected_at: Option<String>,
}
pub(crate) const CACHED: &str = "timecard.employees";
pub(crate) fn roster(store: &Store, id: &str) -> Result<Roster> {
    let db = store.collector(id, paycom::PROVIDER)?;
    let settings = preferences(&db)?;
    let p = &settings["values"];
    let Some(publication) = db.one(
        "SELECT id,collected_at FROM publications WHERE active=1",
        [],
    )?
    else {
        return Ok(Roster {
            employees: vec![],
            search: vec![],
            collected_at: None,
        });
    };
    let mut rows = db.all(
        "SELECT code,name,department,position,station,active FROM employees WHERE \
         publication_id=?1 AND (?2='' OR department=?2) AND (?3='' OR station=?3)",
        params![s(&publication, "id"), s(p, "department"), s(p, "station")],
    )?;
    for row in &mut rows {
        boolean(row, &["active"]);
        row["name"] = json!(display(s(row, "name")));
    }
    let mut employees: Vec<Employee> = serde_json::from_value(json!(rows))?;
    let mut photos = photos(store, id)?;
    for employee in &mut employees {
        employee.photo = photos.remove(&employee.code);
    }
    employees.sort_by(|a, b| compare(&a.name, &b.name).then_with(|| compare(&a.code, &b.code)));
    let search = employees
        .iter()
        .map(|row| format!("{} {}", row.name, row.code).to_lowercase())
        .collect();
    Ok(Roster {
        employees,
        search,
        collected_at: publication["collected_at"].as_str().map(str::to_owned),
    })
}
impl Roster {
    pub fn page(
        &self,
        query: &str,
        offset: usize,
        limit: Option<usize>,
        desc: bool,
        active: Option<bool>,
    ) -> EmployeesResponse {
        let query = query.to_lowercase();
        let mut indices: Vec<_> = self
            .employees
            .iter()
            .enumerate()
            .filter(|(index, row)| {
                active.is_none_or(|active| active == row.active)
                    && self.search[*index].contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        if desc {
            indices.reverse();
        }
        let total = indices.len();
        EmployeesResponse {
            employees: indices
                .into_iter()
                .skip(offset)
                .take(limit.unwrap_or(usize::MAX))
                .map(|i| self.employees[i].clone())
                .collect(),
            total,
            collected_at: self.collected_at.clone(),
        }
    }
}

/// The version of each employee's photo, by their code, of those whose person has one.
pub(super) fn photos(store: &Store, dsp: &str) -> Result<BTreeMap<String, String>> {
    Ok(people::who(store, dsp, dispatch_paycom::SOURCE)?
        .into_iter()
        .filter_map(|(code, person)| Some((code, person.photo?)))
        .collect())
}
