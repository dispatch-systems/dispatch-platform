//! HR Line: one DSP's own line of phones for HR, apart from Dispatch Phone's: its page, where its
//! texts are read and answered from its own phones, and its phones, paired, renamed and removed on
//! a card of its own on Settings' DSP Connections. Its README says more.
mod api;

use dispatch_communications::lines::{self, Line, Pushed};
use dispatch_core::{
    manifest::{Audit, DspMaintenance, Feature, Section, custom, feature, perm, section},
    tenancy::api::audit::AuditArea::Connections,
};
use std::time::Duration;

pub const FEATURE: Feature = Feature {
    place: 160,
    switch: custom("cjf87gqd_hr_line", "HR Line", &[]),
    // Its phones are a line of Dispatch Phone's, which keeps and relays their texts by its phones'
    // address.
    depends_on: &["communications"],
    // Reading HR Line's texts and answering them; and pairing, renaming and removing its phones,
    // as those who manage the DSP's connections may too.
    permissions: &[
        perm("cjf87gqd_hr_line.use", "Use HR Line", 160),
        perm("cjf87gqd_hr_line.phones", "Manage HR Line Phones", 170),
    ],
    // How its page looks to each member and alerts them, their own, on Settings' Display page.
    settings: &[DISPLAY],
    routes: api::routes::routes,
    // Watching its DSP for what to push to its members' browsers, as after a restart.
    dsp_maintenance: &[PUSHING],
    // Its phones are the DSP's own, managed on DSP Connections.
    dsp_connection: true,
    // Pairing, renaming and removing its phones, under Connections; the names it saves and the
    // numbers it blocks, under Settings, as Dispatch Phone's own are.
    audit: Audit {
        areas: &[("cjf87gqd_hr_line.phones.", Connections)],
        ..Audit::NONE
    },
    ..feature("cjf87gqd_hr_line")
};

/// Its line of the DSP's phones: theirs alone, named HR Line, then HR Line 2 and so on, as they
/// pair. Its texts aren't filed yet: the next release turns `texting` on, once a rollback no longer
/// returns to a release that reads every conversation as Dispatch Phone's.
pub const LINE: Line<'static> = Line {
    id: FEATURE.switch.id,
    name: "HR Line",
    texting: false,
};

/// Its page's section of Settings' Display page, each member's own: how HR Line's page looks to
/// them and how a new text to its phones alerts them, with Dispatch Phone's choices, apart from
/// Dispatch Phone's own Message display, after it on the page.
pub const DISPLAY: Section = section(
    "cjf87gqd_hr_line.display",
    "HR Line",
    "cjf87gqd_hr_line.use",
    lines::DISPLAY_FIELDS,
)
.on("display")
.member()
.custom()
.keywords(lines::DISPLAY_KEYWORDS)
.order(1);

/// Its pushes: each new text to its phones, to each browser kept for HR Line of a member who may
/// use it and has its Notifications on, titled "HR Line" before who wrote, opening HR Line's page;
/// never Dispatch Phone's texts, nor its page.
pub const PUSHED: Pushed = Pushed {
    line: Some(LINE.id),
    permission: "cjf87gqd_hr_line.use",
    page: "cjf87gqd-hr-line",
    label: Some(LINE.name),
    display: &DISPLAY,
};

/// Every minute, HR Line's DSP is watched for what to push, if a browser is kept for it, as after
/// a restart.
const PUSHING: DspMaintenance = DspMaintenance {
    every: Duration::from_secs(60),
    run: |state, _, _| lines::pushing(state, &PUSHED),
};
