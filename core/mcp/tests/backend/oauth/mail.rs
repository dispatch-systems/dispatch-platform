use super::*;

fn connected(known: bool, dsps: &[String]) -> ConnectedApp<'_> {
    ConnectedApp {
        origin: "https://dispatch.test",
        dev: false,
        to: "owner@dispatch.test",
        connection: "Laptop <Claude>",
        app: "Claude Code",
        known,
        destination: Some("this computer"),
        dsps,
        tools: "All 3, 2 that make changes",
        approved_by: "Platform Owner",
        at: 1_790_337_600_000,
    }
}
#[test]
fn a_connected_app_is_described_plainly_and_escaped() {
    let mail = app_connected(&connected(true, &[]));
    assert_eq!(mail.subject, "Claude Code connected to Dispatch");
    for line in [
        "Connection: Laptop <Claude>",
        "App: Claude Code (known metadata)",
        "Sends access to: this computer",
        "DSPs: All DSPs",
        "Tools: All 3, 2 that make changes\n",
        "Approved by: Platform Owner",
        "Connected: September 25, 2026 at 12:00 PM UTC",
        "Review connected apps: https://dispatch.test/#agents?tab=apps",
    ] {
        assert!(mail.text.contains(line), "{line}\n{}", mail.text);
    }
    assert!(mail.html.contains("Laptop &lt;Claude&gt;") && !mail.html.contains("<Claude>"));
    assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
    let names: Vec<String> = (1..=12).map(|n| format!("DSP {n}")).collect();
    let mail = app_connected(&connected(false, &names));
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
#[test]
fn a_disconnected_app_says_why_in_plain_words() {
    let app = connected(true, &[]);
    let mail = app_disconnected(&app, "refresh_reused");
    assert_eq!(mail.subject, "Dispatch disconnected Claude Code");
    assert!(mail.text.contains("already used was presented again"));
    assert!(
        app_disconnected(&app, "code_reused")
            .text
            .contains("one-time code")
    );
    assert!(mail.text.ends_with(NO_REPLY));
}
