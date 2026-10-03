//! In-memory wakeups and bounded invalidation hints; reads always recheck authority.
use crate::{Result, contracts::CollectionChange};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, watch};

struct Tenant {
    sender: watch::Sender<u64>,
    changes: VecDeque<(u64, CollectionChange)>,
}
pub struct Updates {
    epoch: String,
    tenants: Mutex<HashMap<String, Tenant>>,
    pub slots: Arc<Semaphore>,
}
impl Updates {
    pub fn new() -> Result<Self> {
        Ok(Self {
            epoch: super::crypto::id("live")?,
            tenants: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(128)),
        })
    }
    pub fn subscribe(&self, dsp: &str) -> watch::Receiver<u64> {
        self.tenants
            .lock()
            .unwrap()
            .entry(dsp.into())
            .or_insert_with(|| Tenant {
                sender: watch::channel(0).0,
                changes: VecDeque::new(),
            })
            .sender
            .subscribe()
    }
    pub fn notify(&self, dsp: &str) {
        self.changed(dsp, CollectionChange::all());
    }
    pub fn changed(&self, dsp: &str, change: CollectionChange) {
        if let Some(tenant) = self.tenants.lock().unwrap().get_mut(dsp) {
            let revision = *tenant.sender.borrow() + 1;
            tenant.changes.push_back((revision, change));
            while tenant.changes.len() > 128 {
                tenant.changes.pop_front();
            }
            tenant.sender.send_replace(revision);
        }
    }
    pub fn token(&self, receiver: &watch::Receiver<u64>) -> String {
        format!("{}:{}", self.epoch, *receiver.borrow())
    }
    pub fn changes(&self, dsp: &str, after: &str, through: &str) -> Vec<CollectionChange> {
        if after == through {
            return vec![];
        }
        let revision = |token: &str| {
            token
                .strip_prefix(&format!("{}:", self.epoch))
                .and_then(|s| s.parse::<u64>().ok())
        };
        let (Some(after), Some(through)) = (revision(after), revision(through)) else {
            return vec![CollectionChange::all()];
        };
        let tenants = self.tenants.lock().unwrap();
        let Some(tenant) = tenants.get(dsp) else {
            return vec![CollectionChange::all()];
        };
        if after > through
            || tenant
                .changes
                .front()
                .is_none_or(|(first, _)| after < first.saturating_sub(1))
        {
            return vec![CollectionChange::all()];
        }
        tenant
            .changes
            .iter()
            .filter(|(n, _)| *n > after && *n <= through)
            .map(|(_, change)| change.clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "../tests/backend/live.rs"]
mod tests;
