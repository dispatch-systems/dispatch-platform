//! How Dispatch knows a request came from Slack: only one Slack signed with the app's signing
//! secret, as it was sent, and recently, is taken. Slack's own published example is the
//! reference, so the check agrees with Slack rather than with itself. And how an event is
//! routed: a message written to the app directly goes to an integration on there that hears of
//! it, and only when none does is Slack's own to answer.
use super::*;
use crate::backend::{
    app, fixture,
    support::{self, LISTENING},
};
use axum::http::HeaderValue;

const SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";
const AT: i64 = 1_531_420_618;
const BODY: &str = "token=xyzz0WbapA4vBCDEFasx0q6G&team_id=T1DC2JH3J&team_domain=testteamnow&\
channel_id=G8PSS9T3V&channel_name=foobar&user_id=U2CERLKJA&user_name=roadrunner&\
command=%2Fwebhook-collect&text=&response_url=https%3A%2F%2Fhooks.slack.com%2Fcommands%2F\
T1DC2JH3J%2F397700885554%2F96rGlfmibIGlgcZRskXaIFfN&\
trigger_id=398738663015.47445629121.803a0bc887a14d10d2c447fce8b6703c";
const SIGNATURE: &str = "v0=a2114d57b48eac39b9ad189dd8316235a7b4a8d21a10bd27519666489c69b503";

fn headers(timestamp: i64, signature: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-slack-request-timestamp",
        HeaderValue::from_str(&timestamp.to_string()).unwrap(),
    );
    headers.insert(
        "x-slack-signature",
        HeaderValue::from_str(signature).unwrap(),
    );
    headers
}

#[test]
fn only_a_request_slack_signed_as_sent_and_recently_is_taken() {
    let sent = headers(AT, SIGNATURE);
    assert!(signed(SECRET, &sent, BODY.as_bytes(), AT + 60));
    // Signed with another secret: not Slack's for this app.
    assert!(!signed("another-secret", &sent, BODY.as_bytes(), AT));
    // Changed on the way.
    let changed = BODY.replace("roadrunner", "coyote");
    assert!(!signed(SECRET, &sent, changed.as_bytes(), AT));
    // Sent again more than five minutes later, or dated ahead.
    assert!(!signed(SECRET, &sent, BODY.as_bytes(), AT + 301));
    assert!(!signed(SECRET, &sent, BODY.as_bytes(), AT - 301));
    // Its time changed to pass for recent: the signature covers the time too.
    let redated = headers(AT + 600, SIGNATURE);
    assert!(!signed(SECRET, &redated, BODY.as_bytes(), AT + 600));
    // No signature, or not Slack's kind.
    assert!(!signed(SECRET, &headers(AT, ""), BODY.as_bytes(), AT));
    let unversioned = SIGNATURE.trim_start_matches("v0=");
    assert!(!signed(
        SECRET,
        &headers(AT, unversioned),
        BODY.as_bytes(),
        AT
    ));
    assert!(!signed(SECRET, &HeaderMap::new(), BODY.as_bytes(), AT));
}

#[tokio::test]
async fn a_direct_message_is_answered_by_slack_until_an_integration_on_there_takes_it() {
    const TEAM: &str = "T0FIXTURE1";
    const DM: &str = "D0FIXINBOUND";
    let (_root, state, dsp) = support::platform();
    let bot = support::connect(&state, &dsp, TEAM, &app::scopes()).await;
    let written = json!({"type": "message", "channel_type": "im", "channel": DM,
        "user": "U0FIXJORDAN", "text": "Running late", "ts": "1760000000.000100"});
    let answered = ["DSP Messaging is unavailable"];
    // No integration on in the workspace hears of it, so Slack answers it.
    on_event(&bot, &written).await.unwrap();
    assert_eq!(fixture::posted(TEAM, DM), answered);
    assert!(support::heard(&dsp).is_empty());
    // One on there hears of it, as `message.im`, and takes it: Slack answers no more.
    let of = dsp.clone();
    state
        .run(move |db| db.turn_on_slack_integration(&of, TEAM, LISTENING.id, "U1"))
        .await
        .unwrap();
    on_event(&bot, &written).await.unwrap();
    let taken = [(TEAM.to_owned(), "message.im".to_owned())];
    assert_eq!(support::heard(&dsp), taken);
    assert_eq!(fixture::posted(TEAM, DM), answered);
}
