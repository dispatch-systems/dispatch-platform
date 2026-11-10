//! The first gate on the agent API, before any database turn. A credential must be shaped
//! like one Dispatch issued, which its checksum says; and an address whose credentials keep
//! being refused waits out the minute. A flood of made-up keys then costs no database turn,
//! which the dashboard's own requests need. The keys themselves are never at risk: one is 32
//! random characters, so none is guessed.
use crate::minute::Minute;
use dispatch_core::{Result, db::now, ensure};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

/// Refused credentials an address may present in a rolling minute before it waits.
pub const REFUSALS: usize = 20;
/// Addresses remembered at once; past it, those quiet for a minute are forgotten first, and
/// a new one is not remembered while none is quiet.
const MOST_SOURCES: usize = 2_000;

/// When each address's credentials were refused in the last minute.
#[derive(Default)]
pub struct Gate(Mutex<HashMap<String, Minute>>);
impl Gate {
    /// Lets a request from `ip` through, or refuses it while the address waits.
    pub fn admit(&self, ip: &str) -> Result<()> {
        self.admit_at(ip, now())
    }
    fn admit_at(&self, ip: &str, at: i64) -> Result<()> {
        let mut sources = self.sources();
        if let Some(refusals) = sources.get_mut(ip) {
            ensure(refusals.count(at) < REFUSALS, "rate_limited", 429)?;
        }
        Ok(())
    }
    /// Counts a credential of `ip`'s refused.
    pub fn refused(&self, ip: &str) {
        self.refused_at(ip, now());
    }
    fn refused_at(&self, ip: &str, at: i64) {
        let mut sources = self.sources();
        if sources.len() >= MOST_SOURCES && !sources.contains_key(ip) {
            sources.retain(|_, refusals| refusals.active(at));
            if sources.len() >= MOST_SOURCES {
                return;
            }
        }
        sources.entry(ip.to_owned()).or_default().record(at);
    }
    fn sources(&self) -> MutexGuard<'_, HashMap<String, Minute>> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
#[path = "../tests/backend/gate.rs"]
mod tests;
