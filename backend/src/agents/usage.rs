//! How much each key is used, kept in memory: one process serves an environment, so a
//! count here is the whole count. Agent reads stay reads; once a minute the scheduler
//! writes down when each key was last used.
use crate::{Result, db::now, ensure};
use std::{collections::HashMap, sync::Mutex};

/// Calls a key may make in one minute.
pub const PER_MINUTE: u32 = 120;

#[derive(Default)]
pub struct Usage(Mutex<HashMap<String, Use>>);
struct Use {
    minute: i64,
    count: u32,
    at: i64,
    client: String,
    unsaved: bool,
}
/// A key's last call: when, and from which client.
pub type LastUse = (i64, String);

impl Usage {
    /// Counts one call, or refuses it once the key has made `PER_MINUTE` this minute.
    pub fn admit(&self, key: &str, client: &str) -> Result<()> {
        self.admit_at(key, client, now())
    }
    fn admit_at(&self, key: &str, client: &str, at: i64) -> Result<()> {
        let minute = at / 60_000;
        let mut keys = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let entry = keys.entry(key.to_owned()).or_insert_with(|| Use {
            minute,
            count: 0,
            at,
            client: String::new(),
            unsaved: false,
        });
        if entry.minute != minute {
            entry.minute = minute;
            entry.count = 0;
        }
        ensure(entry.count < PER_MINUTE, "rate_limited", 429)?;
        entry.count += 1;
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
    /// Each key's last call as this process saw it, newer than what is written down.
    pub fn last(&self) -> HashMap<String, LastUse> {
        let keys = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        keys.iter()
            .map(|(key, entry)| (key.clone(), (entry.at, entry.client.clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_gets_a_minute_of_calls_then_waits() {
        let usage = Usage::default();
        let at = 1_790_000_000_000;
        for _ in 0..PER_MINUTE {
            usage.admit_at("key_a", "curl", at).unwrap();
        }
        let refused = usage.admit_at("key_a", "curl", at + 1).unwrap_err();
        assert_eq!(refused.code, "rate_limited");
        // The next minute starts a new count, and another key has its own.
        usage.admit_at("key_a", "curl", at + 60_000).unwrap();
        usage.admit_at("key_b", "Codex 0.157", at).unwrap();
        let taken = usage.take();
        assert_eq!(taken.len(), 2);
        assert!(usage.take().is_empty());
        assert_eq!(usage.last()["key_b"].1, "Codex 0.157");
    }
}
