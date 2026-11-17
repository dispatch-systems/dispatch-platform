//! HR Line: one DSP's own line of phones for HR, apart from Dispatch Phone's: its phones, paired,
//! renamed and removed on a card of its own on Settings' DSP Connections. Its README says more.
mod api;

use dispatch_communications::lines::Line;
use dispatch_core::{
    manifest::{Audit, Feature, custom, feature, perm},
    tenancy::api::audit::AuditArea::Connections,
};

pub const FEATURE: Feature = Feature {
    place: 160,
    switch: custom("cjf87gqd_hr_line", "HR Line", &[]),
    // Its phones are a line of Dispatch Phone's, which relays their texts by its phones' address.
    depends_on: &["communications"],
    // Pairing, renaming and removing its phones, as those who manage the DSP's connections may
    // too.
    permissions: &[perm(
        "cjf87gqd_hr_line.phones",
        "Manage HR Line Phones",
        170,
    )],
    routes: api::routes::routes,
    // Its phones are the DSP's own, managed on DSP Connections.
    dsp_connection: true,
    audit: Audit {
        areas: &[("cjf87gqd_hr_line.phones.", Connections)],
        ..Audit::NONE
    },
    ..feature("cjf87gqd_hr_line")
};

/// Its line of the DSP's phones: theirs alone, named HR Line, then HR Line 2 and so on, as they
/// pair.
pub const LINE: Line<'static> = Line {
    id: FEATURE.switch.id,
    name: "HR Line",
};
