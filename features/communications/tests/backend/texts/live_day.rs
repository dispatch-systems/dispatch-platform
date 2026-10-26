//! A work conversation waits only on texts written on the live day: counted while live route data
//! holds today, from what came before its first read, and done with at the stop, as it empties.
use crate::backend::{
    conversations,
    support::{at, filed, held, hold, started, texted, today},
    texts::reading,
};
use crate::ConversationState;
use dispatch_core::State;
use std::sync::Arc;

/// How many conversations wait, as the sidebar counts them.
async fn counted(state: &Arc<State>, dsp: &str) -> usize {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| conversations::waiting(db, &shared, &dsp))
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

#[tokio::test]
async fn a_work_conversation_waits_only_on_the_live_day_from_before_its_first_read_until_the_stop()
 {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let (now, then) = (|h, m| at(zone, day, h, m), |h, m| at(zone, yesterday, h, m));
    // Yesterday, as saved: Avery held 0101, and texted at 5 PM, unanswered as the day stopped.
    let saved = vec![(
        yesterday.to_string(),
        hold("A1", "2065550101", then(8, 0), None),
    )];
    held(&state, &dsp, None, Vec::new(), saved.clone()).await;
    let late = texted(&state, &dsp, "+12065550101", "Keys are in the box", then(17, 0)).await;
    let avery = filed(&state, &dsp, &late).await.id;
    // Today, before the first live read, they text again at 6:30. Without a live day, nothing
    // waits.
    texted(&state, &dsp, "+12065550101", "Which van today?", now(6, 30)).await;
    assert_eq!(counted(&state, &dsp).await, 0);

    // The first live read shows Avery holding 0101 since 7:00: today's text is theirs, and they
    // wait from it, not from yesterday's.
    let live = vec![hold("A1", "2065550101", now(7, 0), None)];
    held(&state, &dsp, Some(&day.to_string()), live, saved.clone()).await;
    assert_eq!(
        shown(&state, &dsp, &avery).await,
        (ConversationState::Waiting, Some(now(6, 30)))
    );
    assert_eq!(counted(&state, &dsp).await, 1);

    // At the stop, live route data empties: still unanswered, it no longer waits, nor counts.
    held(&state, &dsp, None, Vec::new(), saved).await;
    assert_eq!(
        shown(&state, &dsp, &avery).await,
        (ConversationState::Done, None)
    );
    assert_eq!(counted(&state, &dsp).await, 0);
}
