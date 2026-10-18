//! The upkeep slot: work a feature runs for the platform on the scheduler (`maintenance`).
//! What it does after each collection is an event it hears (`events`).
use crate::State;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

/// Work a feature runs for the platform, with nothing to answer: it logs its own failures.
pub type Upkeep = Pin<Box<dyn Future<Output = ()> + Send>>;
/// Upkeep offered every minute, independently of collection starts and other upkeep.
/// One pass per task may run at a time. `run` hears whether `every` has passed since
/// it was last due, for work it does less often than the rest.
pub struct Maintenance {
    pub every: Duration,
    pub run: fn(Arc<State>, bool) -> Upkeep,
}
