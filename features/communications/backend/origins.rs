//! Exact, timezone-aware run starts. No body, file, directory label or per-message run ID is
//! persisted here. Unknown writers invalidate readiness through ordinary SQLite triggers.
//!
//! `source` is each transport's SQL dialect and a source row's topology, `repair` keeps a ready
//! generation exact through each known write, `backfill` builds one in upkeep, and `read` answers
//! a window from it. `read` also names the rows beside any window's selection, ready or not, so
//! one projection of a source row says who sent it.
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row},
};
use rusqlite::params;

pub(crate) mod backfill;
mod read;
mod repair;
mod source;

pub(crate) use read::{beside, read};
pub(crate) use repair::{change, place};
pub(crate) use source::{Place, Transport};

/// Bump the prefix when writer/day semantics change. A tzdata update changes this key too.
pub(crate) fn algorithm() -> String {
    format!("conversation-origins-1:{}", chrono_tz::IANA_TZDB_VERSION)
}

#[derive(Clone)]
pub(crate) struct IndexState {
    epoch: i64,
    indexed: Option<i64>,
    generation: Option<String>,
    zone: Option<String>,
    algorithm: Option<String>,
}
impl FromRow for IndexState {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            epoch: row.get("source_epoch")?,
            indexed: row.get("indexed_epoch")?,
            generation: row.get("active_generation")?,
            zone: row.get("zone")?,
            algorithm: row.get("algorithm")?,
        })
    }
}
impl IndexState {
    fn ready(&self, zone: &str) -> bool {
        self.generation.is_some()
            && self.indexed == Some(self.epoch)
            && self.zone.as_deref() == Some(zone)
            && self.algorithm.as_deref() == Some(algorithm().as_str())
    }
}
pub(crate) fn state(
    db: &Db,
    transport: Transport,
    conversation: &str,
) -> Result<Option<IndexState>> {
    db.one_as(
        "SELECT source_epoch,indexed_epoch,active_generation,zone,algorithm \
        FROM communications_origin_state WHERE transport=? AND conversation_id=?",
        params![transport.name(), conversation],
    )
}

fn invariant() -> Error {
    Error::new("communications_origins_invariant", 500)
}
