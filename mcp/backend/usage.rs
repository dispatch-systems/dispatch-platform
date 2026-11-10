//! How much each key is used, kept in memory: one process serves an environment, so a
//! count here is the whole count. Agent reads stay reads; once a minute the scheduler
//! writes down when each key was last used.
use crate::minute::Minute;
use dispatch_core::{Result, db::now, ensure};
use std::{collections::HashMap, sync::Mutex};

/// Calls a key may make in one minute.
pub const PER_MINUTE: u32 = 120;

#[derive(Default)]
pub struct Usage(Mutex<HashMap<String, Use>>);
struct Use {
    calls: Minute,
    at: i64,
    client: String,
    unsaved: bool,
}
/// A key's last call: when, and from which client.
pub type LastUse = (i64, String);

impl Usage {
    /// Counts one call, or refuses it once the key has made `PER_MINUTE` calls in the
    /// preceding rolling minute.
    pub fn admit(&self, key: &str, client: &str) -> Result<()> {
        self.admit_at(key, client, now())
    }
    fn admit_at(&self, key: &str, client: &str, at: i64) -> Result<()> {
        let mut keys = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let entry = keys.entry(key.to_owned()).or_insert_with(|| Use {
            calls: Minute::default(),
            at,
            client: String::new(),
            unsaved: false,
        });
        let calls = entry.calls.count(at);
        ensure(calls < PER_MINUTE as usize, "rate_limited", 429)?;
        entry.calls.record(at);
        entry.at = at;
        entry.client = client.to_owned();
        entry.unsaved = true;
        Ok(())
    }
    /// The last calls not yet written down, which then count as written.
    pub fn take(&self) -> Vec<(String, LastUse)> {
        let mut keys = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        keys.iter_mut()
            .filter(|(_, entry)| entry.unsaved)
            .map(|(key, entry)| {
                entry.unsaved = false;
                (key.clone(), (entry.at, entry.client.clone()))
            })
            .collect()
    }
    /// Marks keys' last calls as not yet written down again, after writing them failed.
    /// The next take writes whatever each key's last call is by then.
    pub fn unsaved(&self, keys: &[String]) {
        let mut entries = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        for key in keys {
            if let Some(entry) = entries.get_mut(key) {
                entry.unsaved = true;
            }
        }
    }
    /// Each key's last call as this process saw it, newer than what is written down.
    pub fn last(&self) -> HashMap<String, LastUse> {
        let keys = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        keys.iter()
            .map(|(key, entry)| (key.clone(), (entry.at, entry.client.clone())))
            .collect()
    }
}

#[cfg(test)]
#[path = "../tests/backend/usage.rs"]
mod tests;
