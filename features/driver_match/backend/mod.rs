//! Driver Match's logic: one code for each person the DSP's features name, whatever each
//! source calls them. Collected rows keep their sources' IDs; `person_ids` leads every ID
//! to its person. New IDs get a person after every collection, joining someone only on a
//! certain match; anything less waits for a decision in Settings. It reads only the
//! features the DSP has on. It is private to the crate: `feature.rs` re-exports what the app
//! uses.
mod assign;
mod audit;
mod contacts;
mod decisions;
mod details;
mod directory;
mod legacy;
pub(crate) mod maintenance;
mod matching;
mod names;
mod overview;
mod pairs;
mod person;
mod review;
mod sources;

pub(crate) use audit::name_driver_events;
pub(crate) use directory::Directory;

use crate::api::types::{DriverCounts, DriverDetails, DriverLink, DriverMatch};
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, FromRow, Row, Store},
    manifest::people::{DriverSource, Photo},
    server::cache::DataDomain,
};
use legacy::Saved;
use sources::Key;

/// Driver codes, the decisions about them and the IDs they lead to: core's own domain, as
/// every feature showing people reads them.
pub const DOMAIN: DataDomain = DataDomain::DRIVERS;
/// When collected data was last checked for new IDs.
const CHECKED: &str = "driver_match.checked_at";
/// Letters and digits that cannot be mistaken for one another, without U, so a code
/// rarely spells a word.
const ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 6;
/// How long someone no source lists can go unseen before they count as having left: days
/// before the newest day anything was collected for.
const FORMER_AFTER_DAYS: i64 = 21;

pub fn valid_code(code: &str) -> bool {
    code.len() == CODE_LENGTH && code.bytes().all(|b| ALPHABET.contains(&b))
}

/// What one pass reads: every ID the collections hold and the links saved on the meal-break
/// page. Reading takes the shared lock; `assign_drivers` then writes in one short step.
pub struct DriverSources {
    identities: Vec<sources::Identity>,
    saved: Saved,
}

/// A row of `person_ids`: an ID, its person and how it came to them. Nothing a source
/// says about the person is kept here; that stays in the source's own database. Its source
/// is kept as written, as a feature no longer in the build may have named it.
pub(crate) struct IdRow {
    source: String,
    external_id: String,
    code: String,
    linked_by: DriverLink,
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
    fn source(&self) -> Option<DriverSource> {
        DriverSource::parse(&self.source)
    }
    fn key(&self) -> Option<Key> {
        Some((self.source()?, self.external_id.clone()))
    }
}

/// What Driver Match reads and writes for a DSP: its codes, the decisions about them and the
/// IDs they lead to.
pub trait DriverMatchStore {
    fn match_drivers(&self, dsp: &str) -> Result<usize>;
    fn driver_sources(&self, dsp: &str) -> Result<DriverSources>;
    fn assign_drivers(&self, dsp: &str, found: DriverSources) -> Result<usize>;
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch>;
    fn driver_counts(&self, dsp: &str) -> Result<DriverCounts>;
    fn driver_details(&self, dsp: &str, code: &str) -> Result<DriverDetails>;
    /// The photo of `code`'s person, from whichever feature holds one: its kind and its bytes.
    fn driver_photo(&self, dsp: &str, code: &str) -> Result<Option<Photo>>;
    fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch>;
    fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch>;
    fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch>;
    /// Confirms one of `code`'s IDs is theirs, as a match only suggested it.
    fn confirm_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch>;
}
impl DriverMatchStore for Store {
    /// Gives every new ID in the DSP's collections a person. Answers how many IDs were new.
    fn match_drivers(&self, dsp: &str) -> Result<usize> {
        self.assign_drivers(dsp, self.driver_sources(dsp)?)
    }
    /// Reads what a pass needs, without writing: run it under the shared lock.
    fn driver_sources(&self, dsp: &str) -> Result<DriverSources> {
        assign::read(self, dsp)
    }
    /// Writes what a pass found: a person for every new ID.
    fn assign_drivers(&self, dsp: &str, found: DriverSources) -> Result<usize> {
        assign::write(self, dsp, found)
    }
    /// The Driver Match tab: the pairs to decide, then every person with a code.
    fn driver_match(&self, dsp: &str) -> Result<DriverMatch> {
        Ok(overview::overview(self, dsp)?.result)
    }
    /// The Settings tab badge uses the same decisions without transferring the roster.
    fn driver_counts(&self, dsp: &str) -> Result<DriverCounts> {
        Ok(overview::overview(self, dsp)?.result.counts)
    }
    fn driver_details(&self, dsp: &str, code: &str) -> Result<DriverDetails> {
        details::details(self, dsp, code)
    }
    fn driver_photo(&self, dsp: &str, code: &str) -> Result<Option<Photo>> {
        details::photo(self, dsp, code)
    }
    fn merge_drivers(&self, c: &Context, code: &str, into: &str) -> Result<DriverMatch> {
        decisions::merge(self, c, code, into)
    }
    fn split_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch> {
        decisions::split(self, c, code, source, id)
    }
    fn keep_drivers_apart(&self, c: &Context, code: &str, other: &str) -> Result<DriverMatch> {
        decisions::keep_apart(self, c, code, other)
    }
    fn confirm_driver(
        &self,
        c: &Context,
        code: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<DriverMatch> {
        decisions::confirm(self, c, code, source, id)
    }
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
