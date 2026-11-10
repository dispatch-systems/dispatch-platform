//! Bounded waiting the server's limiters share: a slot taken without waiting, and turns keyed
//! by what waits for them. A refusal is `platform_busy`, named for the limiter behind it.
use crate::{Error, Result};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// One of `slots` for a caller that doesn't wait for it, or a refusal named `full` when none
/// is free and `closed` when no more are given.
pub(crate) fn slot(
    slots: &Arc<Semaphore>,
    full: &'static str,
    closed: &'static str,
) -> Result<OwnedSemaphorePermit> {
    slots
        .clone()
        .try_acquire_owned()
        .map_err(|error| match error {
            TryAcquireError::NoPermits => Error::busy(full),
            TryAcquireError::Closed => Error::busy(closed),
        })
}

/// What each refusal of some `KeyedTurns` is named, so a busy response says which turned it
/// away.
#[derive(Clone, Copy)]
pub struct Origins {
    full: &'static str,
    closed: &'static str,
    poisoned: &'static str,
    key_timeout: &'static str,
}
impl Origins {
    /// Identical expensive reads, each waiting for the one before to fill the cache.
    pub const READS: Self = Self {
        full: "read_turns_full",
        closed: "read_turns_closed",
        poisoned: "read_turns_poisoned",
        key_timeout: "read_turns_key_timeout",
    };
    /// One DSP's unpublished writes, each waiting for the one before.
    pub const UNPUBLISHED: Self = Self {
        full: "unpublished_turns_full",
        closed: "unpublished_turns_closed",
        poisoned: "unpublished_turns_poisoned",
        key_timeout: "unpublished_turns_key_timeout",
    };
}

/// Callers with the same key take turns before entering the database pool; other keys don't
/// wait for them. Keep the turn inside the database closure if its caller is cancelled.
pub struct KeyedTurns {
    origins: Origins,
    keys: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    slots: Arc<Semaphore>,
}
pub struct KeyedTurn {
    _slot: OwnedSemaphorePermit,
    _lock: OwnedMutexGuard<()>,
}
impl KeyedTurns {
    pub fn new(origins: Origins) -> Self {
        Self {
            origins,
            keys: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(64)),
        }
    }
    pub async fn enter(&self, key: String) -> Result<KeyedTurn> {
        self.enter_for(key, Duration::from_secs(15)).await
    }
    /// Takes a keyed turn with the caller's waiting budget, before entering the database pool.
    pub(crate) async fn enter_for(&self, key: String, wait: Duration) -> Result<KeyedTurn> {
        let slot = slot(&self.slots, self.origins.full, self.origins.closed)?;
        let lock = {
            let mut keys = self
                .keys
                .lock()
                .map_err(|_| Error::busy(self.origins.poisoned))?;
            keys.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = keys.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                keys.insert(key, Arc::downgrade(&lock));
                lock
            }
        };
        let lock = tokio::time::timeout(wait, lock.lock_owned())
            .await
            .map_err(|_| Error::busy(self.origins.key_timeout))?;
        Ok(KeyedTurn {
            _slot: slot,
            _lock: lock,
        })
    }
}

#[cfg(test)]
#[path = "../tests/backend/admission.rs"]
mod tests;
