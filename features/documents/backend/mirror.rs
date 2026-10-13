//! Each connected DSP's Documents as Drive last described it, kept in memory so a folder opens
//! and a search finds without asking Google. Google's change feed brings it up to date, and
//! what Dispatch changes itself goes in as soon as Google confirms it. A copy nobody has looked
//! at for half an hour goes, so the server keeps only those of the DSPs using Documents; a
//! server that starts, or a DSP that connects again or comes back, lists the account's Drive
//! whole once more.
use super::{
    drive::{Changes, Item},
    tree::Tree,
};
use dispatch_core::Error;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::OwnedMutexGuard;

/// How long a copy answers without asking Google what changed.
pub const FRESH: Duration = Duration::from_secs(5);
/// How long a copy answers at once while Google is asked in the background: an older one asks
/// before it answers.
pub const STALE: Duration = Duration::from_secs(60);
/// How long a copy is kept after it was last looked at.
const IDLE: Duration = Duration::from_secs(30 * 60);
/// How long Google's failing to answer stands for those waiting their turn to ask.
const FAILED: Duration = Duration::from_secs(5);

/// What the account's Drive held when Dispatch last asked.
struct Copy {
    tree: Arc<Tree>,
    /// Where the account's change feed goes on from.
    changes: String,
    /// When Google last said what changed.
    synced: Instant,
}

/// One DSP's copy, when it was last looked at, and the turn to bring it up to date: Google is
/// asked once at a time. When asking last failed, and why.
pub struct Mirror {
    copy: Mutex<Option<Copy>>,
    viewed: Mutex<Instant>,
    turn: Arc<tokio::sync::Mutex<()>>,
    failed: Mutex<Option<(Instant, String, u16)>>,
}
static MIRRORS: LazyLock<Mutex<HashMap<String, Arc<Mirror>>>> = LazyLock::new(Default::default);

fn mirrors() -> MutexGuard<'static, HashMap<String, Arc<Mirror>>> {
    MIRRORS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
/// The DSP's mirror.
pub fn of(dsp: &str) -> Arc<Mirror> {
    Arc::clone(mirrors().entry(dsp.to_owned()).or_default())
}
/// Forgets the DSP's copy, as a disconnect or a new connection does.
pub fn forget(dsp: &str) {
    mirrors().remove(dsp);
}
/// Forgets the copies nobody has looked at for half an hour.
pub fn sweep() {
    mirrors().retain(|_, mirror| mirror.viewed().elapsed() < IDLE);
}

impl Default for Mirror {
    fn default() -> Self {
        Self {
            copy: Mutex::new(None),
            viewed: Mutex::new(Instant::now()),
            turn: Arc::default(),
            failed: Mutex::new(None),
        }
    }
}
impl Mirror {
    fn copy(&self) -> MutexGuard<'_, Option<Copy>> {
        self.copy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn viewed(&self) -> MutexGuard<'_, Instant> {
        self.viewed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    /// Runs `change` on the copy of the main folder `root`, if there is one.
    fn change<T>(&self, root: &str, change: impl FnOnce(&mut Copy) -> T) -> Option<T> {
        self.copy()
            .as_mut()
            .filter(|copy| copy.tree.root() == root)
            .map(change)
    }
    /// How long ago Google last said what changed in the copy of `root`: none without one.
    pub fn age(&self, root: &str) -> Option<Duration> {
        let copy = self.copy();
        let copy = copy.as_ref().filter(|copy| copy.tree.root() == root)?;
        Some(copy.synced.elapsed())
    }
    /// The tree under `root`, as the copy holds it, for someone looking at it.
    pub fn tree(&self, root: &str) -> Option<Arc<Tree>> {
        *self.viewed() = Instant::now();
        let copy = self.copy();
        let copy = copy.as_ref().filter(|copy| copy.tree.root() == root)?;
        Some(Arc::clone(&copy.tree))
    }
    /// Where the change feed goes on from, for a copy of `root`.
    pub fn changes(&self, root: &str) -> Option<String> {
        let copy = self.copy();
        let copy = copy.as_ref().filter(|copy| copy.tree.root() == root)?;
        Some(copy.changes.clone())
    }
    /// Makes a new copy of `root` from every file Drive listed, its change feed going on from
    /// `changes`, asked before they were listed.
    pub fn replace(&self, root: &str, all: Vec<Item>, changes: String) {
        *self.copy() = Some(Copy {
            tree: Arc::new(Tree::new(root, all)),
            changes,
            synced: Instant::now(),
        });
    }
    /// Brings the copy of `root` up to date with what changed. Answers whether the Drive
    /// needs listing whole again, as a folder came in from elsewhere in the account.
    pub fn follow(&self, root: &str, changes: Changes) -> bool {
        self.change(root, |copy| {
            copy.changes = changes.next;
            copy.synced = Instant::now();
            Arc::make_mut(&mut copy.tree).follow(changes.changed)
        })
        .unwrap_or(false)
    }
    /// Puts in a file Dispatch made or changed, as Google answered.
    pub fn put(&self, root: &str, item: Item) {
        self.change(root, |copy| Arc::make_mut(&mut copy.tree).put(item));
    }
    /// Takes out a file Dispatch trashed, or Google said is gone.
    pub fn remove(&self, root: &str, id: &str) {
        self.change(root, |copy| Arc::make_mut(&mut copy.tree).remove(id));
    }
    /// The turn to ask Google about the DSP's Drive, once whoever has it is done.
    pub async fn wait(&self) -> OwnedMutexGuard<()> {
        Arc::clone(&self.turn).lock_owned().await
    }
    /// The turn to ask Google, unless someone has it.
    pub fn try_turn(&self) -> Option<OwnedMutexGuard<()>> {
        Arc::clone(&self.turn).try_lock_owned().ok()
    }
    fn failure(&self) -> MutexGuard<'_, Option<(Instant, String, u16)>> {
        self.failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    /// Notes that asking Google failed with `error`.
    pub fn fail(&self, error: &Error) {
        *self.failure() = Some((Instant::now(), error.code.clone(), error.status));
    }
    /// Why asking Google failed a moment ago, if it did: what those who waited for the turn
    /// answer, rather than each asking in turn.
    pub fn failed(&self) -> Option<Error> {
        self.failure()
            .as_ref()
            .filter(|(at, _, _)| at.elapsed() < FAILED)
            .map(|(_, code, status)| Error::new(code.clone(), *status))
    }
}

#[cfg(test)]
#[path = "../tests/backend/mirror.rs"]
mod tests;
