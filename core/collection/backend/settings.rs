//! Collection's section of a DSP's Settings: the DSP's connections and the member's own,
//! drawn by collection's own Connections.
use crate::{
    accounts::Context,
    manifest::{Section, registry, section},
    tenancy::roles,
};

/// Every member's: what it shows them is the frontend's to say, as the DSP's accounts are for
/// those who manage its connections and each card says who else sees it, and a member's own
/// accounts are theirs.
pub const CONNECTIONS: Section =
    section("collection.connections", "Connections", roles::ACCESS, &[])
        .on("connections")
        .custom()
        .keywords(&["accounts", "sign-in", "credentials"])
        .found(connectable)
        .order(10);

/// The accounts the DSP can connect, by name: each collector's it has, and each of its
/// features' with a DSP account of its own, as "Cortex" and "Slack" find Connections.
fn connectable(c: &Context) -> Vec<String> {
    let registry = registry();
    let collectors = registry
        .collectors
        .iter()
        .filter(|collector| c.has(collector.id()))
        .map(|collector| collector.label());
    let features = registry
        .features
        .iter()
        .filter(|feature| feature.dsp_connection && c.has(feature.switch.id))
        .map(|feature| feature.switch.label);
    collectors.chain(features).map(str::to_owned).collect()
}
