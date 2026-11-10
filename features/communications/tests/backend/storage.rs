//! What's kept of each conversation: its messages, an edit in place of what was written, and
//! whether it waits on an answer, from its messages in the order they were written.
use crate::backend::{
    changes::ChangeStore,
    model::ConversationState,
    storage::{
        conversations::ConversationStore,
        messages::{MessageStore, Sent, Written},
        replies::ReplyStore,
    },
    support::{ReadBudget, written},
};
use dispatch_core::testing;

/// A message someone at the DSP sent at `at`.
fn sent(ts: &str, at: i64) -> Sent<'_> {
    Sent {
        ts,
        text: "On my way",
        at,
        by: "user_1",
        name: "Morgan Lane",
        photos: &[],
        gif: None,
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

#[test]
fn latest_messages_stay_bounded_as_history_grows_and_keep_the_latest_incoming() {
    use crate::backend::{
        phone::support::paired,
        texts::{model::Filed, storage::TextStore},
    };
    crate::backend::support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let phone = paired(&store, &dsp, "latest-read-key", "+12065550100");
    let text = store
        .open_communications_text(&dsp, &Filed::Number("+12065550101".into()))
        .unwrap()
        .id;
    let empty_text = store
        .open_communications_text(&dsp, &Filed::Number("+12065550102".into()))
        .unwrap()
        .id;
    let slack = store
        .keep_communications_message(&dsp, &written("0000000000000.000001", 0, "First"))
        .unwrap()
        .unwrap()
        .conversation;
    let db = store.dsp(&dsp).unwrap();
    db.exec(
        "WITH RECURSIVE history(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM history WHERE n<4999) \
         INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id) \
         SELECT 'history-'||n,?1,'history-'||n,CASE WHEN n%2=0 THEN 'in' ELSE 'out' END,\
         'phone','+12065550101','Earlier',n,n,?2 FROM history",
        rusqlite::params![phone.id, text],
    ).unwrap();
    for (id, direction, blocked) in [
        ("z-in", "in", 0),
        ("zz-blocked", "in", 1),
        ("zzz-out", "out", 0),
    ] {
        db.exec(
            "INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id,blocked) \
             VALUES (?1,?2,?1,?3,'phone','+12065550101','Latest',5000,5000,?4,?5)",
            rusqlite::params![id, phone.id, direction, text, blocked],
        ).unwrap();
    }
    db.exec(
        "WITH RECURSIVE history(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM history WHERE n<4999) \
         INSERT INTO communications_messages(id,conversation_id,ts,direction,text,at) \
         SELECT 'slack-history-'||n,?1,printf('%013d.000001',n),\
         CASE WHEN n%2=0 THEN 'in' ELSE 'out' END,'Earlier',n FROM history",
        [&slack],
    )
    .unwrap();
    for (id, ts, direction) in [
        ("slack-in", "0000000005000.000001", "in"),
        ("slack-out", "0000000005000.000002", "out"),
    ] {
        db.exec(
            "INSERT INTO communications_messages(id,conversation_id,ts,direction,text,at) VALUES (?,?,?,?,'Latest',5000)",
            [id, &slack, ts, direction],
        ).unwrap();
    }
    let texts = vec![text.clone(), text, empty_text, "missing".into()];
    let slack = vec![slack.clone(), slack, "missing".into()];
    let budget = ReadBudget::new(&db.0);
    let latest = store.latest_communications_texts(&dsp, &texts).unwrap();
    drop(budget);
    assert_eq!(
        latest
            .iter()
            .map(|text| text.id.as_str())
            .collect::<Vec<_>>(),
        ["zzz-out"]
    );
    let budget = ReadBudget::new(&db.0);
    let incoming = store.latest_communications_texts_in(&dsp, &texts).unwrap();
    drop(budget);
    assert_eq!(
        incoming
            .iter()
            .map(|text| text.id.as_str())
            .collect::<Vec<_>>(),
        ["z-in"]
    );
    let budget = ReadBudget::new(&db.0);
    let latest = store.latest_communications_messages(&dsp, &slack).unwrap();
    drop(budget);
    assert_eq!(
        latest
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        ["slack-out"]
    );
    let budget = ReadBudget::new(&db.0);
    let incoming = store.latest_communications_written(&dsp, &slack).unwrap();
    drop(budget);
    assert_eq!(
        incoming
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        ["slack-in"]
    );
    assert!(
        store
            .latest_communications_texts(&dsp, &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .latest_communications_written(&dsp, &[])
            .unwrap()
            .is_empty()
    );

    // A long suffix of rejected messages must not make the incoming-only read scan history:
    // outgoing broadcasts and blocked incoming texts can both follow an unanswered message.
    db.exec(
        "WITH RECURSIVE suffix(n) AS (VALUES(5001) UNION ALL SELECT n+1 FROM suffix WHERE n<10000) \
         INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id,blocked) \
         SELECT 'suffix-'||n,?1,'suffix-'||n,CASE WHEN n%2=0 THEN 'in' ELSE 'out' END,\
         'phone','+12065550101','Rejected',n,n,?2,1 FROM suffix",
        rusqlite::params![phone.id, texts[0]],
    ).unwrap();
    db.exec(
        "WITH RECURSIVE suffix(n) AS (VALUES(5001) UNION ALL SELECT n+1 FROM suffix WHERE n<10000) \
         INSERT INTO communications_messages(id,conversation_id,ts,direction,text,at) \
         SELECT 'slack-suffix-'||n,?1,printf('%013d.000001',n),'out','Broadcast',n FROM suffix",
        [&slack[0]],
    )
    .unwrap();
    let budget = ReadBudget::new(&db.0);
    let incoming = store.latest_communications_texts_in(&dsp, &texts).unwrap();
    drop(budget);
    assert_eq!(
        incoming
            .iter()
            .map(|text| text.id.as_str())
            .collect::<Vec<_>>(),
        ["z-in"]
    );
    let budget = ReadBudget::new(&db.0);
    let incoming = store.latest_communications_written(&dsp, &slack).unwrap();
    drop(budget);
    assert_eq!(
        incoming
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        ["slack-in"]
    );
}
