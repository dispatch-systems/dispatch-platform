//! How a live reader reads for a DSP, as Settings shows it with the section that sets when it
//! reads: its last read, when it reads next by the DSP's plan, and its last failure, from the
//! records of the last two weeks.
use super::{now, plan};
use crate::{
    Error, Result,
    accounts::api::types::Dsp,
    collection::api::types::{LiveError, LiveRead, LiveReading},
    db::{self, Store},
    manifest::registry,
};

/// How `reader` reads for `dsp` now: none while it reads nothing there, as while its feature is
/// off or its schedule isn't set. Refused for a reader no feature declares (`not_found`).
pub fn reading(db: &Store, dsp: &Dsp, reader: &str) -> Result<Option<LiveReading>> {
    let (feature, reads) = registry()
        .live_readers()
        .find(|(_, each)| each.id() == reader)
        .ok_or_else(|| Error::new("not_found", 404))?;
    let Some(plan) = plan(db, &dsp.id, &dsp.timezone, feature, reads)? else {
        return Ok(None);
    };
    let last = db.last_read(reader, &dsp.id)?;
    let ended = db.last_ended(reader, &dsp.id)?;
    let after = last.as_ref().map_or(i64::MIN, |read| read.at);
    Ok(Some(LiveReading {
        last_read: last.map(|read| LiveRead {
            at: db::at(read.at),
            kept: read.kept,
        }),
        next_read: db::at(plan.next_read(now(), after, ended)),
        last_error: db
            .last_failed(reader, &dsp.id)?
            .map(|(at, code)| LiveError {
                at: db::at(at),
                code,
            }),
    }))
}
