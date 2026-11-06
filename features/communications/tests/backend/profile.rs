//! The profile beside a conversation: how to reach whom it's with; their route on the DSP's own day
//! today, as the conversation's read has it, with what the DSP calls its van, each only while the
//! feature that knows it is on, whose van the head of their conversations in Slack and by their
//! own number shows; and whether they're working, which only live route data having them on a
//! route today proves.
use super::*;
use crate::api::types::Today;
use crate::backend::support::{self, DRIVERS, phoned, route, routed, texted, van};
use crate::backend::{conversations, texts::reading};
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
/// The route today the Slack conversation `id`'s read has.
async fn today_of(state: &Arc<State>, dsp: &str, id: &str) -> Option<Today> {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    state
        .read(move |db| Ok(conversations::opened(db, &shared, &dsp, &id, None)?.today))
        .await
        .unwrap()
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
        added_phones: Vec::new(),
        confirmed: true,
        photo: None,
    }
}
#[tokio::test]
async fn the_conversation_comes_with_the_route_driven_today_and_the_profile_shows_how_to_reach_the_member()
 {
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
    let today = today_of(&state, &dsp, &id).await.expect("a route today");
    assert_eq!(today.route.as_deref(), Some("CX14"));
    assert_eq!(today.van.as_deref(), Some("214"));
    assert_eq!((today.wave_at, today.stops), (Some(WAVE), Some(168)));
    let shown = profile(&state, &dsp, &id).await;
    assert_eq!(shown.phone.as_deref(), Some("+16125550100"));
    assert_eq!(shown.email.as_deref(), Some("jordan.ellis@example.com"));
    assert_eq!(shown.slack.as_deref(), Some(TEAM));

    // With Vans off, the route without its van; with Routes off, no route at all.
    support::switch(&state, &dsp, "fleet", false).await;
    let today = today_of(&state, &dsp, &id).await.expect("a route today");
    assert_eq!((today.route.as_deref(), today.van), (Some("CX14"), None));
    support::switch(&state, &dsp, "fleet", true).await;
    support::switch(&state, &dsp, "driving", false).await;
    assert!(today_of(&state, &dsp, &id).await.is_none());
    let shown = profile(&state, &dsp, &id).await;
    assert_eq!(shown.email.as_deref(), Some("jordan.ellis@example.com"));
}

#[tokio::test]
async fn no_route_of_another_day_comes_with_the_conversation_and_no_contact_with_someone_unknown() {
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
    assert!(today_of(&state, &dsp, &jordan_id).await.is_none());
    let shown = profile(&state, &dsp, &jordan_id).await;
    assert_eq!(shown.phone.as_deref(), Some("+16125550100"));
    // Someone Driver Match doesn't know: their workspace alone.
    assert!(today_of(&state, &dsp, &cam).await.is_none());
    let shown = profile(&state, &dsp, &cam).await;
    assert_eq!((shown.phone, shown.email), (None, None));
    assert_eq!(shown.slack.as_deref(), Some(TEAM));
}

#[tokio::test]
async fn their_slack_and_own_numbers_conversations_come_with_the_van_they_drive_today() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let slack = started(&state, &bot, "U0FIXJORDAN").await;
    let text = texted(&state, &dsp, "+16125550100", "Van won't start", now() - 1).await;
    let number = support::filed(&state, &dsp, &text).await.id;
    let (today, yesterday) = days(&state, &dsp).await;
    support::know(
        &state,
        &dsp,
        vec![jordan()],
        vec![route(&yesterday, "A1", "CX9", "1FTVAN009", 190, WAVE - DAY)],
        vec![van("1FTVAN214", "214"), van("1FTVAN009", "9")],
    );
    support::switch(&state, &dsp, "driving", true).await;
    support::switch(&state, &dsp, "fleet", true).await;
    let vans = || {
        let (shared, dsp, slack, number) =
            (state.clone(), dsp.clone(), slack.clone(), number.clone());
        state.read(move |db| {
            let van = |today: Option<Today>| today.and_then(|today| today.van);
            Ok((
                van(conversations::opened(db, &shared, &dsp, &slack, None)?.today),
                van(reading::opened(db, &shared, &dsp, &number, None)?.today),
            ))
        })
    };
    // No van today, only yesterday's: none in either.
    assert_eq!(vans().await.unwrap(), (None, None));

    support::know(
        &state,
        &dsp,
        vec![jordan()],
        vec![route(&today, "A1", "CX14", "1FTVAN214", 168, WAVE)],
        vec![van("1FTVAN214", "214")],
    );
    let assigned = Some("214".to_owned());
    assert_eq!(vans().await.unwrap(), (assigned.clone(), assigned));
}

#[tokio::test]
async fn only_a_route_live_route_data_holds_today_has_them_working_and_their_number_shows_their_profile()
 {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let slack = started(&state, &bot, "U0FIXJORDAN").await;
    let written = texted(&state, &dsp, "+16125550100", "Van won't start", now() - 1).await;
    let number = support::filed(&state, &dsp, &written).await.id;
    let (today, _) = days(&state, &dsp).await;
    let cx14 = route(&today, "A1", "CX14", "1FTVAN214", 168, WAVE);
    support::know(&state, &dsp, vec![jordan()], vec![cx14.clone()], Vec::new());
    for feature in ["driving", "live"] {
        support::switch(&state, &dsp, feature, true).await;
    }
    let working = || {
        let (shared, dsp, slack, number) =
            (state.clone(), dsp.clone(), slack.clone(), number.clone());
        state.read(move |db| {
            Ok((
                conversations::opened(db, &shared, &dsp, &slack, None)?.working,
                reading::opened(db, &shared, &dsp, &number, None)?.working,
            ))
        })
    };
    // Routes has them on a route today, but live route data holds no day yet: no proof.
    assert_eq!(working().await.unwrap(), (false, false));

    // Live route data holds today, without them on a route: not working.
    phoned(&state, &dsp, Some(&today), Vec::new(), Vec::new()).await;
    assert_eq!(working().await.unwrap(), (false, false));

    // With them on a route: working, in Slack and by their own number alike.
    routed(&dsp, vec![cx14]);
    phoned(&state, &dsp, Some(&today), Vec::new(), Vec::new()).await;
    assert_eq!(working().await.unwrap(), (true, true));

    // Their own number's conversation has their profile, as their Slack one does.
    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let shown = state
        .read(move |db| text(db, &shared, &at_dsp, &number))
        .await
        .unwrap();
    assert_eq!(shown.email.as_deref(), Some("jordan.ellis@example.com"));
}
