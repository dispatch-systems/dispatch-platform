use super::*;
use crate::manifest::{custom, feature};

static CONNECTION: Feature = Feature {
    integration_events: Some(&["message.direct"]),
    ..feature("chat")
};
static SHARED: Feature = Feature {
    depends_on: &["chat"],
    integrations: &[Integration {
        hears: &["file_shared"],
        heard: Some(|_, _| Box::pin(async {})),
        ..integration("chat", "shared.files", "Files")
    }],
    ..feature("shared")
};
static CUSTOM: Feature = Feature {
    switch: custom("dsp42_probe", "Probe", &[]),
    depends_on: &["chat"],
    integrations: &[Integration {
        hears: &["message.direct", "file_shared"],
        heard: Some(|_, _| Box::pin(async {})),
        ..integration("chat", "dsp42_probe.files", "Files")
    }],
    ..feature("dsp42_probe")
}
.registered(Some("dsp42"), "dsp42_probe");

#[test]
fn custom_integrations_can_reuse_the_connections_and_shared_integrations_events() {
    check(&[&CONNECTION, &SHARED, &CUSTOM]);
}

#[test]
#[should_panic(expected = "cannot introduce globally subscribed event types")]
fn a_custom_integration_cannot_expand_global_subscriptions() {
    static NEW: Feature = Feature {
        switch: custom("dsp42_probe", "Probe", &[]),
        depends_on: &["chat"],
        integrations: &[Integration {
            hears: &["new_event"],
            heard: Some(|_, _| Box::pin(async {})),
            ..integration("chat", "dsp42_probe.new", "New event")
        }],
        ..feature("dsp42_probe")
    }
    .registered(Some("dsp42"), "dsp42_probe");
    check(&[&CONNECTION, &SHARED, &NEW]);
}
