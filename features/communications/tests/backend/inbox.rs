//! What the sidebar reads of the conversations waiting: the latest message from whom each is
//! with, which a message the DSP sends a group leaves as it was.
use super::*;
use crate::backend::{
    groups::send,
    reply,
    support::{self, USE},
};
use std::sync::Arc;

const TEAM: &str = "T0FIXTURE2";

/// Each waiting conversation's latest message from whom it's with, by who wrote it and its text.
async fn latest(state: &Arc<State>, dsp: &str) -> Vec<(String, String)> {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| waiting(db, &shared, &dsp))
        .await
        .unwrap()
        .latest
        .into_iter()
        .map(|each| (each.from, each.message.text))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn waiting_names_the_latest_message_each_waits_on_from_whom_its_with() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let casey = |text: &str| vec![("Casey Rivera".to_owned(), text.to_owned())];
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    assert_eq!(latest(&state, &dsp).await, casey("Where do I park?"));

    // A message to a group answers no one: Casey's question is still the latest they wrote.
    let people = [(TEAM.to_owned(), "U0FIXCASEY".to_owned())];
    send::create(
        &state,
        &c,
        USE,
        "Drivers",
        &people,
        "Pick up your vests",
        &[],
    )
    .await
    .unwrap();
    send::drain(&state, &dsp).await;
    assert_eq!(latest(&state, &dsp).await, casey("Where do I park?"));

    // Written again, the new message; answered, nothing waits.
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Hello?").await;
    assert_eq!(latest(&state, &dsp).await, casey("Hello?"));
    let conversation = support::conversation_with(&state, &dsp, TEAM, "U0FIXCASEY")
        .await
        .unwrap();
    reply::send(&state, &c, USE, &conversation.id, "By the door", &[])
        .await
        .unwrap();
    assert!(latest(&state, &dsp).await.is_empty());
}
