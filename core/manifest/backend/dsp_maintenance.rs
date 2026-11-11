//! Upkeep dispatched separately for each active DSP with the feature enabled. Custom
//! features use this slot, so their callbacks receive only their owner's DSP id.
use crate::{
    State,
    manifest::{Feature, Registry, Upkeep},
};
use std::{sync::Arc, time::Duration};

/// Upkeep offered every minute to each DSP the feature is enabled for. One pass of a
/// declaration runs at a time, visiting its DSPs in order. `due` says whether `every` has
/// passed since the declaration was last due, as for platform-wide maintenance.
pub struct DspMaintenance {
    pub every: Duration,
    /// Work for this DSP alone. Writes use `State::run_serving` and check the feature again
    /// in the write turn when work awaited an outside service.
    pub run: fn(Arc<State>, String, bool) -> Upkeep,
}

impl Registry {
    /// Each scoped upkeep declaration with the feature whose switch and owner admit it.
    pub fn dsp_maintenance(
        &self,
    ) -> impl Iterator<Item = (&'static Feature, &'static DspMaintenance)> {
        self.features.iter().flat_map(|feature| {
            feature
                .dsp_maintenance
                .iter()
                .map(move |each| (*feature, each))
        })
    }
}

/// Custom upkeep must never receive a platform-wide dispatch.
pub(crate) fn check(registry: &Registry) {
    for feature in registry.features {
        assert!(
            feature.owner.is_none() || feature.maintenance.is_empty(),
            "{} is custom: use dsp_maintenance instead of platform-wide maintenance",
            feature.name
        );
    }
}

#[cfg(test)]
#[path = "../tests/backend/dsp_maintenance.rs"]
mod tests;
