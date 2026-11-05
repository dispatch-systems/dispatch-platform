//! A work conversation waits only on texts written on the live day: counted while live route data
//! holds today, from what came before its first read, and for two hours after the stop, as it
//! empties; then done with, the pages hearing it.
use super::{AFTER_STOP_MS, waits_from};
use crate::ConversationState;
use crate::backend::{
    changes::ChangeStore,
    inbox,
    support::{at, failing, filed, phoned, phones, read_live, saved, started, texted, today},
    texts::{filing::watch_live_day, reading},
};
use dispatch_core::{State, db::now};
use std::sync::Arc;

/// Thirty minutes, in milliseconds.
const HALF_HOUR_MS: i64 = 30 * 60 * 1000;

/// How many conversations wait, as the sidebar counts them.
async fn counted(state: &Arc<State>, dsp: &str) -> usize {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| inbox::waiting(db, &shared, &dsp))
        .await
        .unwrap()
        .count
}
/// The conversation `id`'s state and since when it waits, as the page reads it.
async fn shown(state: &Arc<State>, dsp: &str, id: &str) -> (ConversationState, Option<i64>) {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    let conversation = state
        .read(move |db| reading::one(db, &shared, &dsp, &id))
        .await
        .unwrap();
    (conversation.state, conversation.waiting_since)
}

/// Avery, A1, has the work phone 0101 today as live route data reads it, until the stop at `stop`,
/// as Communications first sees it, and the day is saved; then Avery texts, now.
async fn texted_after_the_stop(state: &Arc<State>, dsp: &str, stop: i64) -> String {
    let today = today(state, dsp).await.1;
    let avery = &[("A1", "2065550101")];
    phoned(
        state,
        dsp,
        Some(&today.to_string()),
        phones(avery),
        Vec::new(),
    )
    .await;
    read_live(dsp, None, Vec::new());
    watch_live_day(state, dsp, stop).await.unwrap();
    phoned(state, dsp, None, Vec::new(), vec![saved(today, avery)]).await;
    let text = texted(state, dsp, "+12065550101", "Locked out of the van", now()).await;
    filed(state, dsp, &text).await.id
}

#[tokio::test]
async fn a_work_conversation_waits_only_on_the_live_day_from_before_its_first_read_into_the_stop() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let (now, then) = (|h, m| at(zone, day, h, m), |h, m| at(zone, yesterday, h, m));
    // Yesterday, as saved: Avery had 0101, and texted at 5 PM, unanswered as the day stopped.
    let earlier = vec![saved(yesterday, &[("A1", "2065550101")])];
    phoned(&state, &dsp, None, Vec::new(), earlier.clone()).await;
    let late = texted(
        &state,
        &dsp,
        "+12065550101",
        "Keys are in the box",
        then(17, 0),
    )
    .await;
    let avery = filed(&state, &dsp, &late).await.id;
    // Today, before the first live read, they text again at 6:30. Without a live day, nothing
    // waits.
    texted(&state, &dsp, "+12065550101", "Which van today?", now(6, 30)).await;
    assert_eq!(counted(&state, &dsp).await, 0);

    // The first live read shows Avery with 0101: today's text is theirs, and they wait from it,
    // not from yesterday's.
    let live = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&day.to_string()), live, earlier.clone()).await;
    assert_eq!(
        shown(&state, &dsp, &avery).await,
        (ConversationState::Waiting, Some(now(6, 30)))
    );
    assert_eq!(counted(&state, &dsp).await, 1);

    // At the stop, live route data empties: still unanswered, it waits, and counts, two hours more.
    phoned(&state, &dsp, None, Vec::new(), earlier).await;
    assert_eq!(
        shown(&state, &dsp, &avery).await,
        (ConversationState::Waiting, Some(now(6, 30)))
    );
    assert_eq!(counted(&state, &dsp).await, 1);
}

#[tokio::test]
async fn a_work_text_half_an_hour_after_the_stop_waits_and_counts_until_two_hours_after_it() {
    let (_root, state, dsp) = started();
    let stop = now() - HALF_HOUR_MS;
    let avery = texted_after_the_stop(&state, &dsp, stop).await;
    let (waiting, _) = shown(&state, &dsp, &avery).await;
    assert_eq!(waiting, ConversationState::Waiting);
    assert_eq!(counted(&state, &dsp).await, 1);

    // As the two hours end, it stops waiting, which a page with it open hears, though Routes can't
    // read the day then.
    let (at, end) = (dsp.clone(), stop + AFTER_STOP_MS);
    let waits = state
        .read(move |db| Ok((waits_from(db, &at, end - 1)?, waits_from(db, &at, end)?)))
        .await
        .unwrap();
    assert!(matches!(waits, (Some(_), None)), "{waits:?}");
    let at = dsp.clone();
    let before = state
        .read(move |db| db.communications_revision(&at))
        .await
        .unwrap();
    failing(&dsp, true);
    watch_live_day(&state, &dsp, stop + AFTER_STOP_MS)
        .await
        .unwrap();
    let at = dsp.clone();
    let (changed, _) = state
        .read(move |db| db.communications_changed_since(&at, before))
        .await
        .unwrap();
    assert!(changed.unwrap().contains(&avery));
}

#[tokio::test]
async fn a_work_text_two_and_a_half_hours_after_the_stop_neither_waits_nor_counts() {
    let (_root, state, dsp) = started();
    let stop = now() - AFTER_STOP_MS - HALF_HOUR_MS;
    let avery = texted_after_the_stop(&state, &dsp, stop).await;
    assert_eq!(
        shown(&state, &dsp, &avery).await,
        (ConversationState::Done, None)
    );
    assert_eq!(counted(&state, &dsp).await, 0);
}
