//! The profile beside a conversation: the member's route on the DSP's own day today, with what
//! the DSP calls its van, and how to reach them, each only while the feature that knows it is on.
use super::*;
use crate::backend::support::{self, DRIVERS, route, van};
use crate::integrations::messages::incoming;
use dispatch_core::{db::now, foundation::clock, manifest::people::PersonRole};
use dispatch_slack::kit::{self, Bot};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

/// A workspace the tests connect, named by its ID.
const TEAM: &str = "T0FIXTURE2";
/// When a route's wave starts, in milliseconds, and an hour and a day of them.
const WAVE: i64 = 1_792_482_000_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

/// The conversation the member `user` started.
async fn started(state: &Arc<State>, bot: &Bot, user: &str) -> String {
    let event = json!({"type": "message", "channel_type": "im", "channel": format!("D{user}"),
        "user": user, "text": "Van won't start", "ts": incoming::ts(now() - 1)});
    incoming::heard(state, &bot.dsp, TEAM, &event)
        .await
        .unwrap();
    support::conversation_with(state, &bot.dsp, TEAM, user)
        .await
        .unwrap()
        .id
}
/// The DSP's own days today and yesterday.
async fn days(state: &Arc<State>, dsp: &str) -> (String, String) {
    let dsp = dsp.to_owned();
    let zone = state.read(move |db| Ok(db.find_dsp(&dsp)?.timezone));
    let today = clock::today(zone.await.unwrap().parse().unwrap());
    (today.to_string(), today.pred_opt().unwrap().to_string())
}
/// The profile beside the conversation `id`.
async fn profile(state: &Arc<State>, dsp: &str, id: &str) -> Profile {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    state
        .read(move |db| member(db, &shared, &dsp, &id))
        .await
        .unwrap()
}
/// Jordan Ellis, as Driver Match knows them: by their Slack account and two drivers' IDs.
fn jordan() -> Person {
    Person {
        code: "jordan".into(),
        name: "Jordan Ellis".into(),
        names: vec!["Jordan Ellis".into()],
        role: PersonRole::Driver,
        title: None,
        ids: BTreeMap::from([
            (kit::SOURCE, vec![kit::member_id(TEAM, "U0FIXJORDAN")]),
            (DRIVERS, vec!["A1".into(), "A2".into()]),
        ]),
        emails: vec!["jordan.ellis@example.com".into(), "je@example.org".into()],
        phones: vec!["+16125550100".into(), "+16515550100".into()],
        confirmed: true,
        photo: None,
    }
}
#[tokio::test]
async fn the_profile_shows_the_route_driven_today_and_how_to_reach_the_member() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "U0FIXJORDAN").await;
    let (today, yesterday) = days(&state, &dsp).await;
    // Their route today, beside a rescue they drove today and their route yesterday.
    support::know(
        &state,
        &dsp,
        vec![jordan()],
        vec![
            route(&yesterday, "A1", "CX9", "1FTVAN009", 190, WAVE - DAY),
            route(&today, "A2", "CX20", "1FTVAN020", 12, WAVE + HOUR),
            route(&today, "A1", "CX14", "1FTVAN214", 168, WAVE),
        ],
        vec![van("1FTVAN214", "214"), van("1FTVAN009", "9")],
    );
    support::switch(&state, &dsp, "driving", true).await;
    support::switch(&state, &dsp, "fleet", true).await;
    let shown = profile(&state, &dsp, &id).await;
    let today = shown.today.expect("a route today");
    assert_eq!(today.route.as_deref(), Some("CX14"));
    assert_eq!(today.van.as_deref(), Some("214"));
    assert_eq!((today.wave_at, today.stops), (Some(WAVE), Some(168)));
    assert_eq!(shown.phone.as_deref(), Some("+16125550100"));
    assert_eq!(shown.email.as_deref(), Some("jordan.ellis@example.com"));
    assert_eq!(shown.slack.as_deref(), Some(TEAM));

    // With Vans off, the route without its van; with Routes off, no route at all.
    support::switch(&state, &dsp, "fleet", false).await;
    let shown = profile(&state, &dsp, &id).await;
    let today = shown.today.expect("a route today");
    assert_eq!((today.route.as_deref(), today.van), (Some("CX14"), None));
    support::switch(&state, &dsp, "fleet", true).await;
    support::switch(&state, &dsp, "driving", false).await;
    let shown = profile(&state, &dsp, &id).await;
    assert!(shown.today.is_none());
    assert_eq!(shown.email.as_deref(), Some("jordan.ellis@example.com"));
}

#[tokio::test]
async fn the_profile_shows_no_route_of_another_day_and_no_contact_for_someone_unknown() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let jordan_id = started(&state, &bot, "U0FIXJORDAN").await;
    let cam = started(&state, &bot, "U0FIXCAM").await;
    let (_, yesterday) = days(&state, &dsp).await;
    support::know(
        &state,
        &dsp,
        vec![jordan()],
        vec![route(&yesterday, "A1", "CX9", "1FTVAN009", 190, WAVE - DAY)],
        vec![van("1FTVAN009", "9")],
    );
    support::switch(&state, &dsp, "driving", true).await;
    support::switch(&state, &dsp, "fleet", true).await;
    // Yesterday's route is never today's.
    let shown = profile(&state, &dsp, &jordan_id).await;
    assert!(shown.today.is_none());
    assert_eq!(shown.phone.as_deref(), Some("+16125550100"));
    // Someone Driver Match doesn't know: their workspace alone.
    let shown = profile(&state, &dsp, &cam).await;
    assert!(shown.today.is_none());
    assert_eq!((shown.phone, shown.email), (None, None));
    assert_eq!(shown.slack.as_deref(), Some(TEAM));
}
