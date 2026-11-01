//! A conversation of either kind, through the dispatcher: what a message written in it may carry,
//! as reading it says, by text as a text does and in Slack as Slack does; and an answer that went,
//! kept though its writer's access ended as it went, read back to them only while they may read it.
use super::*;
use crate::backend::{
    compose::SLACK,
    support::{self, filed, switch, texted},
    texts::storage::TextStore,
};
use dispatch_core::db::now;
use std::task::Poll;

const TEAM: &str = "T0FIXTURE2";

/// What a message written in the conversation `reference` may carry, as reading it says: its
/// characters, its pictures, and the largest each may be.
async fn limits(state: &Arc<State>, dsp: &str, reference: Reference) -> (usize, usize, u64) {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    let limits = state
        .read(move |db| Ok(opened(db, &shared, &dsp, &reference, None)?.limits))
        .await
        .unwrap();
    (limits.text, limits.photos, limits.largest)
}

#[tokio::test]
async fn a_conversation_says_what_a_message_in_it_may_carry_as_its_kind_does() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    let slack = support::conversation_with(&state, &dsp, TEAM, "U0FIXCASEY")
        .await
        .unwrap()
        .id;
    let asked = texted(&state, &dsp, "+12535550177", "Which van?", now()).await;
    let text = filed(&state, &dsp, &asked).await.id;
    let slacks = (SLACK.text, SLACK.photos, SLACK.largest);

    // By text, as a text carries: 10,000 characters and ten pictures of up to 20 MB, where a Slack
    // message carries 3,000.
    let texts = (10_000, 10, 20 * 1024 * 1024);
    assert_eq!(limits(&state, &dsp, Reference::Text(text)).await, texts);
    assert_eq!(limits(&state, &dsp, Reference::Slack(slack)).await, slacks);
}

#[tokio::test]
async fn an_answer_that_went_stays_kept_though_its_writers_access_ended_as_it_went() {
    let (_root, state, dsp) = support::started();
    let asked = texted(&state, &dsp, "+12535550177", "Can I swap Saturday?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    // Communications goes off for the DSP once the answer is queued, and with it their permission:
    // it went and is kept, but nothing of the conversation is read back to them. Each database
    // call waits in line for a slot, first come first served, and this test holds every slot: the
    // write that queues the answer waits first, the switch next, and once one slot is let go, the
    // switch goes right after the queue and before anything the send reads afterwards, which waits
    // behind it.
    let mut slots = Vec::new();
    while let Ok(slot) = state.db_slots.clone().try_acquire_owned() {
        slots.push(slot);
    }
    let reference = Reference::Text(conversation.clone());
    let sending = send(&state, &c, support::USE, &reference, "Swap it", &[]);
    let switching = switch(&state, &dsp, "communications", false);
    tokio::pin!(sending, switching);
    std::future::poll_fn(|cx| {
        assert!(sending.as_mut().poll(cx).is_pending());
        assert!(switching.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    slots.pop();
    let ((), sent) = tokio::join!(switching, sending);
    assert_eq!(sent.unwrap_err().code, "permission_denied");
    let at = dsp.clone();
    let kept = state
        .read(move |db| db.communications_texts(&at, &conversation, None, 10))
        .await
        .unwrap();
    let bodies: Vec<&str> = kept.iter().map(|text| text.body.as_str()).collect();
    assert_eq!(bodies, ["Can I swap Saturday?", "Swap it"]);
}
