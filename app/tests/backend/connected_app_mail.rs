//! The mail that describes a connected app, which reads every registered feature's areas.
use dispatch_core::mcp::api::types::{AgentArea, AgentReads};
use dispatch_core::server::mail::templates::*;
fn connected<'a>(known: bool, dsps: &'a [String], reads: &'a AgentReads) -> ConnectedApp<'a> {
    ConnectedApp {
        origin: "https://dispatch.test",
        dev: false,
        to: "owner@dispatch.test",
        connection: "Laptop <Claude>",
        app: "Claude Code",
        known,
        destination: Some("this computer"),
        dsps,
        reads,
        approved_by: "Platform Owner",
        at: 1_790_337_600_000,
    }
}
fn reading(areas: &[AgentArea], bypass: bool) -> AgentReads {
    AgentReads {
        areas: areas.to_vec(),
        bypass,
    }
}
fn kind(id: &str) -> AgentArea {
    AgentArea::parse(id).unwrap()
}
#[test]
fn a_connected_app_is_described_plainly_and_escaped() {
    let some = reading(&[kind("routes"), kind("timecards")], false);
    let mail = app_connected(&connected(true, &[], &some));
    assert_eq!(mail.subject, "Claude Code connected to Dispatch");
    for line in [
        "Connection: Laptop <Claude>",
        "App: Claude Code (known metadata)",
        "Sends access to: this computer",
        "DSPs: All DSPs",
        "Access: Reads Routes & packages, Timecards\n",
        "Approved by: Platform Owner",
        "Connected: September 25, 2026 at 12:00 PM UTC",
        "Review connected apps: https://dispatch.test/#agents?tab=apps",
    ] {
        assert!(mail.text.contains(line), "{line}\n{}", mail.text);
    }
    assert!(mail.html.contains("Laptop &lt;Claude&gt;") && !mail.html.contains("<Claude>"));
    assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
    // Every kind of data reads as all of it; bypassing features is said beside it.
    let everything = reading(&AgentArea::all().collect::<Vec<_>>(), true);
    let mail = app_connected(&connected(true, &[], &everything));
    assert!(
        mail.text
            .contains("Access: Reads all data. Bypasses switched-off features.\n"),
        "{}",
        mail.text
    );
    let names: Vec<String> = (1..=12).map(|n| format!("DSP {n}")).collect();
    let mail = app_connected(&connected(false, &names, &some));
    assert_eq!(
        mail.subject,
        "Claude Code (unrecognized) connected to Dispatch"
    );
    assert!(
        mail.text
            .contains("App: Unrecognized app, which says it is \u{201c}Claude Code\u{201d}")
    );
    assert!(mail.text.contains(
        "DSPs: DSP 1, DSP 2, DSP 3, DSP 4, DSP 5, DSP 6, DSP 7, DSP 8, DSP 9, DSP 10 and 2 more"
    ));
}
