use super::preferences::preferences;
use crate::api::types::EmployeesResponse;
use dispatch_core::{
    Result,
    db::{Store, boolean, s},
    foundation::names::display,
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
    let direction = if desc { "DESC" } else { "ASC" };
    // Paycom sends both LAST, FIRST and First Last. Search and pagination use the
    // same first-name order as the directory, preserving compound surnames.
    let name = "CASE WHEN instr(name,',')>0 THEN trim(substr(name,instr(name,',')+1))||' '||trim(substr(name,1,instr(name,',')-1)) ELSE name END";
    let condition = format!(
        "publication_id=?1 AND (?2='' OR department=?2) AND (?3='' OR \
        station=?3) AND (?4='' OR instr(dispatch_lower(({name})||' '||code),?4)>0) \
        AND (?5 IS NULL OR active=?5)"
    );
    let total = db.count(
        &format!("SELECT count(*) FROM employees WHERE {condition}"),
        params![
            publication_id,
            s(p, "department"),
            s(p, "station"),
            query.to_lowercase(),
            active
        ],
    )?;
    let mut rows = db.all(
        &format!(
            "SELECT code,({name}) \
        name,department,position,station,active FROM employees WHERE {condition} ORDER \
        BY name COLLATE dispatch_unicode {direction},code COLLATE dispatch_unicode \
        {direction} LIMIT ?6 OFFSET \
        ?7"
        ),
        params![
            publication_id,
            s(p, "department"),
            s(p, "station"),
            query.to_lowercase(),
            active,
            limit.map_or(-1, |value| value as i64),
            offset as i64
        ],
    )?;
    for row in &mut rows {
        boolean(row, &["active"]);
        row["name"] = json!(display(s(row, "name")));
    }
    Ok(serde_json::from_value(
        json!({"employees":rows,"total":total,"collectedAt":publication["collected_at"]}),
    )?)
}
