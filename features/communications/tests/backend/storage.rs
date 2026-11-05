//! What's kept of each conversation: its messages, an edit in place of what was written, and
//! whether it waits on an answer, from its messages in the order they were written.
use super::*;
use crate::backend::changes::ChangeStore;
use dispatch_core::testing;

const TEAM: &str = "T0FIXTURE1";

/// A message the member `U0JORDAN` wrote at `at`, in milliseconds.
fn written(ts: &str, at: i64, text: &str) -> Written {
    Written {
        team: TEAM.into(),
        user: "U0JORDAN".into(),
        channel: "D0JORDAN".into(),
        name: "Jordan Ellis".into(),
        ts: ts.into(),
        thread_ts: None,
        text: text.into(),
        at,
        edited_at: None,
        files: Vec::new(),
    }
}
/// A message someone at the DSP sent at `at`.
fn sent(ts: &str, at: i64) -> Sent<'_> {
    Sent {
        ts,
        text: "On my way",
        at,
        by: "user_1",
        name: "Morgan Lane",
        photos: &[],
    }
}

#[test]
fn a_conversation_waits_from_the_members_first_unanswered_message_until_answered_or_done() {
    crate::backend::support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let kept = store
        .keep_communications_message(&dsp, &written("100.000001", 100_000, "Flat tire"))
        .unwrap()
        .unwrap();
    let id = kept.conversation;
    let state = || {
        let conversation = store
            .communications_conversation(&dsp, &id)
            .unwrap()
            .unwrap();
        (
            conversation.state,
            conversation.waiting_since,
            conversation.last_at,
        )
    };
    assert_eq!(
        state(),
        (ConversationState::Waiting, Some(100_000), Some(100_000))
    );
    // Kept once, however often Slack or catching up tells of it.
    let again = written("100.000001", 100_000, "Flat tire");
    assert_eq!(
        store.keep_communications_message(&dsp, &again).unwrap(),
        None
    );
    // A second message waits since the first.
    store
        .keep_communications_message(&dsp, &written("110.000001", 110_000, "Hello?"))
        .unwrap();
    assert_eq!(
        state(),
        (ConversationState::Waiting, Some(100_000), Some(110_000))
    );
    // Answered, it's responded; a message caught up later, written before the answer, leaves it so.
    store
        .keep_communications_reply(&dsp, &id, &sent("120.000001", 120_000), "rpl_1")
        .unwrap();
    store
        .keep_communications_message(&dsp, &written("115.000001", 115_000, "Still here"))
        .unwrap();
    assert_eq!(state(), (ConversationState::Responded, None, Some(120_000)));
    // Written to again, it waits since then; marked done, it waits no more.
    store
        .keep_communications_message(&dsp, &written("130.000001", 130_000, "Thanks"))
        .unwrap();
    assert_eq!(
        state(),
        (ConversationState::Waiting, Some(130_000), Some(130_000))
    );
    // Done covers only what its member was shown: what came after still waits, though written
    // in the same millisecond, kept before the Done or after it.
    let keep = |ts, text| {
        store
            .keep_communications_message(&dsp, &written(ts, 140_000, text))
            .unwrap();
    };
    keep("140.000001", "And the vest?");
    keep("140.000500", "Found it");
    let done = |seen| store.mark_communications_done(&dsp, &id, "user_1", seen);
    let waits = (ConversationState::Waiting, Some(140_000), Some(140_000));
    assert!(done(Some((130_000, "130.000001"))).unwrap());
    assert_eq!(state(), waits);
    assert!(done(Some((140_000, "140.000001"))).unwrap());
    assert_eq!(state(), waits);
    assert!(done(Some((140_000, "140.000500"))).unwrap());
    assert_eq!(state().0, ConversationState::Done);
    keep("140.000999", "Thanks again");
    assert_eq!(state(), waits);
    assert!(done(None).unwrap());
    assert_eq!(state().0, ConversationState::Done);
    assert!(!done(None).unwrap());
    // Its messages, in the order written.
    let texts: Vec<String> = store
        .communications_messages(&dsp, &id, None, 50)
        .unwrap()
        .into_iter()
        .map(|message| message.text)
        .collect();
    assert_eq!(
        texts,
        [
            "Flat tire",
            "Hello?",
            "Still here",
            "On my way",
            "Thanks",
            "And the vest?",
            "Found it",
            "Thanks again"
        ]
    );
}

#[test]
fn an_edit_keeps_only_the_latest_text() {
    crate::backend::support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    store
        .keep_communications_message(&dsp, &written("100.000001", 100_000, "Van 12 is out"))
        .unwrap();
    let before = store.communications_revision(&dsp).unwrap();
    let edit = |text: &str, at: i64| {
        let edited = Written {
            edited_at: Some(at),
            ..written("100.000001", 100_000, text)
        };
        store.keep_communications_message(&dsp, &edited).unwrap()
    };
    assert!(edit("Van 21 is out", 105_000).is_some());
    // An earlier edit told of late changes nothing.
    assert!(edit("Van 2 is out", 101_000).is_none());
    let kept = store
        .communications_messages(
            &dsp,
            &store.communications_with_messages(&dsp).unwrap()[0].id,
            None,
            50,
        )
        .unwrap();
    assert_eq!(
        (kept[0].text.as_str(), kept[0].edited_at),
        ("Van 21 is out", Some(105_000))
    );
    // The earlier text is nowhere in the DSP's database.
    let db = store.dsp(&dsp).unwrap();
    let rows = db.all("SELECT * FROM communications_messages", []).unwrap();
    assert!(!serde_json::to_string(&rows).unwrap().contains("Van 12"));
    // A page waiting since before the edit hears of it.
    let (changed, _) = store.communications_changed_since(&dsp, before).unwrap();
    assert_eq!(changed.unwrap().len(), 1);
}
