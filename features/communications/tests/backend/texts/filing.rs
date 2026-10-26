//! Filing texts: a number's in its own conversation, with whoever Driver Match has it for, else
//! the name the DSP saved for it, else the number; a work phone's under the driver who held it as
//! each was written, by today's holds or a saved day's, and a day's again as who held it settles,
//! both ways, while Routes keeps the day's holds; a blocked number's out of the list, never
//! waiting on what it sent while blocked.
use super::*;
use crate::backend::{
    conversations,
    storage::CommunicationsStore,
    support::{self, DRIVERS, at, filed, held, hold, route, routed, started, texted, today},
    texts::{
        contacts, reply,
        storage::{ConversationKind, TextStore},
    },
};
use crate::{Communications, ConversationState};
use dispatch_core::manifest::people::{Person, PersonRole};
use std::collections::BTreeMap;

/// The page's list as the member `me` reads it.
async fn listed(state: &Arc<State>, dsp: &str) -> Communications {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| conversations::overview(db, &shared, &dsp, "me", None))
        .await
        .unwrap()
}
/// Blocks the number of the conversation `id`, as the member `me`, or unblocks it: whether it's
/// blocked now, and its state.
async fn block(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    blocked: bool,
) -> (bool, ConversationState) {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    let by = blocked.then_some("me");
    let after = state
        .run(move |db| contacts::block(db, &shared, &dsp, &id, by))
        .await
        .unwrap();
    (after.blocked, after.state)
}
/// Moves the DSP to `zone`, as its owner might on Settings.
async fn moved(state: &Arc<State>, dsp: &str, zone: Tz) {
    let dsp = dsp.to_owned();
    state
        .run(move |db| {
            let name = db.find_dsp(&dsp)?.name;
            db.keep_dsp_profile(&dsp, &name, zone.name(), None, None)
        })
        .await
        .unwrap();
}
/// Whose a driver's conversation is, by their ID of the stand-in's source.
fn driver(id: &str) -> Option<(String, String)> {
    Some((DRIVERS.as_str().to_owned(), id.to_owned()))
}
/// A driver Driver Match has the phone number `phone` for.
fn person(code: &str, name: &str, phone: &str) -> Person {
    Person {
        code: code.into(),
        name: name.into(),
        names: vec![name.into()],
        role: PersonRole::Driver,
        title: None,
        ids: BTreeMap::new(),
        emails: Vec::new(),
        phones: vec![phone.into()],
        confirmed: true,
        photo: None,
    }
}

#[tokio::test]
async fn a_numbers_texts_are_with_whom_driver_match_has_it_for_else_the_name_saved_else_the_number()
{
    let (_root, state, dsp) = started();
    support::know(
        &state,
        &dsp,
        vec![person("brianna", "Brianna Scott", "(253) 555-0177")],
        Vec::new(),
        Vec::new(),
    );
    let minute = 60_000;
    texted(
        &state,
        &dsp,
        "+12535550177",
        "Can I swap Saturday?",
        now() - 3 * minute,
    )
    .await;
    let lakeside = texted(
        &state,
        &dsp,
        "+14255550134",
        "Van 208 is ready",
        now() - 2 * minute,
    )
    .await;
    texted(
        &state,
        &dsp,
        "+14255550199",
        "Is this dispatch?",
        now() - minute,
    )
    .await;
    let lakeside = filed(&state, &dsp, &lakeside).await.id;
    let (shared, at) = (state.clone(), dsp.clone());
    state
        .run(move |db| contacts::save_contact(db, &shared, &at, &lakeside, "Lakeside Fleet", "me"))
        .await
        .unwrap();

    let listed = listed(&state, &dsp).await;
    assert_eq!(listed.waiting, 3);
    let shown: Vec<_> = listed
        .conversations
        .iter()
        .map(|each| {
            (
                each.kind,
                each.with.name.as_str(),
                each.with.code.as_deref(),
                each.contact,
                each.state,
            )
        })
        .collect();
    use ConversationKind::Number;
    use ConversationState::Waiting;
    assert_eq!(
        shown,
        [
            (Number, "Brianna Scott", Some("brianna"), false, Waiting),
            (Number, "Lakeside Fleet", None, true, Waiting),
            (Number, "(425) 555-0199", None, false, Waiting),
        ]
    );
}

