//! A conversation of either kind, through the dispatcher: what a message written in it may carry,
//! as reading it says, by text as a text does and in Slack as Slack does; and an answer that went,
//! kept though its writer's access ended as it went, read back to them only while they may read it.
use super::*;
use crate::backend::{
    compose::SLACK,
    phone,
    support::{self, filed, switch, texted},
    texts::storage::TextStore,
};
use dispatch_core::db::now;

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
    // Communications goes off for the DSP once the answer is queued, as the phone's outbox wakes
    // for it, and with it their permission: it went and is kept, but nothing of the conversation
    // is read back to them. The answer goes on only once it's off.
    let mut outbox = state.topics.subscribe(phone::live::TOPIC, &dsp);
    let reference = Reference::Text(conversation.clone());
    let sending = send(&state, &c, support::USE, &reference, "Swap it", &[]);
    tokio::pin!(sending);
    tokio::select! {
        biased;
        _ = outbox.changed() => {}
        _ = &mut sending => panic!("it answered before it was queued"),
    }
    switch(&state, &dsp, "communications", false).await;
    assert_eq!(sending.await.unwrap_err().code, "permission_denied");
    let at = dsp.clone();
    let kept = state
        .read(move |db| db.communications_texts(&at, &conversation, None, 10))
        .await
        .unwrap();
    let bodies: Vec<&str> = kept.iter().map(|text| text.body.as_str()).collect();
    assert_eq!(bodies, ["Can I swap Saturday?", "Swap it"]);
}
