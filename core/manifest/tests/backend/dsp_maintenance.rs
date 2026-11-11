use super::*;
use crate::manifest::{Maintenance, custom, feature};

#[test]
#[should_panic(expected = "is custom: use dsp_maintenance instead of platform-wide maintenance")]
fn custom_features_cannot_dispatch_platform_wide_upkeep() {
    static CUSTOM: Feature = Feature {
        switch: custom("dsp42_probe", "Probe", &[]),
        maintenance: &[Maintenance {
            every: Duration::from_secs(60),
            run: |_, _| Box::pin(async {}),
        }],
        ..feature("dsp42_probe")
    }
    .registered(Some("dsp42"), "dsp42_probe");
    crate::testing::whole(None, &[], &[&CUSTOM]).check();
}