#[tokio::test]
async fn a_work_phones_texts_are_the_drivers_who_held_it_as_each_was_written_today_or_a_saved_day()
{
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let (now, then) = (|h, m| at(zone, day, h, m), |h, m| at(zone, yesterday, h, m));
    // Today, as read live: Avery held 0101 until noon, Blake since; Dana held 0103 until 10:00.
    // Yesterday, as saved: Casey held 0101, and 0102, which no one holds today.
    held(
        &state,
        &dsp,
        Some(&day.to_string()),
        vec![
            hold("avery", "2065550101", now(8, 0), Some(now(12, 0))),
            hold("blake", "2065550101", now(12, 0), None),
            hold("dana", "2065550103", now(8, 0), Some(now(10, 0))),
        ],
        vec![
            (
                yesterday.to_string(),
                hold("casey", "2065550101", then(7, 0), None),
            ),
            (
                yesterday.to_string(),
                hold("casey", "2065550102", then(7, 0), None),
            ),
        ],
    )
    .await;
    let phone = "+12065550101";
    for (written, whose) in [
        // Before the day's first hold, the day's first holder's.
        (now(6, 45), driver("avery")),
        (now(9, 0), driver("avery")),
        (now(13, 0), driver("blake")),
        (then(10, 0), driver("casey")),
    ] {
        let text = texted(&state, &dsp, phone, "Gate code?", written).await;
        let filed = filed(&state, &dsp, &text).await;
        assert_eq!((filed.kind, filed.driver), (ConversationKind::Work, whose));
    }
    // A work phone no one held as it was written is still a work phone, the phone's own.
    for (phone, written) in [("+12065550103", now(11, 0)), ("+12065550102", now(9, 0))] {
        let text = texted(&state, &dsp, phone, "Who has the charger?", written).await;
        let filed = filed(&state, &dsp, &text).await;
        assert_eq!(
            (filed.kind, filed.driver, filed.number.as_deref()),
            (ConversationKind::Work, None, Some(phone))
        );
    }
    // Messaging lists none of them: they're Today's Routes', those waiting on the live day
    // counted, and yesterday's Casey's not.
    let listed = listed(&state, &dsp).await;
    assert!(
        listed
            .conversations
            .iter()
            .all(|each| each.kind == ConversationKind::Work)
    );
    assert_eq!(listed.waiting, 4);
}

#[tokio::test]
async fn todays_texts_both_ways_go_to_whoever_holds_the_work_phone_once_live_route_data_shows_it() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    // Before the day's first live read, a phone no one has held yet texts: it's a number's.
    let asked = texted(
        &state,
        &dsp,
        "+12065550104",
        "Which van?",
        at(zone, day, 6, 30),
    )
    .await;
    let number = filed(&state, &dsp, &asked).await;
    assert_eq!(number.kind, ConversationKind::Number);
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let answer = reply::send(&state, &c, support::USE, &number.id, "Van 214", &[])
        .await
        .unwrap();

    // The first live read shows Avery holding it since 7:00: both are theirs, and the number's
    // conversation, holding none, leaves the list.
    held(
        &state,
        &dsp,
        Some(&day.to_string()),
        vec![hold("avery", "2065550104", at(zone, day, 7, 0), None)],
        Vec::new(),
    )
    .await;
    for text in [&asked, &answer.id] {
        assert_eq!(filed(&state, &dsp, text).await.driver, driver("avery"));
    }
    let listed = listed(&state, &dsp).await;
    assert!(listed.conversations.iter().all(|each| each.id != number.id));
}

#[tokio::test]
async fn a_saved_days_texts_go_to_whoever_held_the_work_phone_once_its_holds_are_saved_after_them()
{
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    // Yesterday where the DSP is; and a day long gone in Beirut, whose clocks went from midnight
    // to 01:00 that day, written in the hour after.
    let beirut: Tz = "Asia/Beirut".parse().unwrap();
    let skipped = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
    for (zone, day, phone, (hour, minute)) in [
        (zone, day.pred_opt().unwrap(), "2065550101", (18, 0)),
        (beirut, skipped, "2065550102", (1, 30)),
    ] {
        moved(&state, &dsp, zone).await;
        // The day's text, relayed late, comes before the day is saved: no hold says whose it is.
        let late = texted(
            &state,
            &dsp,
            &format!("+1{phone}"),
            "Dropped the keys at the station",
            at(zone, day, hour, minute),
        )
        .await;
        assert_eq!(
            filed(&state, &dsp, &late).await.kind,
            ConversationKind::Number
        );

        // Saved with Casey holding the phone since 7:00: it's theirs.
        let saved = hold("casey", phone, at(zone, day, 7, 0), None);
        held(
            &state,
            &dsp,
            None,
            Vec::new(),
            vec![(day.to_string(), saved)],
        )
        .await;
        let filed = filed(&state, &dsp, &late).await;
        assert_eq!(
            (filed.kind, filed.driver),
            (ConversationKind::Work, driver("casey")),
            "{day}"
        );
    }
}

