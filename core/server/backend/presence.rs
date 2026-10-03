//! Who has the dashboard open. Held in memory only: presence is short-lived, and
//! after a restart every open dashboard reports itself again within one heartbeat.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

// Dashboards beat every 30 seconds. Browsers slow the timers of a background tab
// to one a minute, which must not read as the member leaving.
const FRESH: Duration = Duration::from_secs(90);
const TABS: usize = 16;

struct Beat {
    active: bool,
    at: Instant,
}
type Members = HashMap<String, HashMap<String, Beat>>;

#[derive(Default)]
pub struct Presence(Mutex<HashMap<String, Members>>);
impl Presence {
    /// Records one dashboard tab. `active` is `None` when the tab is closing.
    pub fn beat(&self, dsp: &str, user: &str, tab: &str, active: Option<bool>) {
        self.beat_at(dsp, user, tab, active, Instant::now());
    }
    /// The most active of a member's open dashboards wins.
    pub fn status(&self, dsp: &str, user: &str) -> &'static str {
        self.status_at(dsp, user, Instant::now())
    }
    fn beat_at(&self, dsp: &str, user: &str, tab: &str, active: Option<bool>, now: Instant) {
        let mut dsps = self.0.lock().unwrap();
        let members = dsps.entry(dsp.into()).or_default();
        let tabs = members.entry(user.into()).or_default();
        match active {
            Some(active) if tabs.len() < TABS || tabs.contains_key(tab) => {
                tabs.insert(tab.into(), Beat { active, at: now });
            }
            Some(_) => {}
            None => {
                tabs.remove(tab);
            }
        }
        // Every beat sweeps its DSP, so closed dashboards never accumulate.
        members.retain(|_, tabs| {
            tabs.retain(|_, beat| now.duration_since(beat.at) < FRESH);
            !tabs.is_empty()
        });
        if members.is_empty() {
            dsps.remove(dsp);
        }
    }
    fn status_at(&self, dsp: &str, user: &str, now: Instant) -> &'static str {
        let dsps = self.0.lock().unwrap();
        let mut fresh = dsps
            .get(dsp)
            .and_then(|members| members.get(user))
            .into_iter()
            .flat_map(HashMap::values)
            .filter(|beat| now.duration_since(beat.at) < FRESH)
            .peekable();
        if fresh.peek().is_none() {
            "offline"
        } else if fresh.any(|beat| beat.active) {
            "active"
        } else {
            "idle"
        }
    }
}

#[cfg(test)]
#[path = "../tests/backend/presence.rs"]
mod tests;
