//! Driver Match's logic: one code for each person the DSP's features name, whatever each
//! source calls them. Collected rows keep their sources' IDs; `person_ids` leads every ID
//! to its person. New IDs get a person after every collection, joining someone only on a
//! certain match; anything less waits for a decision on its page. It reads only the
//! features the DSP has on. It is private to the crate: `feature.rs` re-exports what the app
//! uses.
mod assign;
mod audit;
mod codes;
mod contacts;
mod decisions;
mod details;
mod directory;
mod gather;
pub(crate) mod maintenance;
mod matching;
mod names;
mod overview;
mod pairs;
mod person;
mod photos;
mod review;
mod sources;

pub(crate) use audit::name_driver_events;
pub(crate) use directory::Directory;

use crate::api::types::{DriverMatch, IdLink, MatchCounts, PersonDetails, PersonTitle};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, FromRow, Row, Store},
    manifest::people::Source,
    server::cache::DataDomain,
};
use sources::Key;
use std::collections::BTreeSet;

/// Driver codes, the decisions about them and the IDs they lead to: core's own domain, as
/// every feature showing people reads them.
pub const DOMAIN: DataDomain = DataDomain::DRIVERS;
/// When collected data was last checked for new IDs.
const CHECKED: &str = "driver_match.checked_at";
/// How long someone no source lists can go unseen before they count as having left: days
/// before the newest day anything was collected for.
const FORMER_AFTER_DAYS: i64 = 21;

/// What one pass reads: every ID the DSP's collections hold now, and the sources that only
/// list people the DSP can reach. Reading takes the shared lock; `write_pass` then writes in
/// one short step.
pub struct Collected {
    found: sources::Found,
    contacts_only: BTreeSet<Source>,
}

/// A row of `person_ids`: an ID, its person and how it came to them. Nothing a source
/// says about the person is kept here; that stays in the source's own database. Its source
/// is kept as written, as a feature no longer in the build may have named it.
pub(crate) struct IdRow {
    source: String,
    external_id: String,
    code: String,
    linked_by: IdLink,
    linked_at: String,
    actor_id: Option<String>,
}
impl FromRow for IdRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            source: row.get("source")?,
            external_id: row.get("external_id")?,
            code: row.get("code")?,
            linked_by: row.get("linked_by")?,
            linked_at: row.get("linked_at")?,
            actor_id: row.get("actor_id")?,
        })
    }
}
impl IdRow {
    /// Its source, while a feature in this build declares it.
    fn source(&self) -> Option<Source> {
        Source::parse(&self.source)
    }
    fn key(&self) -> Option<Key> {
        Some((self.source()?, self.external_id.clone()))
    }
}

/// What Driver Match reads and writes for a DSP: its codes, the decisions about them and the
/// IDs they lead to.
pub trait DriverMatchStore {
    /// Reads what a pass needs, without writing: run it under the shared lock.
    fn read_pass(&self, dsp: &str) -> Result<Collected>;
    /// Writes what a pass found: a person for every new ID. Answers how many IDs were new.
    fn write_pass(&self, dsp: &str, collected: Collected) -> Result<usize>;
    /// The Driver Match page: the questions waiting, then every person with a code.
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch>;
    /// How many questions wait, for the sidebar's count, without the roster.
    fn match_counts(&self, dsp: &str) -> Result<MatchCounts>;
    fn person_details(&self, dsp: &str, code: &str) -> Result<PersonDetails>;
    fn merge_people(&self, c: &Context, code: &str, into: &str) -> Result<()>;
    fn split_off(&self, c: &Context, code: &str, source: Source, id: &str) -> Result<()>;
    fn keep_apart(&self, c: &Context, code: &str, other: &str) -> Result<()>;
    /// Confirms one of `code`'s IDs is theirs, as a match only suggested it.
    fn confirm_link(&self, c: &Context, code: &str, source: Source, id: &str) -> Result<()>;
    /// Gives `code` a title, or with `None` takes theirs away.
    fn set_title(&self, c: &Context, code: &str, title: Option<PersonTitle>) -> Result<()>;
}
impl DriverMatchStore for Store {
    fn read_pass(&self, dsp: &str) -> Result<Collected> {
        assign::read(self, dsp)
    }
    fn write_pass(&self, dsp: &str, collected: Collected) -> Result<usize> {
        assign::write(self, dsp, collected)
    }
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch> {
        overview::driver_match(self, dsp)
    }
    fn match_counts(&self, dsp: &str) -> Result<MatchCounts> {
        overview::counts(self, dsp)
    }
    fn person_details(&self, dsp: &str, code: &str) -> Result<PersonDetails> {
        details::details(self, dsp, code)
    }
    fn merge_people(&self, c: &Context, code: &str, into: &str) -> Result<()> {
        decisions::merge(self, c, code, into)
    }
    fn split_off(&self, c: &Context, code: &str, source: Source, id: &str) -> Result<()> {
        decisions::split(self, c, code, source, id)
    }
    fn keep_apart(&self, c: &Context, code: &str, other: &str) -> Result<()> {
        decisions::keep_apart(self, c, code, other)
    }
    fn confirm_link(&self, c: &Context, code: &str, source: Source, id: &str) -> Result<()> {
        decisions::confirm(self, c, code, source, id)
    }
    fn set_title(&self, c: &Context, code: &str, title: Option<PersonTitle>) -> Result<()> {
        decisions::title(self, c, code, title)
    }
}

/// Every ID a person still holds, as they came: the IDs of people merged into others lead
/// to those others.
pub(super) fn current_ids(db: &Db) -> Result<Vec<IdRow>> {
    db.query_as(
        "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
         WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.source,i.external_id",
        [],
    )
}

/// The code a merged person's IDs went to, followed to the end.
pub(super) fn surviving(db: &Db, code: &str) -> Result<String> {
    let mut code = code.to_owned();
    for _ in 0..32 {
        let row = db
            .query_as::<(Option<String>,)>("SELECT merged_into FROM people WHERE code=?", [&code])?
            .into_iter()
            .next()
            .ok_or_else(|| Error::new("not_found", 404))?;
        match row.0 {
            Some(next) => code = next,
            None => return Ok(code),
        }
    }
    Err(Error::new("not_found", 404))
}
