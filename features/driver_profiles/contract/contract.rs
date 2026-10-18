//! Amazon Roster, as any feature or agent tool uses it without depending on Driver Profiles:
//! TODO(scaffold): what it offers, in a sentence. Driver Profiles exposes it as `amazon_roster`.
use dispatch_core::{Result, db::Store};

/// TODO(scaffold): what it reads or does for a DSP, in a sentence.
pub trait AmazonRoster: Send + Sync {
    /// Whether it's ready for the DSP. TODO(scaffold): its readers and actions instead, each
    /// with only what its first user needs, and every one naming the DSP.
    fn ready(&self, db: &Store, dsp: &str) -> Result<bool>;
}

/// What offers it for the DSP: only while Driver Profiles is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn AmazonRoster>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn AmazonRoster>(db, dsp)
}
