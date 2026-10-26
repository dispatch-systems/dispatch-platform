//! Filing texts: a number's in its own conversation, with whoever Driver Match has it for, else
//! the name the DSP saved for it, else the number; a work phone's under the driver who held it as
//! each was written, by today's holds or a saved day's, and today's again as who held it settles,
//! both ways; a blocked number's out of the list, never waiting on what it sent while blocked.
use super::*;
use crate::backend::{
    conversations,
    support::{self, DRIVERS, held, hold, texted},
    texts::{
        reply,
        storage::{ConversationKind, TextConversation, TextStore},
    },
};
use crate::{Communications, ConversationState};
use chrono::{NaiveDate, TimeZone};
use dispatch_core::{
    manifest::people::{Person, PersonRole},
    testing,
};
use std::collections::BTreeMap;

/// A platform with one DSP, every feature on for it, and its server's state.
fn started() -> (impl Drop, Arc<State>, String) {
    support::install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), dsp)
}
/// The DSP's own timezone, and its day today.
async fn today(state: &Arc<State>, dsp: &str) -> (Tz, NaiveDate) {
    let dsp = dsp.to_owned();
    let zone: Tz = state
        .read(move |db| Ok(db.find_dsp(&dsp)?.timezone))
        .await
        .unwrap()
        .parse()
        .unwrap();
    (zone, clock::today(zone))
}
/// `hour`:`minute` on `day` in `zone`, in milliseconds.
fn at(zone: Tz, day: NaiveDate, hour: u32, minute: u32) -> i64 {
    zone.from_local_datetime(&day.and_hms_opt(hour, minute, 0).unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis()
}
/// The page's list as the member `me` reads it.
async fn listed(state: &Arc<State>, dsp: &str) -> Communications {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| conversations::overview(db, &shared, &dsp, "me", None))
        .await
        .unwrap()
}
/// The conversation the text `id` is filed in.
async fn filed(state: &Arc<State>, dsp: &str, id: &str) -> TextConversation {
    let (dsp, id) = (dsp.to_owned(), id.to_owned());
    state
        .read(move |db| {
            let text = db.communications_text(&dsp, &id)?.unwrap();
            Ok(db
                .communications_text_conversation(&dsp, &text.conversation)?
                .unwrap())
        })
        .await
        .unwrap()
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
        .run(move |db| reply::save_contact(db, &shared, &at, &lakeside, "Lakeside Fleet", "me"))
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
    // Messaging lists none of them, nor counts them.
    let listed = listed(&state, &dsp).await;
    assert_eq!((listed.waiting, listed.conversations.len()), (0, 0));
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
    let answer = reply::send(&state, &c, support::USE, &number.id, "Van 214")
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
    assert!(listed(&state, &dsp).await.conversations.is_empty());
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
    let block = |by: Option<&'static str>| {
        let (shared, at, id) = (state.clone(), dsp.clone(), id.clone());
        state.run(move |db| reply::block(db, &shared, &at, &id, by))
    };
    assert!(block(Some("me")).await.unwrap().blocked);
    texted(&state, &dsp, spam, "Last chance", now() + minute).await;
    let shown = listed(&state, &dsp).await;
    assert_eq!((shown.waiting, shown.conversations.len()), (0, 0));

    // Unblocked, it's listed again, done: what came while it was blocked doesn't wait. What
    // comes now does.
    let unblocked = block(None).await.unwrap();
    assert_eq!(
        (unblocked.blocked, unblocked.state),
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
