//! The deleting slot: what a feature lets go of as a DSP is deleted for good, beyond the DSP's
//! own folder, which goes whole. Tenancy deletes a DSP (`tenancy::deletion`): it asks each
//! feature to give back what the DSP connected through it, then deletes the DSP's rows with
//! each feature's in one transaction, then its folder, and tells each feature it's gone.
use crate::{
    Result, State,
    db::Store,
    manifest::{Registry, Upkeep},
};
use std::sync::Arc;

/// What a feature lets go of as a DSP is deleted.
pub struct Deleting {
    /// Gives back what the DSP connected outside Dispatch through it, as a Slack workspace or a
    /// Google account, before anything of the DSP is deleted, whether or not the feature is on
    /// there. Best effort: a failure doesn't keep the DSP, so it logs its own.
    pub revoke: Option<fn(Arc<State>, String) -> Upkeep>,
    /// Deletes what it keeps of the DSP outside the DSP's folder, its rows in the platform's
    /// tables, in the transaction that deletes the DSP's rows there.
    pub forget: Option<fn(&Store, &str) -> Result<()>>,
    /// Lets go of what it holds of the DSP in memory, once the DSP is deleted.
    pub forgotten: Option<fn(&State, &str)>,
}
impl Deleting {
    /// A feature keeping nothing of a DSP outside its folder.
    pub const NONE: Self = Self {
        revoke: None,
        forget: None,
        forgotten: None,
    };
}

/// Panics unless every feature keeping tables in the platform's database deletes a DSP's rows
/// of them, as a deleted DSP leaves nothing there.
pub(super) fn check(registry: &Registry) {
    for feature in registry.features {
        let platform = feature
            .tables
            .iter()
            .any(|(database, _)| *database == "platform");
        assert!(
            !platform || feature.deleting.forget.is_some(),
            "{} keeps tables in the platform's database, so it deletes a deleted DSP's rows of \
             them (`deleting.forget`)",
            feature.name
        );
    }
}
