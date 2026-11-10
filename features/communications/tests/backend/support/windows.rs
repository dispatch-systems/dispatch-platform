//! What the window and origins tests start from: a long history in each transport, the origins
//! built for it, and how a window of it is asked for and checked.
use super::{install, written};
use crate::api::types::ConversationWindow;
use crate::backend::{
    changes::changed_conversation,
    model::Reference,
    origins::{Transport, backfill},
    phone::support::paired,
    storage::messages::MessageStore,
    texts::{self, model::Filed, storage::TextStore},
    windows::{Request, read_origins},
};
use dispatch_core::{State, db::Store, testing};
use rusqlite::params;
use std::sync::Arc;

/// When the fixture's rows were written, in milliseconds.
pub const AT: i64 = 1_700_000_000_000;
/// A platform with one DSP and its server's state, and a Slack and a text conversation of
/// `count` incoming rows each, numbered from 0 in their bodies, all written at `AT`.
pub fn fixture(count: usize) -> (impl Drop, Store, Arc<State>, String, Reference, Reference) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    let phone = paired(&store, &dsp, "window-phone", "+12065550100");
    let text = store
        .open_communications_text(&dsp, &Filed::Number("+12065550101".into()))
        .unwrap()
        .id;
    let slack = store
        .keep_communications_message(&dsp, &written("1700000000.000000", AT, "0"))
        .unwrap()
        .unwrap()
        .conversation;
    let db = store.dsp(&dsp).unwrap();
    db.transaction(|| {
        if count == 0 { db.exec("DELETE FROM communications_messages WHERE conversation_id=?", [&slack])?; }
        for n in 0..count {
            db.exec("INSERT OR IGNORE INTO communications_messages(id,conversation_id,ts,direction,text,at) \
                VALUES (?,?,?,'in',?,?)", params![format!("slack-{n:06}"), slack,
                    format!("1700000000.{n:06}"), n.to_string(), AT])?;
            db.exec("INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id) \
                VALUES (?1,?2,?1,'in','phone','+12065550101',?3,?4,?5,?6)",
                params![format!("text-{n:06}"), phone.id, n.to_string(), AT, n as i64, text])?;
        }
        changed_conversation(&db, &slack)?;
        changed_conversation(&db, &text)
    }).unwrap();
    drop(db);
    let state = State::new(store.config.clone()).unwrap();
    (
        root,
        store,
        state,
        dsp,
        Reference::Slack(slack),
        Reference::Text(text),
    )
}
/// Builds the origins of a conversation of `count` rows, to its end.
pub fn build_origins(db: &Store, dsp: &str, reference: &Reference, count: usize) {
    let connection = db.dsp(dsp).unwrap();
    let zone = texts::zone(db, dsp).unwrap();
    // A finite fixture traversal, not a timing wait. One extra visit observes exact-chunk EOF.
    for _ in 0..=count / 256 {
        if backfill::chunk(&connection, Transport::of(reference), reference.id(), zone).unwrap() {
            return;
        }
    }
    panic!("fixture backfill did not reach EOF");
}
/// A window's own bounds, and that its range and context agree with its messages.
pub fn bounded(page: &ConversationWindow) {
    assert!(page.messages.len() <= 150);
    assert!(page.latest.len() <= 50);
    assert_eq!(
        page.range.first.as_deref(),
        page.messages.first().map(|message| message.ts.as_str())
    );
    assert_eq!(
        page.range.last.as_deref(),
        page.messages.last().map(|message| message.ts.as_str())
    );
    assert_eq!(page.range.has_before, page.range.before.is_some());
    assert_eq!(page.range.has_after, page.range.after.is_some());
    assert_eq!(page.range.has_before, page.context.previous.is_some());
    assert_eq!(page.range.has_after, page.context.next.is_some());
    assert!(page.latest_context.next.is_none());
    if page.messages.is_empty() {
        assert!(page.context.sent_before.is_none());
        assert!(page.latest_context.previous.is_none());
        assert!(page.latest_context.sent_before.is_none());
    }
}
/// Whether the conversation's latest window answers with ready origins.
pub fn ready(db: &Store, state: &State, dsp: &str, reference: &Reference) -> bool {
    read_origins(db, state, dsp, reference, Request::Latest)
        .unwrap()
        .origins
        .unwrap()
        .ready
}
/// The fixture's row 100 as a window's anchor, with `leading` rows before it.
pub fn row_100(reference: &Reference, leading: usize) -> Request {
    let place = match reference {
        Reference::Slack(_) => "1700000000.000100",
        Reference::Text(_) => "1700000000000:text-000100",
    };
    Request::Anchor {
        place: place.into(),
        leading,
    }
}
