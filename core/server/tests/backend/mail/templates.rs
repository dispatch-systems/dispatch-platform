use super::*;
fn sample(onboarding: bool, inviter: &'static str) -> Message {
    invitation(&Invitation {
        origin: "https://dispatch.test",
        dev: false,
        to: "alex@dispatch.test",
        inviter,
        dsp: "North <Line> Logistics",
        role: "manager",
        url: "https://dispatch.test/#invite?token=abc",
        expires_at: 1_790_337_600_000,
        onboarding,
    })
}
#[test]
fn avatar_matches_dashboard_initials_and_tone() {
    let html = avatar("Northline Logistics");
    assert!(html.contains(">NL<") && html.contains("#eaf0f7"));
    assert!(avatar("acme").contains(">AC<") && avatar("").contains(">?<"));
}
#[test]
fn invitation_names_the_inviter_and_escapes_the_dsp() {
    let mail = sample(false, "Alex Morgan");
    assert_eq!(mail.subject, "Join North <Line> Logistics on Dispatch");
    assert!(mail.html.contains("North &lt;Line&gt; Logistics") && !mail.html.contains("<Line>"));
    assert!(mail.html.contains(">Accept invitation</a>"));
    assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
    assert!(mail.text.contains("Alex Morgan invited you to join"));
    assert!(mail.text.contains("expires on September 25, 2026."));
    assert!(
        sample(false, "")
            .text
            .contains("You've been invited to join")
    );
}
#[test]
fn onboarding_hides_the_placeholder_dsp_name() {
    let mail = sample(true, "Alex Morgan");
    assert_eq!(mail.subject, "Set up your DSP on Dispatch");
    assert!(mail.html.contains(">Start DSP onboarding</a>") && !mail.html.contains("Logistics"));
}

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
#[test]
fn a_disconnected_app_says_why_in_plain_words() {
    let nothing = reading(&[], false);
    let app = connected(true, &[], &nothing);
    let mail = app_disconnected(&app, "refresh_reused");
    assert_eq!(mail.subject, "Dispatch disconnected Claude Code");
    assert!(mail.text.contains("already used was presented again"));
    assert!(
        mail.text.contains("Access: Reads nothing\n"),
        "{}",
        mail.text
    );
    assert!(
        app_disconnected(&app, "code_reused")
            .text
            .contains("one-time code")
    );
    assert!(mail.text.ends_with(NO_REPLY));
}

#[test]
fn every_email_says_replies_are_not_read() {
    let mail = reset(
        "https://dispatch.example",
        false,
        "owner@example.com",
        "https://dispatch.example/#reset?token=t",
    );
    assert!(mail.text.ends_with(NO_REPLY) && mail.html.contains(NO_REPLY));
}
