//! What a connection's queries answer with: who each kind of data names (`Named`), how much it
//! holds of each (`Records`) and when they worked (`Workdays`), each read from a query by its
//! columns' names. A query naming a column none of these reads is refused, so a misspelt one
//! fails the app's people test rather than going unread.
use crate::{Error, Result, db::Db};
use serde_json::Value;

/// An ID as one kind of data names it. `named` reads one from each row of a query by its
/// columns' names: `id` and `name`, and any of `first` and `last` (the days it saw them),
/// `listed` (on its source's current roster, 1), `email`, `phone`, `roles` (a JSON list),
/// `department`, `position` and `photo` (its version).
#[derive(Clone, Debug, Default)]
pub struct Named {
    pub id: String,
    /// As written, spaces and all.
    pub name: String,
    pub department: Option<String>,
    pub position: Option<String>,
    /// The first and last days it saw them.
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On its source's current roster.
    pub listed: bool,
    /// The email addresses and phone numbers it has for them, as written.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// What its source says they do: `Driver`, `Helper`.
    pub roles: Vec<String>,
    /// The version of the photo it holds of them, which changes when the photo does.
    pub photo: Option<String>,
    /// The source's other IDs for the same person, as their account in each of several
    /// workspaces: Driver Match keeps them all with one person.
    pub same_as: Vec<String>,
}
impl Named {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ..Self::default()
        }
    }
    /// One row of a query, by its columns' names.
    fn from_row(row: &Value) -> Self {
        let text = |key: &str| {
            row[key]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Self {
            id: text("id").unwrap_or_default(),
            name: text("name").unwrap_or_default(),
            department: text("department"),
            position: text("position"),
            first_seen: text("first"),
            last_seen: text("last"),
            listed: row["listed"].as_i64() == Some(1) || row["listed"] == true,
            emails: text("email").into_iter().collect(),
            phones: text("phone").into_iter().collect(),
            roles: text("roles")
                .and_then(|roles| serde_json::from_str(&roles).ok())
                .unwrap_or_default(),
            photo: text("photo"),
            same_as: vec![],
        }
    }
    /// Extends the days it saw them by a first and last day; an empty one says nothing.
    pub fn seen(&mut self, first: &str, last: &str) {
        if !first.is_empty() && self.first_seen.as_deref().is_none_or(|f| first < f) {
            self.first_seen = Some(first.to_owned());
        }
        if !last.is_empty() && self.last_seen.as_deref().is_none_or(|l| last > l) {
            self.last_seen = Some(last.to_owned());
        }
    }
}
/// The columns `named` reads, the first two of which every query names.
const NAMED: &[&str] = &[
    "id",
    "name",
    "first",
    "last",
    "listed",
    "email",
    "phone",
    "roles",
    "department",
    "position",
    "photo",
];
/// The columns `records` reads, every one of which a query names.
const RECORDS: &[&str] = &["id", "count", "last"];
/// Refuses a query answering with a column `known` lacks, or without one of its first
/// `needed`, so a misspelt column fails the app's people test rather than going unread.
fn columns(db: &Db, sql: &str, known: &[&str], needed: usize) -> Result<()> {
    let columns = db.columns(sql)?;
    let fits = columns.iter().all(|c| known.contains(&c.as_str()))
        && known[..needed]
            .iter()
            .all(|k| columns.iter().any(|c| c == k));
    if fits {
        return Ok(());
    }
    Err(Error::caused(
        "people_columns",
        500,
        format!(
            "a people query answers {columns:?}; it reads {known:?}, the first {needed} always"
        ),
    ))
}
/// Everyone a query names, a row each, as `Named` reads them.
pub fn named(db: &Db, sql: &str) -> Result<Vec<Named>> {
    columns(db, sql, NAMED, 2)?;
    Ok(db.all(sql, [])?.iter().map(Named::from_row).collect())
}

/// How much one kind of data holds of an ID, in its unit, and the last day it has.
#[derive(Clone, Debug)]
pub struct Records {
    pub id: String,
    pub count: usize,
    pub last: String,
}
/// How much a query counts of each ID, from its columns `id`, `count` and `last`.
pub fn records(db: &Db, sql: &str) -> Result<Vec<Records>> {
    columns(db, sql, RECORDS, RECORDS.len())?;
    Ok(db
        .all(sql, [])?
        .iter()
        .map(|row| Records {
            id: crate::db::s(row, "id").trim().to_owned(),
            count: crate::db::n(row, "count").max(0) as usize,
            last: crate::db::s(row, "last").to_owned(),
        })
        .collect())
}
/// The columns `worked` reads, both of which a query names.
const WORKED: &[&str] = &["id", "day"];
/// Each ID a query names with a day it worked, from its columns `id` and `day`.
pub fn worked(db: &Db, sql: &str) -> Result<Vec<(String, String)>> {
    columns(db, sql, WORKED, WORKED.len())?;
    Ok(db
        .all(sql, [])?
        .iter()
        .map(|row| {
            (
                crate::db::s(row, "id").trim().to_owned(),
                crate::db::s(row, "day").to_owned(),
            )
        })
        .collect())
}

/// When one kind of data saw people at work.
#[derive(Debug, Default)]
pub struct Workdays {
    /// Each ID with a day it worked.
    pub worked: Vec<(String, String)>,
    /// The days it was collected for, so that a day outside them says nothing.
    pub collected: Vec<String>,
}
impl Workdays {
    /// Data collected for each day it holds, as a day's routes are: whoever it names on a
    /// day worked it, and every day it holds was collected.
    pub fn daily(worked: Vec<(String, String)>) -> Self {
        let collected = worked.iter().map(|(_, day)| day.clone()).collect();
        Self { worked, collected }
    }
}
