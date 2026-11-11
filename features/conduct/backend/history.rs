//! Bounded history and date-specific drafting, resolving merged directory codes before filtering.
use super::{categories, overview, standing, writeups};
use crate::api::types::{
    ConductContext, ConductHistory, ConductImpact, ConductStepDown, ConductWriteupRow,
};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{FromRow, Row, Store},
    ensure,
    manifest::people::{self, Everyone},
};
use rusqlite::{params, params_from_iter, types::Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Query<'a> {
    pub person: &'a str,
    pub search: &'a str,
    pub category: &'a str,
    pub tier: &'a str,
    pub status: &'a str,
    pub after: &'a str,
    pub limit: usize,
}

struct Dated {
    row: ConductWriteupRow,
    created: String,
}
impl FromRow for Dated {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            row: ConductWriteupRow::from_row(row)?,
            created: row.get("created_at")?,
        })
    }
}

/// Matching stored identities, including aliases now held by the same directory person.
fn identities(
    store: &Store,
    dsp: &str,
    everyone: &Everyone,
    person: &str,
    search: &str,
) -> Result<Vec<String>> {
    let rows: Vec<(String, String)> = store.dsp(dsp)?.query_as(
        "SELECT DISTINCT person,person_name FROM conduct_writeups",
        [],
    )?;
    let search = search.trim().to_lowercase();
    Ok(rows
        .into_iter()
        .filter_map(|(code, name)| {
            let current = overview::now(everyone, &code, &name);
            ((person.is_empty() || current.code == person)
                && current.name.to_lowercase().contains(&search))
            .then_some(code)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn matching(
    store: &Store,
    dsp: &str,
    everyone: &Everyone,
    person: &str,
    search: &str,
) -> Result<(String, Vec<Value>)> {
    if person.is_empty() && search.trim().is_empty() {
        return Ok(("1".into(), Vec::new()));
    }
    let codes = identities(store, dsp, everyone, person, search)?;
    if codes.is_empty() {
        return Ok(("0".into(), Vec::new()));
    }
    let slots = vec!["?"; codes.len()].join(",");
    Ok((
        format!("w.person IN ({slots})"),
        codes.into_iter().map(Value::Text).collect(),
    ))
}

pub(crate) fn read(
    store: &Store,
    state: &State,
    dsp: &str,
    query: Query<'_>,
) -> Result<ConductHistory> {
    let everyone = people::everyone(store, state, dsp)?;
    read_from(store, dsp, &everyone, query)
}

fn read_from(
    store: &Store,
    dsp: &str,
    everyone: &Everyone,
    query: Query<'_>,
) -> Result<ConductHistory> {
    let (mut filter, mut values) = matching(store, dsp, everyone, query.person, query.search)?;
    for (column, value) in [("category_id", query.category), ("tier", query.tier)] {
        if !value.is_empty() {
            filter.push_str(&format!(" AND w.{column}=?"));
            values.push(value.to_owned().into());
        }
    }
    match query.status {
        "signed" => filter.push_str(" AND w.signed_on IS NOT NULL"),
        "waiting" => filter.push_str(" AND w.signed_on IS NULL"),
        _ => {}
    }
    if !query.after.is_empty() {
        let (date, created, id): (String, String, String) =
            serde_json::from_str(query.after).map_err(|_| Error::new("invalid_input", 400))?;
        filter.push_str(" AND (w.written_on<? OR (w.written_on=? AND (w.created_at<? OR (w.created_at=? AND w.id>?))))");
        values.extend(
            [date.clone(), date, created.clone(), created, id]
                .into_iter()
                .map(Value::Text),
        );
    }
    values.push(Value::Integer((query.limit + 1) as i64));
    let mut rows: Vec<Dated> = store.dsp(dsp)?.query_as(
        &format!(
            "{} WHERE {filter} ORDER BY w.written_on DESC,w.created_at DESC,w.id LIMIT ?",
            writeups::ROWS
        ),
        params_from_iter(values),
    )?;
    let more = rows.len() > query.limit;
    rows.truncate(query.limit);
    let next_cursor = if more {
        rows.last()
            .map(|w| serde_json::to_string(&(&w.row.written_on, &w.created, &w.row.id)))
            .transpose()?
    } else {
        None
    };
    let writeups = rows
        .into_iter()
        .map(|mut w| {
            w.row.person = overview::now(everyone, &w.row.person, &w.row.person_name).code;
            w.row
        })
        .collect();
    Ok(ConductHistory {
        writeups,
        next_cursor,
    })
}

pub(crate) fn context(
    store: &Store,
    state: &State,
    dsp: &str,
    person: &str,
    category: &str,
    date: &str,
    edit: &str,
) -> Result<ConductContext> {
    let everyone = people::everyone(store, state, dsp)?;
    context_from(store, dsp, &everyone, person, category, date, edit)
}

fn context_from(
    store: &Store,
    dsp: &str,
    everyone: &Everyone,
    person: &str,
    category: &str,
    date: &str,
    edit: &str,
) -> Result<ConductContext> {
    let on =
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    let (filter, mut values) = matching(store, dsp, everyone, person, "")?;
    values.extend(
        [category, date, edit]
            .into_iter()
            .map(|s| Value::Text(s.into())),
    );
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        let kind = categories::one(&db, category)?;
        let cutoff = if edit.is_empty() { None } else {
            db.one_as::<(String, String)>("SELECT written_on,created_at FROM conduct_writeups WHERE id=?", [edit])?
        };
        let dated: Vec<Dated> = db.query_as(
            &format!("{} WHERE {filter} AND w.category_id=? AND w.written_on<=? AND w.id!=? ORDER BY w.written_on DESC,w.created_at DESC,w.id", writeups::ROWS),
            params_from_iter(values),
        )?;
        let rows: Vec<_> = dated.into_iter().filter(|w| cutoff.as_ref().is_none_or(|(_, created)|
            w.row.written_on.as_str() < date || w.created < *created || (w.created == *created && w.row.id.as_str() > edit)
        )).map(|mut w| { w.row.person = person.to_owned(); w.row }).collect();
        let worked = standing::work_out(&[kind], &rows, on);
        let held = worked.standings.first().map(|s| s.tier);
        let mut earlier = Vec::new();
        if held.is_some() {
            for row in rows {
                let clean_before = !worked.stood_before.contains_key(&row.id);
                earlier.push(row);
                if clean_before { break; }
            }
        }
        earlier.reverse();
        Ok(ConductContext { standing: held, earlier })
    })
}

pub(crate) fn impact(
    store: &Store,
    state: &State,
    c: &Context,
    id: &str,
    revision: i64,
    step: Option<ConductStepDown>,
) -> Result<Vec<ConductImpact>> {
    let db = store.dsp(&c.dsp.id)?;
    db.transaction(|| {
        let mut category = categories::one(&db, id)?;
        ensure(
            category.revision == revision,
            "conduct_category_changed",
            409,
        )?;
        let everyone = people::everyone(store, state, &c.dsp.id)?;
        let mut rows: Vec<ConductWriteupRow> = db.query_as(
            &format!(
                "{} WHERE w.category_id=? ORDER BY w.written_on DESC,w.created_at DESC,w.id",
                writeups::ROWS
            ),
            params![id],
        )?;
        for row in &mut rows {
            row.person = overview::now(&everyone, &row.person, &row.person_name).code;
        }
        let today = NaiveDate::parse_from_str(&overview::today(c)?, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?;
        let before = standing::work_out(&[category.clone()], &rows, today).standings;
        category.step_down = step;
        let after = standing::work_out(&[category], &rows, today).standings;
        let mut changes: BTreeMap<_, _> = before
            .into_iter()
            .map(|s| {
                (
                    s.person.clone(),
                    ConductImpact {
                        person: s.person.clone(),
                        before: Some(s),
                        after: None,
                    },
                )
            })
            .collect();
        for s in after {
            let person = s.person.clone();
            let change = changes.entry(person.clone()).or_insert(ConductImpact {
                person,
                before: None,
                after: None,
            });
            change.after = Some(s);
        }
        Ok(changes
            .into_values()
            .filter(|change| change.before != change.after)
            .collect())
    })
}

#[cfg(test)]
#[path = "../tests/backend/history.rs"]
mod tests;
