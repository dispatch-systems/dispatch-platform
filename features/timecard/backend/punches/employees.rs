use super::preferences::preferences;
use crate::api::types::EmployeesResponse;
use dispatch_core::{
    Result,
    db::{Store, boolean, s},
    foundation::names::{compare, display},
};
use dispatch_paycom as paycom;
use rusqlite::params;
use serde_json::json;
pub(crate) fn timecard_employees(
    store: &Store,
    id: &str,
    query: &str,
    offset: usize,
    limit: Option<usize>,
    desc: bool,
    active: Option<bool>,
) -> Result<EmployeesResponse> {
    let db = store.collector(id, paycom::PROVIDER)?;
    let settings = preferences(&db)?;
    let p = &settings["values"];
    let Some(publication) = db.one(
        "SELECT id,collected_at FROM publications WHERE active=1",
        [],
    )?
    else {
        return Ok(EmployeesResponse {
            employees: vec![],
            total: 0,
            collected_at: None,
        });
    };
    let publication_id = s(&publication, "id");
    let mut rows = db.all(
        "SELECT code,name,department,position,station,active FROM employees WHERE \
         publication_id=?1 AND (?2='' OR department=?2) AND (?3='' OR station=?3) \
         AND (?4 IS NULL OR active=?4)",
        params![publication_id, s(p, "department"), s(p, "station"), active],
    )?;
    for row in &mut rows {
        boolean(row, &["active"]);
        row["name"] = json!(display(s(row, "name")));
    }
    // Normalize before searching, ordering and paging so every operation sees exactly
    // the displayed first/last name, including spacing, compound surnames and suffixes.
    let query = query.to_lowercase();
    rows.retain(|row| {
        query.is_empty()
            || format!("{} {}", s(row, "name"), s(row, "code"))
                .to_lowercase()
                .contains(&query)
    });
    rows.sort_by(|a, b| {
        let order =
            compare(s(a, "name"), s(b, "name")).then_with(|| compare(s(a, "code"), s(b, "code")));
        if desc { order.reverse() } else { order }
    });
    let total = rows.len();
    let rows: Vec<_> = rows
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    Ok(serde_json::from_value(
        json!({"employees":rows,"total":total,"collectedAt":publication["collected_at"]}),
    )?)
}
