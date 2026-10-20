//! What members wrote while Dispatch couldn't hear Slack, caught up on from Slack: in a direct
//! message Slack lists the app as having, with the replies in its threads, but nothing sent
//! before Communications was on, nor the app's own posts, and nothing twice.
use super::*;
use crate::backend::{
    storage::{CommunicationsStore, ConversationState},
    support,
};
use crate::integrations::messages::fixture::{self, JORDAN};

const TEAM: &str = "T0FIXTURE1";

#[tokio::test]
async fn what_dispatch_didnt_hear_is_caught_up_once_from_when_it_was_on() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    catch_up(&state, &dsp).await.unwrap();
    let kept = support::conversation_with(&state, &dsp, TEAM, "U0FIXJORDAN")
        .await
        .unwrap();
    assert_eq!(
        (kept.channel.as_str(), kept.state),
        (JORDAN, ConversationState::Waiting)
    );
    assert!(kept.caught_up_at.is_some());
    let messages = support::messages(&state, &dsp, &kept.id).await;
    let read: Vec<(&str, usize, bool)> = messages
        .iter()
        .map(|message| {
            (
                message.text.as_str(),
                message.files.len(),
                message.in_thread,
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            ("Running 10 minutes late, flat tire", 0, false),
            ("", 1, false),
            ("Can I swap Saturday?", 0, false),
            ("Never mind, Tyler took it", 0, true),
        ]
    );
    // Caught up again, nothing is kept twice.
    let at = dsp.clone();
    let before = state
        .read(move |db| db.communications_revision(&at))
        .await
        .unwrap();
    catch_up(&state, &dsp).await.unwrap();
    let at = dsp.clone();
    let after = state
        .read(move |db| db.communications_revision(&at))
        .await
        .unwrap();
    assert_eq!(
        (after, support::messages(&state, &dsp, &kept.id).await.len()),
        (before, 4)
    );
}

#[tokio::test]
async fn a_reply_in_a_thread_begun_before_where_catching_up_left_off_is_caught_up() {
    const TEAM: &str = "T0FIXTURE2";
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    catch_up(&state, &dsp).await.unwrap();
    let kept = support::conversation_with(&state, &dsp, TEAM, "U0FIXJORDAN")
        .await
        .unwrap();
    // Caught up to after the question; Jordan replies in its thread after that.
    let question = support::messages(&state, &dsp, &kept.id)
        .await
        .into_iter()
        .find(|message| message.text == "Can I swap Saturday?")
        .unwrap();
    let asked: i64 = question.ts.split('.').next().unwrap().parse().unwrap();
    let (at, id) = (dsp.clone(), kept.id.clone());
    state
        .run(move |db| db.caught_up_communications(&at, &id, (asked + 5) * 1000))
        .await
        .unwrap();
    fixture::reply_later(TEAM, "Swapped with Tyler", 10);
    catch_up(&state, &dsp).await.unwrap();
    let last = support::messages(&state, &dsp, &kept.id)
        .await
        .pop()
        .unwrap();
    assert_eq!(
        (last.text.as_str(), last.in_thread),
        ("Swapped with Tyler", true)
    );
}
