//! The stand-in for Vans' `amazon_fleet`, while `fleet` is on: the DSP's vans, as a test says.
use super::Known;
use dispatch_core::{
    Error, Result,
    db::Store,
    manifest::{Feature, feature, optional},
};
use dispatch_vans_contract::{AmazonFleet, MOST_VINS, Van};

pub(super) static FLEET: Feature = Feature {
    switch: optional("fleet", "Fleet", &[]),
    exposes: &[dispatch_core::expose!(
        dyn AmazonFleet,
        &STAND_IN,
        "amazon_fleet",
        "The DSP's vans, as a test says.",
    )],
    ..feature("fleet")
};
struct Fleet;
static STAND_IN: Fleet = Fleet;
/// Each DSP's vans.
static VANS: Known<Vec<Van>> = Known::new();

/// Has it answer that the DSP's vans are `vans`.
pub(super) fn know(dsp: &str, vans: Vec<Van>) {
    VANS.of(dsp, |known| *known = vans);
}

impl AmazonFleet for Fleet {
    fn collected(&self, _: &Store, _: &str) -> Result<Option<String>> {
        Ok(None)
    }
    /// The vans it knows under `vins`; refused for more than Vans reads.
    fn vans(&self, _: &Store, dsp: &str, vins: &[String]) -> Result<Vec<Van>> {
        if vins.len() > MOST_VINS {
            return Err(Error::new("invalid_input", 400));
        }
        Ok(VANS.of(dsp, |vans| {
            vans.iter()
                .filter(|van| vins.contains(&van.vin))
                .cloned()
                .collect()
        }))
    }
}
