//! In-memory wakeups and bounded invalidation hints; reads always recheck authority. Collection
//! progress has its hub, `Updates`; every feature's own live channel is a `Topic` of `Topics`.
use crate::{Result, collection::api::types::CollectionChange};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, watch};

struct Tenant {
    sender: watch::Sender<u64>,
    /// Each change, with the kind of job whose collection made it; none reaches every reader.
    changes: VecDeque<(u64, Option<String>, CollectionChange)>,
}
pub struct Updates {
    epoch: String,
    tenants: Mutex<HashMap<String, Tenant>>,
    pub slots: Arc<Semaphore>,
}
impl Updates {
    pub fn new() -> Result<Self> {
        Ok(Self {
            epoch: crate::foundation::crypto::id("live")?,
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
        self.record(dsp, None, CollectionChange::all());
    }
    /// What a finished collection of a job of `kind` changed.
    pub fn changed(&self, dsp: &str, kind: &str, change: CollectionChange) {
        self.record(dsp, Some(kind.to_owned()), change);
    }
    fn record(&self, dsp: &str, kind: Option<String>, change: CollectionChange) {
        if let Some(tenant) = self.tenants.lock().unwrap().get_mut(dsp) {
            let revision = *tenant.sender.borrow() + 1;
            tenant.changes.push_back((revision, kind, change));
            while tenant.changes.len() > 128 {
                tenant.changes.pop_front();
            }
            tenant.sender.send_replace(revision);
        }
    }
    pub fn token(&self, receiver: &watch::Receiver<u64>) -> String {
        format!("{}:{}", self.epoch, *receiver.borrow())
    }
    /// The changes between two tokens a reader may follow: those of the kinds `follows` admits,
    /// and those every reader may. Lost history is one change that reaches everything.
    pub fn changes(
        &self,
        dsp: &str,
        after: &str,
        through: &str,
        follows: impl Fn(&str) -> bool,
    ) -> Vec<CollectionChange> {
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
                .is_none_or(|(first, _, _)| after < first.saturating_sub(1))
        {
            return vec![CollectionChange::all()];
        }
        tenant
            .changes
            .iter()
            .filter(|(n, kind, _)| {
                *n > after && *n <= through && kind.as_deref().is_none_or(&follows)
            })
            .map(|(_, _, change)| change.clone())
            .collect()
    }
}

/// A feature's own live channel, named by the feature: `const STOCK: Topic = Topic("uniforms")`.
/// Core names none of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Topic(pub &'static str);

/// Wakeups for every feature's live channels. A request waiting on a topic in a DSP wakes when
/// something there changes, then reads what changed from the feature's own storage.
pub struct Topics {
    senders: Mutex<HashMap<(Topic, String), watch::Sender<u64>>>,
    /// The requests that may wait at once, across every topic.
    pub slots: Arc<Semaphore>,
}
impl Default for Topics {
    fn default() -> Self {
        Self {
            senders: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(128)),
        }
    }
}
impl Topics {
    /// Waits on `topic` in `dsp`. Subscribe before reading, so a change between the read and
    /// the wait still wakes it.
    pub fn subscribe(&self, topic: Topic, dsp: &str) -> watch::Receiver<u64> {
        self.senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry((topic, dsp.to_owned()))
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }
    /// Wakes whatever waits on `topic` in `dsp`.
    pub fn notify(&self, topic: Topic, dsp: &str) {
        if let Some(sender) = self
            .senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&(topic, dsp.to_owned()))
        {
            sender.send_modify(|revision| *revision += 1);
        }
    }
}

#[cfg(test)]
#[path = "../tests/backend/live.rs"]
mod tests;