#[tokio::test]
async fn a_days_texts_stay_with_whoever_held_the_work_phone_once_routes_empties_it_unsaved() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    // Yesterday, while live route data held it, Avery held 0101 and texted from it.
    let since = at(zone, yesterday, 7, 0);
    let avery = vec![hold("avery", "2065550101", since, None)];
    held(
        &state,
        &dsp,
        Some(&yesterday.to_string()),
        avery,
        Vec::new(),
    )
    .await;
    let written = at(zone, yesterday, 16, 0);
    let text = texted(&state, &dsp, "+12065550101", "Back at the station", written).await;
    assert_eq!(filed(&state, &dsp, &text).await.driver, driver("avery"));

    // Emptied unsaved, as the next day's first read leaves it: still theirs.
    held(&state, &dsp, None, Vec::new(), Vec::new()).await;
    assert_eq!(filed(&state, &dsp, &text).await.driver, driver("avery"));
}

#[tokio::test]
async fn a_blocked_numbers_conversation_leaves_the_list_and_never_waits_on_what_it_sent_while_blocked()
 {
    let (_root, state, dsp) = started();
    let minute = 60_000;
    let spam = "+18885550142";
    let first = texted(&state, &dsp, spam, "Win a cruise", now() - 3 * minute).await;
    let id = filed(&state, &dsp, &first).await.id;
    let (at, done) = (dsp.clone(), id.clone());
    state
        .run(move |db| db.mark_communications_text_done(&at, &done, "me"))
        .await
        .unwrap();
    assert!(block(&state, &dsp, &id, true).await.0);
    texted(&state, &dsp, spam, "Last chance", now() + minute).await;
    let shown = listed(&state, &dsp).await;
    assert_eq!((shown.waiting, shown.conversations.len()), (0, 0));

    // Unblocked, it's listed again, done: what came while it was blocked doesn't wait. What
    // comes now does.
    assert_eq!(
        block(&state, &dsp, &id, false).await,
        (false, ConversationState::Done)
    );
    let shown = listed(&state, &dsp).await;
    assert_eq!((shown.waiting, shown.conversations.len()), (0, 1));
    let now_on = texted(&state, &dsp, spam, "Hello?", now() + 2 * minute).await;
    let waits = filed(&state, &dsp, &now_on).await;
    let written = state
        .read({
            let dsp = dsp.clone();
            move |db| Ok(db.communications_text(&dsp, &now_on)?.unwrap().at)
        })
        .await
        .unwrap();
    assert_eq!(waits.waiting_since, Some(written));
}

#[tokio::test]
async fn what_a_number_sent_while_blocked_never_alerts_once_it_is_unblocked() {
    let (_root, state, dsp) = started();
    let minute = 60_000;
    let number = "+14255550123";
    let asked = texted(
        &state,
        &dsp,
        number,
        "Can I swap Saturday?",
        now() - 3 * minute,
    )
    .await;
    let id = filed(&state, &dsp, &asked).await.id;
    block(&state, &dsp, &id, true).await;
    texted(&state, &dsp, number, "Never mind", now() - minute).await;
    block(&state, &dsp, &id, false).await;

    // Unblocked, it waits on what it sent before, and alerts with that.
    let (shared, at) = (state.clone(), dsp.clone());
    let waiting = state
        .read(move |db| conversations::waiting(db, &shared, &at))
        .await
        .unwrap();
    let latest: Vec<_> = waiting
        .latest
        .iter()
        .map(|each| (each.conversation_id.as_str(), each.message.text.as_str()))
        .collect();
    assert_eq!(latest, [(id.as_str(), "Can I swap Saturday?")]);
}

#[tokio::test]
async fn a_drivers_work_conversation_changes_as_they_take_and_return_a_work_phone_with_no_text() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let today = day.to_string();
    // Avery drives CX14 today, holding no work phone, and their conversation is opened, empty.
    routed(
        &dsp,
        vec![route(
            &today,
            "A1",
            "CX14",
            "1FTVAN214",
            120,
            at(zone, day, 9, 20),
        )],
    );
    held(&state, &dsp, Some(&today), Vec::new(), Vec::new()).await;
    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let avery = state
        .run(move |db| contacts::open_driver(db, &shared, &at_dsp, "A1"))
        .await
        .unwrap()
        .id;
    // Whether each change of who holds the work phones is heard by a page with it open.
    let heard = |holds| {
        let (state, dsp, avery, today) = (state.clone(), dsp.clone(), avery.clone(), today.clone());
        async move {
            let at = dsp.clone();
            let before = state
                .read(move |db| db.communications_revision(&at))
                .await
                .unwrap();
            held(&state, &dsp, Some(&today), holds, Vec::new()).await;
            let (changed, _) = state
                .read(move |db| db.communications_changed_since(&dsp, before))
                .await
                .unwrap();
            changed.unwrap().contains(&avery)
        }
    };

    let (taken, returned) = (at(zone, day, 7, 0), at(zone, day, 7, 30));
    assert!(heard(vec![hold("A1", "2065550101", taken, None)]).await);
    assert!(heard(vec![hold("A1", "2065550101", taken, Some(returned))]).await);
}
