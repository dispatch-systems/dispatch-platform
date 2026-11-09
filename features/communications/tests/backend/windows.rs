//! Window traversal and authoritative refresh through the production reader, with fixture SQL
//! for long history. Expected IDs come from the fixture's explicit order, not the selector.
use super::*;
use crate::backend::{
    changes::changed_conversation,
    phone::{storage::TextStore as PhoneTextStore, support::paired},
    storage::messages::Written,
    support,
    texts::model::{Filed, TextStatus},
};
use dispatch_core::{
    db::{Db, Kind},
    testing,
};
use rusqlite::params;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    ffi::{CStr, c_void},
    sync::Arc,
};

const AT: i64 = 1_700_000_000_000;
fn fixture(count: usize) -> (impl Drop, Store, Arc<State>, String, Reference, Reference) {
    support::install();
    let (root, store, dsp) = testing::bootstrapped();
    let phone = paired(&store, &dsp, "window-phone", "+12065550100");
    let text = store
        .open_communications_text(&dsp, &Filed::Number("+12065550101".into()))
        .unwrap()
        .id;
    let slack = store
        .keep_communications_message(
            &dsp,
            &Written {
                team: "T0FIXTURE1".into(),
                user: "U0JORDAN".into(),
                channel: "D0JORDAN".into(),
                name: "Jordan".into(),
                ts: "1700000000.000000".into(),
                thread_ts: None,
                text: "0".into(),
                at: AT,
                edited_at: None,
                files: Vec::new(),
            },
        )
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
fn build_origins(db: &Store, dsp: &str, reference: &Reference, count: usize) {
    use crate::backend::origins::{Transport, backfill};
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
fn bodies(page: &ConversationWindow) -> Vec<usize> {
    page.messages
        .iter()
        .map(|message| message.text.parse().unwrap())
        .collect()
}
fn bounded(page: &ConversationWindow) {
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

#[tokio::test]
async fn both_transports_traverse_ties_in_both_directions_without_a_missing_or_duplicate_pivot() {
    let (_root, db, state, dsp, slack, text) = fixture(701);
    for reference in [&slack, &text] {
        let mut page = read(&db, &state, &dsp, reference, Request::Latest).unwrap();
        assert_eq!(bodies(&page), (551..701).collect::<Vec<_>>());
        let mut seen = bodies(&page).into_iter().collect::<BTreeSet<_>>();
        while let Some(cursor) = page.range.before.clone() {
            let previous = page.messages[0].ts.clone();
            let next = read(&db, &state, &dsp, reference, Request::Continue(cursor)).unwrap();
            bounded(&next);
            assert!(next.messages.iter().any(|message| message.ts == previous));
            let ordered = bodies(&next);
            assert!(ordered.windows(2).all(|pair| pair[1] == pair[0] + 1));
            assert!(next.range.first < page.range.first);
            seen.extend(ordered);
            page = next;
        }
        assert_eq!(seen, (0..701).collect());
        assert_eq!(bodies(&page), (0..150).collect::<Vec<_>>());
        let mut seen = bodies(&page).into_iter().collect::<BTreeSet<_>>();
        while let Some(cursor) = page.range.after.clone() {
            let previous = page.messages.last().unwrap().ts.clone();
            let next = read(&db, &state, &dsp, reference, Request::Continue(cursor)).unwrap();
            bounded(&next);
            // At the tail a short right side tops up from left, so overlap can exceed one.
            assert!(next.messages.iter().any(|message| message.ts == previous));
            assert!(next.range.last > page.range.last);
            assert!(bodies(&next).windows(2).all(|pair| pair[1] == pair[0] + 1));
            seen.extend(bodies(&next));
            page = next;
        }
        assert_eq!(seen, (0..701).collect());
        assert_eq!(bodies(&page), (551..701).collect::<Vec<_>>());
        assert!(!page.range.has_after);
    }
}

#[tokio::test]
async fn empty_exact_and_short_sides_keep_authoritative_flags_and_the_legacy_fifty() {
    for count in [0, 1, 150, 151] {
        let (_root, db, state, dsp, slack, text) = fixture(count);
        for reference in [&slack, &text] {
            let page = read(&db, &state, &dsp, reference, Request::Latest).unwrap();
            bounded(&page);
            assert_eq!(
                bodies(&page),
                (count.saturating_sub(150)..count).collect::<Vec<_>>()
            );
            assert_eq!(page.range.has_before, count > 150);
            assert!(!page.range.has_after);
            assert!(page.origins.is_none());
            let pending = read_origins(&db, &state, &dsp, reference, Request::Latest).unwrap();
            assert!(!pending.origins.unwrap().ready);
            build_origins(&db, &dsp, reference, count);
            let ready = read_origins(&db, &state, &dsp, reference, Request::Latest).unwrap();
            let origins = ready.origins.unwrap();
            assert!(origins.ready);
            assert_eq!(origins.selected.is_none(), count == 0);
            assert_eq!(origins.latest.is_none(), count == 0);
            let legacy = dispatcher::opened(&db, &state, &dsp, reference, None).unwrap();
            assert_eq!(legacy.messages.len(), count.min(50));
            assert_eq!(legacy.more, count > 50);
            assert_eq!(
                page.latest
                    .iter()
                    .map(|m| m.text.parse::<usize>().unwrap())
                    .collect::<Vec<_>>(),
                (count.saturating_sub(50)..count).collect::<Vec<_>>()
            );
            let anchors = match reference {
                Reference::Slack(_) => [("1699999999.999999", 149), ("1700000001.000000", 0)],
                Reference::Text(_) => [("1699999999999:z", 149), ("1700000000001:a", 0)],
            };
            for (end, (place, leading)) in anchors.into_iter().enumerate() {
                let around = read(
                    &db,
                    &state,
                    &dsp,
                    reference,
                    Request::Anchor {
                        place: place.into(),
                        leading,
                    },
                )
                .unwrap();
                bounded(&around);
                assert_eq!(around.messages.len(), count.min(150));
                let start = if end == 0 {
                    0
                } else {
                    count.saturating_sub(150)
                };
                assert_eq!(
                    bodies(&around),
                    (start..(start + count.min(150))).collect::<Vec<_>>()
                );
                assert_eq!(around.range.has_before, end == 1 && count > 150);
                assert_eq!(around.range.has_after, end == 0 && count > 150);
            }
        }
    }
}

#[tokio::test]
async fn bursts_and_old_retries_edits_files_and_refiling_rebase_complete_windows() {
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let historical = |reference: &Reference| {
        let place = match reference {
            Reference::Slack(_) => "1700000000.000100",
            Reference::Text(_) => "1700000000000:text-000100",
        };
        read(
            &db,
            &state,
            &dsp,
            reference,
            Request::Anchor {
                place: place.into(),
                leading: 50,
            },
        )
        .unwrap()
    };
    let tail = read(&db, &state, &dsp, &slack, Request::Latest).unwrap();
    let old_slack = historical(&slack);
    let old_text = historical(&text);
    let old_before = old_text.range.before.clone().unwrap();
    let old_after = old_text.range.after.clone().unwrap();
    assert_eq!(bodies(&old_text), (50..200).collect::<Vec<_>>());
    let connection = db.dsp(&dsp).unwrap();
    connection
        .exec(
            "UPDATE phones_texts SET origin='dispatch',direction='out',status='failed',\
        status_at=?1,attempt=1,queued_in=?2 WHERE id='text-000100'",
            params![AT, text.id()],
        )
        .unwrap();
    connection
        .exec(
            "INSERT INTO communications_files(id,message_id,name,mime,size,status,added_at) \
        VALUES ('window-file','slack-000101','Door.png','image/png',1,'waiting',0)",
            [],
        )
        .unwrap();
    // A burst larger than both old latest50 and the entire historical window.
    connection.transaction(|| {
        for n in 701..901 {
            connection.exec("INSERT INTO communications_messages(id,conversation_id,ts,direction,text,at) \
                VALUES (?,?,?,'in',?,?)", params![format!("slack-{n:06}"), slack.id(),
                format!("1700000000.{n:06}"), n.to_string(), AT])?;
        }
        connection.exec("UPDATE communications_messages SET text='Edited',edited_at=? WHERE id='slack-000100'", [AT+1])?;
        connection.exec("UPDATE communications_files SET status='failed' WHERE id='window-file'", [])?;
        changed_conversation(&connection, slack.id())
    }).unwrap();
    let refreshed = read(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Continue(tail.range.refresh),
    )
    .unwrap();
    assert!(refreshed.range.rebased);
    assert_eq!(bodies(&refreshed), (751..901).collect::<Vec<_>>());
    assert!(refreshed.range.has_before);
    // Walk across the whole >50 gap to the old tail using server continuations.
    let mut gap = BTreeSet::new();
    let mut across = read(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Continue(refreshed.range.refresh.clone()),
    )
    .unwrap();
    loop {
        gap.extend(bodies(&across));
        if gap.contains(&700) {
            break;
        }
        across = read(
            &db,
            &state,
            &dsp,
            &slack,
            Request::Continue(across.range.before.expect("gap remains reachable")),
        )
        .unwrap();
        bounded(&across);
    }
    assert!((700..901).all(|n| gap.contains(&n)));
    let refreshed = read(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Continue(old_slack.range.refresh),
    )
    .unwrap();
    assert!(refreshed.range.rebased && refreshed.range.has_after);
    assert_eq!(
        refreshed
            .messages
            .iter()
            .find(|m| m.id == "slack-000100")
            .unwrap()
            .text,
        "Edited"
    );
    assert_eq!(
        refreshed
            .messages
            .iter()
            .find(|m| m.id == "slack-000101")
            .unwrap()
            .files[0]
            .status,
        crate::backend::model::FileStatus::Failed
    );
    assert!(!refreshed.latest.iter().any(|m| m.id == "slack-000100"));
    assert_eq!(
        refreshed
            .latest
            .iter()
            .map(|m| m.text.parse::<usize>().unwrap())
            .collect::<Vec<_>>(),
        (851..901).collect::<Vec<_>>()
    );
    // Real retry changes status/attempt/device, not ordering position. Settle through the same
    // Communications product entry point used after phone-side changes.
    db.phones_retry(&dsp, "text-000100", AT + 2).unwrap();
    db.settle_communications_texts(&dsp, &["text-000100".into()])
        .unwrap();
    let waiting = read(
        &db,
        &state,
        &dsp,
        &text,
        Request::Continue(old_text.range.refresh),
    )
    .unwrap();
    let retry = waiting
        .messages
        .iter()
        .find(|m| m.id == "text-000100")
        .unwrap();
    assert_eq!(
        (retry.ts.as_str(), retry.status),
        ("1700000000000:text-000100", Some(TextStatus::Waiting))
    );
    assert!(waiting.range.rebased);
    assert!(!waiting.latest.iter().any(|m| m.id == retry.id));
    connection
        .exec(
            "UPDATE phones_texts SET status='delivered',status_at=? WHERE id='text-000100'",
            [AT + 3],
        )
        .unwrap();
    db.settle_communications_texts(&dsp, &["text-000100".into()])
        .unwrap();
    let delivered = read(
        &db,
        &state,
        &dsp,
        &text,
        Request::Continue(waiting.range.refresh),
    )
    .unwrap();
    assert_eq!(
        delivered
            .messages
            .iter()
            .find(|m| m.id == "text-000100")
            .unwrap()
            .status,
        Some(TextStatus::Delivered)
    );
    // Refile the actual anchor and delete one neighbor. The old position still works, and no
    // retained message is resurrected. Backdated insertion is selected on this refresh too.
    let destination = db
        .open_communications_text(&dsp, &Filed::Number("+12065550102".into()))
        .unwrap();
    db.refile_communications_texts(
        &dsp,
        &[("text-000100".into(), Filed::Number("+12065550102".into()))],
    )
    .unwrap();
    connection.transaction(|| {
        connection.exec("DELETE FROM phones_texts WHERE id='text-000101'", [])?;
        connection.exec("INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id) \
            SELECT 'text-000100-z',device_id,'text-000100-z','in','phone',number,'Inserted',at,99999,conversation_id \
            FROM phones_texts WHERE id='text-000099'", [])?;
        changed_conversation(&connection, text.id())
    }).unwrap();
    let moved = read(
        &db,
        &state,
        &dsp,
        &text,
        Request::Continue(delivered.range.refresh),
    )
    .unwrap();
    bounded(&moved);
    assert!(moved.range.rebased && moved.range.has_before && moved.range.has_after);
    assert!(
        !moved
            .messages
            .iter()
            .any(|m| m.id == "text-000100" || m.id == "text-000101")
    );
    assert!(moved.messages.iter().any(|m| m.id == "text-000100-z"));
    for cursor in [old_before, old_after] {
        let navigated = read(&db, &state, &dsp, &text, Request::Continue(cursor)).unwrap();
        bounded(&navigated);
        assert!(navigated.range.rebased);
        assert!(
            !navigated
                .messages
                .iter()
                .any(|m| m.id == "text-000100" || m.id == "text-000101")
        );
    }
    let there = read(
        &db,
        &state,
        &dsp,
        &Reference::Text(destination.id),
        Request::Latest,
    )
    .unwrap();
    assert_eq!(there.messages[0].id, "text-000100");
    // An unaffected conversation can advance the DSP revision; that must rebase without refusing.
    changed_conversation(&connection, slack.id()).unwrap();
    let again = read(
        &db,
        &state,
        &dsp,
        &text,
        Request::Continue(moved.range.refresh),
    )
    .unwrap();
    assert!(again.range.rebased);
}

#[tokio::test]
async fn tokens_reject_modification_wrong_channel_and_future_revision_but_rebase_after_log_pruning()
{
    let (_root, db, state, dsp, slack, text) = fixture(151);
    let page = read(&db, &state, &dsp, &slack, Request::Latest).unwrap();
    let token = page.range.refresh.clone();
    let invalid = |reference: &Reference, token: String| {
        assert_eq!(
            read(&db, &state, &dsp, reference, Request::Continue(token))
                .unwrap_err()
                .code,
            "invalid_input"
        );
    };
    invalid(&text, token.clone());
    invalid(&slack, format!("{token}x"));
    let mut changed = token.as_bytes().to_vec();
    let middle = changed.len() / 2;
    changed[middle] = if changed[middle] == b'A' { b'B' } else { b'A' };
    invalid(&slack, String::from_utf8(changed).unwrap());
    let other = db
        .keep_communications_message(
            &dsp,
            &Written {
                team: "T0FIXTURE1".into(),
                user: "U0ELSE".into(),
                channel: "D0ELSE".into(),
                name: "Else".into(),
                ts: "1700000000.000001".into(),
                thread_ts: None,
                text: "Else".into(),
                at: AT,
                edited_at: None,
                files: Vec::new(),
            },
        )
        .unwrap()
        .unwrap()
        .conversation;
    invalid(&Reference::Slack(other), token.clone());
    let binding =
        serde_json::to_string(&json!(["communications-window-v1", dsp, slack.id()])).unwrap();
    let future = crypto::encrypt(
        &db.key,
        &binding,
        &json!({
            "anchor": null, "leading": 0, "revision": 9_000_000
        }),
    )
    .unwrap();
    assert_eq!(
        read(&db, &state, &dsp, &slack, Request::Continue(future))
            .unwrap_err()
            .code,
        "communications_window_reset"
    );
    // No message journal replay or old revision record is required.
    let connection = db.dsp(&dsp).unwrap();
    connection
        .transaction(|| {
            for _ in 0..1001 {
                changed_conversation(&connection, text.id())?;
            }
            Ok(())
        })
        .unwrap();
    let refreshed = read(&db, &state, &dsp, &slack, Request::Continue(token)).unwrap();
    assert!(refreshed.range.rebased);
    assert_eq!(bodies(&refreshed), bodies(&page));
    for (reference, place, leading) in [
        (&slack, "1700000000000:text-000001", 1),
        (&slack, "1700000000.1", 1),
        (&slack, "9223372036854775807.000000", 1),
        (&text, "1700000000.000001", 1),
        (&text, "1700000000000:bad!", 1),
        (&text, "17000000000000:id", 1),
        (&text, "1700000000000:text-000001", 150),
    ] {
        assert_eq!(
            read(
                &db,
                &state,
                &dsp,
                reference,
                Request::Anchor {
                    place: place.into(),
                    leading
                }
            )
            .unwrap_err()
            .code,
            "invalid_input"
        );
    }
}

/// Tracing is confined to this test connection. The second connection commits exactly after
/// revision is read and before the first message query; no product synchronization hook.
struct SnapshotWrite {
    writer: Db,
    conversation: String,
    fired: Cell<bool>,
    error: RefCell<Option<String>>,
}
struct Trace<'a>(&'a rusqlite::Connection);
impl Drop for Trace<'_> {
    fn drop(&mut self) {
        // SAFETY: deregister before the callback context or connection can be dropped.
        unsafe {
            rusqlite::ffi::sqlite3_trace_v2(self.0.handle(), 0, None, std::ptr::null_mut());
        }
    }
}
#[tokio::test]
async fn revision_and_message_data_share_a_snapshot_even_when_another_connection_commits() {
    let (_root, db, state, dsp, slack, _text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    connection.exec("UPDATE communications_messages SET direction='out',sent_by='user_1', sender_name='Before \
        snapshot' WHERE conversation_id=? AND ts IN \
        ('1700000000.000099','1700000000.000250','1700000000.000650')", [slack.id()]).unwrap();
    build_origins(&db, &dsp, &slack, 701);
    let writer = Db::open(
        &db.area(&dsp, "data").unwrap().join("dispatch.sqlite"),
        Kind::DSP,
    )
    .unwrap();
    let revision = db.communications_revision(&dsp).unwrap();
    let mut write = Box::new(SnapshotWrite {
        writer,
        conversation: slack.id().into(),
        fired: Cell::new(false),
        error: RefCell::new(None),
    });
    unsafe extern "C" fn trace(
        _: u32,
        context: *mut c_void,
        statement: *mut c_void,
        _: *mut c_void,
    ) -> i32 {
        // SAFETY: Box stays alive and the callback is removed by Trace before it is dropped.
        let write = unsafe { &*context.cast::<SnapshotWrite>() };
        let sql = unsafe { CStr::from_ptr(rusqlite::ffi::sqlite3_sql(statement.cast())) }
            .to_string_lossy();
        if sql.starts_with("SELECT * FROM communications_messages WHERE conversation_id=")
            && !write.fired.replace(true)
        {
            let result = write.writer.transaction(|| {
                write.writer.exec("UPDATE communications_messages SET text='Changed during read' WHERE ts='1700000000.000100'", [])?;
                write.writer.exec("UPDATE communications_messages SET sender_name='After snapshot' WHERE conversation_id=? \
                    AND ts IN ('1700000000.000099','1700000000.000250','1700000000.000650')", [&write.conversation])?;
                write.writer.exec("DELETE FROM communications_messages WHERE conversation_id=? AND ts='1700000000.000000'", [&write.conversation])?;
                changed_conversation(&write.writer, &write.conversation)
            });
            if let Err(error) = result {
                *write.error.borrow_mut() = Some(error.to_string());
            }
        }
        0
    }
    // SAFETY: registered on the borrowed reader only, with a stable Box context; callback never
    // panics and the guard unregisters it on failure too. The test databases use the real WAL mode.
    unsafe {
        rusqlite::ffi::sqlite3_trace_v2(
            connection.0.handle(),
            rusqlite::ffi::SQLITE_TRACE_STMT,
            Some(trace),
            (&mut *write as *mut SnapshotWrite).cast(),
        );
    }
    let trace = Trace(&connection.0);
    let page = read_origins(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Anchor {
            place: "1700000000.000100".into(),
            leading: 0,
        },
    )
    .unwrap();
    drop(trace);
    assert!(write.fired.get());
    assert!(write.error.borrow().is_none(), "{:?}", write.error.borrow());
    assert_eq!(page.range.revision, revision.to_string());
    let origins = page.origins.as_ref().unwrap();
    assert!(origins.ready);
    assert_eq!(
        origins.selected.as_ref().unwrap().first_day.ts,
        "1700000000.000000"
    );
    assert_eq!(
        origins
            .selected
            .as_ref()
            .unwrap()
            .sent_before_origin
            .as_ref()
            .unwrap()
            .name,
        "Before snapshot"
    );
    assert_eq!(
        page.messages
            .iter()
            .find(|m| m.ts == "1700000000.000100")
            .unwrap()
            .text,
        "100"
    );
    for neighbor in [
        page.context.previous.as_ref(),
        page.context.next.as_ref(),
        page.latest_context.previous.as_ref(),
    ] {
        assert_eq!(
            neighbor.unwrap().sender.as_ref().unwrap().name,
            "Before snapshot"
        );
    }
    assert_eq!(
        page.context.sent_before.as_ref().unwrap().name,
        "Before snapshot"
    );
    assert_eq!(
        page.latest_context.sent_before.as_ref().unwrap().name,
        "Before snapshot"
    );
    let current = read_origins(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Continue(page.range.refresh),
    )
    .unwrap();
    assert!(current.range.rebased);
    assert!(!current.origins.as_ref().unwrap().ready);
    build_origins(&db, &dsp, &slack, 700);
    let rebuilt = read_origins(&db, &state, &dsp, &slack, Request::Latest)
        .unwrap()
        .origins
        .unwrap();
    assert!(rebuilt.ready);
    assert_eq!(
        rebuilt.selected.as_ref().unwrap().first_day.ts,
        "1700000000.000001"
    );
    assert_ne!(current.range.revision, revision.to_string());
    assert_eq!(
        current
            .messages
            .iter()
            .find(|m| m.ts == "1700000000.000100")
            .unwrap()
            .text,
        "Changed during read"
    );
    for neighbor in [
        current.context.previous.as_ref(),
        current.context.next.as_ref(),
        current.latest_context.previous.as_ref(),
    ] {
        assert_eq!(
            neighbor.unwrap().sender.as_ref().unwrap().name,
            "After snapshot"
        );
    }
    assert_eq!(
        current.context.sent_before.as_ref().unwrap().name,
        "After snapshot"
    );
    assert_eq!(
        current.latest_context.sent_before.as_ref().unwrap().name,
        "After snapshot"
    );
}

#[tokio::test]
async fn compact_boundaries_keep_writer_group_phone_name_and_failed_run_context_for_both_selections()
 {
    use crate::backend::model::Direction;
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    connection
        .exec(
            "INSERT INTO communications_groups(id,name,created_by,created_at,last_at) VALUES \
        ('window-group','Drivers','user_1',0,0)",
            [],
        )
        .unwrap();
    // Stored incoming group attribution is not a visible Message.group / writer component.
    connection.exec("UPDATE communications_messages SET group_id='window-group' WHERE conversation_id=? AND \
        ts='1700000000.000049'", [slack.id()]).unwrap();
    for n in [10, 50, 199, 200, 600, 650, 651] {
        connection
            .exec(
                "UPDATE communications_messages SET direction='out',sent_by='user_1', \
            sender_name='Morgan',group_id=CASE WHEN ?1<300 THEN 'window-group' END WHERE \
            conversation_id=?2 AND ts=?3",
                params![n, slack.id(), format!("1700000000.{n:06}")],
            )
            .unwrap();
    }
    for n in [10, 50, 199, 200, 600, 651] {
        connection
            .exec(
                "UPDATE phones_texts SET direction='out',sent_by='user_1',sender_name='Morgan', \
            origin='dispatch',status='delivered',queued_in=?1 WHERE id=?2",
                params![text.id(), format!("text-{n:06}")],
            )
            .unwrap();
    }
    connection
        .exec(
            "UPDATE phones_texts SET status='failed',error='No service' WHERE id='text-000199'",
            [],
        )
        .unwrap();
    let selected = |reference: &Reference| {
        let place = match reference {
            Reference::Slack(_) => "1700000000.000100",
            Reference::Text(_) => "1700000000000:text-000100",
        };
        read(
            &db,
            &state,
            &dsp,
            reference,
            Request::Anchor {
                place: place.into(),
                leading: 50,
            },
        )
        .unwrap()
    };
    let slack_page = selected(&slack);
    let text_page = selected(&text);
    for page in [&slack_page, &text_page] {
        bounded(page);
        assert_eq!(bodies(page), (50..200).collect::<Vec<_>>());
        assert_eq!(
            page.latest
                .iter()
                .map(|m| m.text.parse::<usize>().unwrap())
                .collect::<Vec<_>>(),
            (651..701).collect::<Vec<_>>()
        );
        assert_eq!(
            page.context.previous.as_ref().unwrap().direction,
            Direction::In
        );
        assert_eq!(page.context.previous.as_ref().unwrap().at, AT);
        assert_eq!(
            page.context.next.as_ref().unwrap().direction,
            Direction::Out
        );
        assert_eq!(page.context.sent_before.as_ref().unwrap().name, "Morgan");
        assert_eq!(
            page.context
                .sent_before
                .as_ref()
                .unwrap()
                .user_id
                .as_deref(),
            Some("user_1")
        );
        assert_eq!(
            page.latest_context.sent_before.as_ref().unwrap().name,
            "Morgan"
        );
        assert!(page.latest_context.next.is_none());
        // Privacy contract of boundary serialization, not a second generated-type inventory.
        let wire = serde_json::to_value(&page.context).unwrap();
        for neighbor in ["previous", "next"] {
            for forbidden in ["text", "body", "files", "gif", "status", "error"] {
                assert!(wire[neighbor].get(forbidden).is_none());
            }
        }
    }
    assert_eq!(
        slack_page.context.previous.as_ref().unwrap().ts,
        "1700000000.000049"
    );
    assert!(
        slack_page
            .context
            .previous
            .as_ref()
            .unwrap()
            .group_id
            .is_none()
    );
    assert_eq!(
        slack_page
            .context
            .next
            .as_ref()
            .unwrap()
            .group_id
            .as_deref(),
        Some("window-group")
    );
    assert_eq!(
        slack_page.latest_context.previous.as_ref().unwrap().ts,
        "1700000000.000650"
    );
    assert!(
        slack_page
            .latest_context
            .previous
            .as_ref()
            .unwrap()
            .group_id
            .is_none()
    );
    assert_eq!(
        text_page.context.previous.as_ref().unwrap().ts,
        "1700000000000:text-000049"
    );
    assert_eq!(
        text_page.context.next.as_ref().unwrap().number.as_deref(),
        Some("+12065550101")
    );
    assert_eq!(
        text_page.messages.last().unwrap().status,
        Some(TextStatus::Failed)
    );
    assert_eq!(
        text_page.messages.last().unwrap().error.as_deref(),
        Some("No service")
    );
    assert_eq!(
        text_page.latest_context.previous.as_ref().unwrap().id,
        "text-000650"
    );
    // Both boundaries can continue the same writer even when a failed row must still show Meta.
    // Name affects label suppression, group and number affect run boundaries, and ten minutes
    // affects pause/run decisions. These facts must be current, not inferred from missing rows.
    connection
        .exec(
            "UPDATE communications_messages SET sender_name='Renamed' WHERE ts='1700000000.000010' \
        AND conversation_id=?",
            [slack.id()],
        )
        .unwrap();
    connection
        .exec(
            "UPDATE communications_messages SET group_id=NULL WHERE ts='1700000000.000200' AND \
        conversation_id=?",
            [slack.id()],
        )
        .unwrap();
    connection
        .exec(
            "UPDATE phones_texts SET number='+12065550102' WHERE id='text-000200'",
            [],
        )
        .unwrap();
    connection
        .exec(
            "UPDATE phones_texts SET at=? WHERE id='text-000200'",
            [AT + 600_000],
        )
        .unwrap();
    // Move the window's next row later in tuple order. The next compact fact must now be201.
    let changed = selected(&text);
    assert_eq!(changed.context.next.as_ref().unwrap().id, "text-000201");
    let moved = read(
        &db,
        &state,
        &dsp,
        &text,
        Request::Anchor {
            place: "1700000000000:text-000699".into(),
            leading: 148,
        },
    )
    .unwrap();
    assert_eq!(moved.context.next.as_ref().unwrap().id, "text-000200");
    assert_eq!(
        moved.context.next.as_ref().unwrap().number.as_deref(),
        Some("+12065550102")
    );
    assert_eq!(moved.context.next.as_ref().unwrap().at, AT + 600_000);
    let changed = selected(&slack);
    assert_eq!(
        changed.context.sent_before.as_ref().unwrap().name,
        "Renamed"
    );
    assert!(changed.context.next.as_ref().unwrap().group_id.is_none());

    connection
        .exec("UPDATE phones_texts SET at=? WHERE id='text-000200'", [AT])
        .unwrap();
    // Phone-origin output uses the current phone label and phone ID, with no member ID.
    connection
        .exec(
            "UPDATE phones_texts SET sent_by=NULL,origin='phone' WHERE id IN \
        ('text-000600','text-000650','text-000651')",
            [],
        )
        .unwrap();
    connection
        .exec(
            "UPDATE phones_texts SET direction='out' WHERE id='text-000650'",
            [],
        )
        .unwrap();
    connection
        .exec(
            "UPDATE phones_devices SET name='Station phone' WHERE id IN (SELECT device_id FROM \
        phones_texts WHERE id='text-000650')",
            [],
        )
        .unwrap();
    let changed = selected(&text);
    let previous = changed.latest_context.previous.as_ref().unwrap();
    let sender = previous.sender.as_ref().unwrap();
    assert_eq!(sender.name, "Station phone");
    assert!(sender.user_id.is_none());
    assert_eq!(
        sender.phone_id,
        changed.latest[0].sender.as_ref().unwrap().phone_id
    );
    assert_eq!(changed.latest_context.sent_before.as_ref().unwrap(), sender);
}

#[tokio::test]
async fn selected_and_latest_sender_context_reset_at_dsp_midnight_and_survive_both_dst_changes() {
    // Explicit UTC instants / expected local-day relation, not an expected value computed by
    // the context code. The 23/25-hour cases reject any fixed 24-hour day calculation.
    for (previous, first, same_day) in [
        ("2024-01-15T07:59:00Z", "2024-01-15T08:00:00Z", false),
        ("2024-03-10T09:59:00Z", "2024-03-10T10:00:00Z", true),
        ("2024-11-03T08:59:00Z", "2024-11-03T09:00:00Z", true),
        ("2024-03-10T08:30:00Z", "2024-03-11T07:00:00Z", false),
        ("2024-11-03T07:30:00Z", "2024-11-04T08:00:00Z", false),
    ] {
        let previous = chrono::DateTime::parse_from_rfc3339(previous)
            .unwrap()
            .timestamp_millis();
        let first = chrono::DateTime::parse_from_rfc3339(first)
            .unwrap()
            .timestamp_millis();
        for cutoff in [100, 651] {
            let (_root, db, state, dsp, _slack, text) = fixture(701);
            let name = db.find_dsp(&dsp).unwrap().name;
            db.keep_dsp_profile(&dsp, &name, "America/Los_Angeles", None, None)
                .unwrap();
            let connection = db.dsp(&dsp).unwrap();
            connection.exec("UPDATE phones_texts SET at=CASE WHEN id<?1 THEN ?2 ELSE ?3 END WHERE conversation_id=?4",
                params![format!("text-{cutoff:06}"), previous, first, text.id()]).unwrap();
            connection
                .exec(
                    "UPDATE phones_texts SET direction='out',origin='dispatch',sent_by='user_1', \
                sender_name='Morgan',queued_in=?1 WHERE id IN ('text-000001','text-000600')",
                    [text.id()],
                )
                .unwrap();
            let at = if cutoff == 100 { first } else { previous };
            let page = read(
                &db,
                &state,
                &dsp,
                &text,
                Request::Anchor {
                    place: format!("{at:013}:text-000100"),
                    leading: 0,
                },
            )
            .unwrap();
            bounded(&page);
            assert_eq!(page.messages[0].id, "text-000100");
            assert_eq!(page.latest[0].id, "text-000651");
            assert_eq!(
                page.context.sent_before.is_some(),
                cutoff != 100 || same_day
            );
            assert_eq!(
                page.latest_context.sent_before.is_some(),
                cutoff != 651 || same_day
            );
            build_origins(&db, &dsp, &text, 701);
            let exact = read_origins(
                &db,
                &state,
                &dsp,
                &text,
                Request::Anchor {
                    place: format!("{at:013}:text-000100"),
                    leading: 0,
                },
            )
            .unwrap()
            .origins
            .unwrap();
            assert!(exact.ready);
            assert_eq!(exact.time_zone, "America/Los_Angeles");
            assert_eq!(
                exact.selected.as_ref().unwrap().first_day.ts,
                if cutoff == 100 && !same_day {
                    format!("{first:013}:text-000100")
                } else {
                    format!("{previous:013}:text-000000")
                }
            );
            assert_eq!(
                exact.latest.as_ref().unwrap().first_day.ts,
                if !same_day {
                    format!("{first:013}:text-{cutoff:06}")
                } else {
                    format!("{previous:013}:text-000000")
                }
            );
            // A timezone edit versions readiness independently from message revision.
            db.keep_dsp_profile(&dsp, &name, "UTC", None, None).unwrap();
            assert!(
                !read_origins(&db, &state, &dsp, &text, Request::Latest)
                    .unwrap()
                    .origins
                    .unwrap()
                    .ready
            );
            assert_eq!(
                read(&db, &state, &dsp, &text, Request::Latest)
                    .unwrap()
                    .messages
                    .len(),
                150
            );
            build_origins(&db, &dsp, &text, 701);
            assert_eq!(
                read_origins(&db, &state, &dsp, &text, Request::Latest)
                    .unwrap()
                    .origins
                    .unwrap()
                    .time_zone,
                "UTC"
            );
            // Neighbors keep their true times across midnight; only sender/day comparisons reset.
            assert_eq!(page.context.previous.as_ref().unwrap().at, previous);
            assert_eq!(
                page.latest_context.previous.as_ref().unwrap().at,
                if cutoff == 651 { previous } else { first }
            );
        }
    }
}

#[tokio::test]
async fn clipped_origins_use_stored_run_first_names_and_prior_run_first_for_both_independent_slices()
 {
    use crate::backend::{
        model::Direction,
        origins::{Transport, backfill},
    };
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    let member =
        dispatch_core::testing::owner_of(&db, &dsp, "origin@example.test", "Current").unwrap();
    dispatch_core::testing::set_user_name(&db, Some(&dsp), &member, "Current", "Member").unwrap();
    let mut person = dispatch_core::manifest::people::Person {
        code: "origin-person".into(),
        name: "Directory name".into(),
        names: vec!["Directory name".into()],
        role: dispatch_core::manifest::people::PersonRole::Driver,
        title: None,
        ids: Default::default(),
        emails: vec!["origin@example.test".into()],
        phones: vec!["+12065550101".into()],
        added_phones: Vec::new(),
        confirmed: true,
        photo: Some("origin-photo".into()),
    };
    support::know(&state, &dsp, vec![person.clone()], Vec::new(), Vec::new());
    connection
        .exec(
            "UPDATE communications_messages SET direction=CASE WHEN ts='1700000000.000299' \
        THEN 'in' ELSE 'out' END,sent_by=CASE WHEN ts='1700000000.000299' THEN NULL ELSE ?1 END, \
        sender_name='Later name' WHERE conversation_id=?2",
            params![member, slack.id()],
        )
        .unwrap();
    connection.exec("UPDATE phones_texts SET direction=CASE WHEN id='text-000299' THEN 'in' ELSE 'out' END, \
        sent_by=CASE WHEN id='text-000299' THEN NULL ELSE ?1 END,sender_name='Later name' \
        WHERE conversation_id=?2",params![member,text.id()]).unwrap();
    for reference in [&slack, &text] {
        let transport = Transport::of(reference);
        let zone = texts::zone(&db, &dsp).unwrap();
        // Crash/restart-equivalent: a committed chunk retains its exact native cursor; source
        // topology mutation between chunks forces a fresh candidate, never partial readiness.
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let old_generation: (String,) = connection
            .one_as(
                "SELECT generation FROM communications_origin_builds \
            WHERE transport=? AND conversation_id=?",
                params![transport.name(), reference.id()],
            )
            .unwrap()
            .unwrap();
        match reference {
            Reference::Slack(id) => connection
                .exec(
                    "UPDATE communications_messages SET at=at+1 WHERE conversation_id=? \
                AND ts='1700000000.000500'",
                    [id],
                )
                .unwrap(),
            Reference::Text(id) => connection
                .exec(
                    "UPDATE phones_texts SET number='+12065550999' WHERE conversation_id=? \
                AND id='text-000500'",
                    [id],
                )
                .unwrap(),
        };
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let restarted: (String,) = connection
            .one_as(
                "SELECT generation FROM communications_origin_builds \
            WHERE transport=? AND conversation_id=?",
                params![transport.name(), reference.id()],
            )
            .unwrap()
            .unwrap();
        assert_ne!(old_generation, restarted);
        match reference {
            Reference::Slack(id) => connection
                .exec(
                    "UPDATE communications_messages SET at=?1 WHERE conversation_id=?2 \
                AND ts='1700000000.000500'",
                    params![AT, id],
                )
                .unwrap(),
            Reference::Text(id) => connection
                .exec(
                    "UPDATE phones_texts SET number='+12065550101' WHERE conversation_id=? \
                AND id='text-000500'",
                    [id],
                )
                .unwrap(),
        };
        build_origins(&db, &dsp, reference, 701);
        let mut prior_bodies = None;
        for (first_name, latest_name) in [("Original A", "Second A"), ("Original B", "Second B")] {
            match reference {
                Reference::Slack(id) => connection
                    .exec(
                        "UPDATE communications_messages SET sender_name=CASE \
                    WHEN ts='1700000000.000000' THEN ?1 ELSE ?2 END WHERE conversation_id=?3 AND \
                    ts IN ('1700000000.000000','1700000000.000300')",
                        params![first_name, latest_name, id],
                    )
                    .unwrap(),
                Reference::Text(id) => connection
                    .exec(
                        "UPDATE phones_texts SET sender_name=CASE WHEN \
                    id='text-000000' THEN ?1 ELSE ?2 END WHERE conversation_id=?3 AND \
                    id IN ('text-000000','text-000300')",
                        params![first_name, latest_name, id],
                    )
                    .unwrap(),
            };
            let anchor = match reference {
                Reference::Slack(_) => "1700000000.000100",
                Reference::Text(_) => "1700000000000:text-000100",
            };
            let page = read_origins(
                &db,
                &state,
                &dsp,
                reference,
                Request::Anchor {
                    place: anchor.into(),
                    leading: 0,
                },
            )
            .unwrap();
            bounded(&page);
            let bodies = serde_json::to_value((&page.messages, &page.latest)).unwrap();
            if let Some(previous) = &prior_bodies {
                assert_eq!(&bodies, previous);
            }
            prior_bodies = Some(bodies);
            let exact = page.origins.unwrap();
            assert!(exact.ready);
            let selected = exact.selected.unwrap();
            let latest = exact.latest.unwrap();
            assert_eq!(selected.origin.sender.as_ref().unwrap().name, first_name);
            assert_eq!(latest.origin.sender.as_ref().unwrap().name, latest_name);
            assert_eq!(latest.sent_before_origin.as_ref().unwrap().name, first_name);
            assert!(selected.sent_before_origin.is_none());
            assert!(selected.before_origin.is_none());
            assert_eq!(selected.origin.at, AT);
            assert_eq!(latest.origin.at, AT);
            assert_eq!(
                latest.before_origin.as_ref().unwrap().direction,
                Direction::In
            );
            assert_eq!(
                page.context.sent_before.as_ref().unwrap().name,
                "Later name"
            );
            for origin in [&selected.origin, &latest.origin] {
                let sender = origin.sender.as_ref().unwrap();
                assert_eq!(sender.user_id.as_deref(), Some(member.as_str()));
                assert!(
                    sender.phone_id.is_none(),
                    "a member's transport device is not its writer phone"
                );
                assert_eq!(sender.code.as_deref(), Some("origin-person"));
                assert_eq!(sender.photo.as_deref(), Some("origin-photo"));
                let wire = serde_json::to_value(origin).unwrap();
                for field in [
                    "text", "body", "files", "gif", "status", "error", "moved", "inThread",
                ] {
                    assert!(wire.get(field).is_none());
                }
            }
        }
    }
    person.name = "Current directory name".into();
    person.photo = Some("changed-photo".into());
    support::know(&state, &dsp, vec![person], Vec::new(), Vec::new());
    let current = read_origins(&db, &state, &dsp, &text, Request::Latest).unwrap();
    assert_eq!(current.conversation.with.name, "Current directory name");
    assert_eq!(
        current.conversation.with.photo.as_deref(),
        Some("changed-photo")
    );
    let sender = current
        .origins
        .unwrap()
        .latest
        .unwrap()
        .origin
        .sender
        .unwrap();
    assert_eq!(sender.name, "Second B");
    assert_eq!(sender.photo.as_deref(), Some("changed-photo"));
    // Original segment-first phone survives a later number/writer switch outside the window.
    connection
        .exec(
            "UPDATE phones_texts SET number='+12065550999' WHERE id='text-000000'",
            [],
        )
        .unwrap();
    build_origins(&db, &dsp, &text, 701);
    let page = read_origins(
        &db,
        &state,
        &dsp,
        &text,
        Request::Anchor {
            place: "1700000000000:text-000100".into(),
            leading: 0,
        },
    )
    .unwrap();
    let exact = page.origins.unwrap().selected.unwrap();
    assert_eq!(exact.origin.id, "text-000001");
    assert_eq!(exact.first_day.number.as_deref(), Some("+12065550999"));
    assert_eq!(
        exact.before_origin.unwrap().number.as_deref(),
        Some("+12065550999")
    );
}

#[tokio::test]
async fn known_inserts_placement_moves_and_retry_preserve_readiness_but_unknown_writes_do_not() {
    use crate::backend::{
        phone::model::Direction as PhoneDirection,
        phone::storage::{Reported, TextStore as _},
    };
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    build_origins(&db, &dsp, &slack, 701);
    build_origins(&db, &dsp, &text, 701);
    // Leave a canonical microsecond gap inside a huge same-writer run, rebuild the dirty
    // fixture, then insert through the real normal writer. Only x and its successor change.
    connection.exec("DELETE FROM communications_messages WHERE conversation_id=? AND ts='1700000000.000050'",[slack.id()]).unwrap();
    build_origins(&db, &dsp, &slack, 700);
    db.keep_communications_message(
        &dsp,
        &Written {
            team: "T0FIXTURE1".into(),
            user: "U0JORDAN".into(),
            channel: "D0JORDAN".into(),
            name: "Jordan".into(),
            ts: "1700000000.000050".into(),
            thread_ts: None,
            text: "Inserted".into(),
            at: AT,
            edited_at: None,
            files: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        read_origins(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );
    let phone: (String,) = connection
        .one_as(
            "SELECT device_id FROM phones_texts WHERE id='text-000000'",
            [],
        )
        .unwrap()
        .unwrap();
    let report = db
        .phones_report(
            &dsp,
            &phone.0,
            vec![Reported {
                client_id: "origin-placement".into(),
                direction: PhoneDirection::Out,
                number: "+12065550101".into(),
                body: "Phone origin".into(),
                at: AT,
                files: Vec::new(),
            }],
        )
        .unwrap();
    let id = &report[0].id;
    db.place_communications_text(&dsp, id, &Filed::Number("+12065550101".into()), None)
        .unwrap();
    db.settle_communications_texts(&dsp, std::slice::from_ref(id))
        .unwrap();
    let destination = db
        .open_communications_text(&dsp, &Filed::Number("+12065550102".into()))
        .unwrap()
        .id;
    let there = Reference::Text(destination.clone());
    build_origins(&db, &dsp, &there, 0);
    db.refile_communications_texts(
        &dsp,
        &[
            ("text-000100".into(), Filed::Number("+12065550102".into())),
            ("text-000101".into(), Filed::Number("+12065550102".into())),
        ],
    )
    .unwrap();
    for reference in [&text, &there] {
        assert!(
            read_origins(&db, &state, &dsp, reference, Request::Latest)
                .unwrap()
                .origins
                .unwrap()
                .ready
        );
    }
    let target = read_origins(&db, &state, &dsp, &there, Request::Latest).unwrap();
    assert_eq!(
        target.origins.unwrap().selected.unwrap().origin.id,
        "text-000100"
    );
    // A real failed text's retry preserves ID/at/name; transport device changes do not split a
    // member's writer. Raw fixture setup first dirties and is rebuilt through production code.
    connection
        .exec(
            "UPDATE phones_texts SET origin='dispatch',direction='out',sent_by='user_1', \
        sender_name='Stored retry name',status='failed',attempt=1 WHERE id='text-000200'",
            [],
        )
        .unwrap();
    build_origins(&db, &dsp, &text, 701);
    let retried = db
        .phones_retry(&dsp, "text-000200", AT + 1_000_000)
        .unwrap();
    assert_eq!(retried.id, "text-000200");
    assert_eq!(retried.at, AT);
    let exact = read_origins(
        &db,
        &state,
        &dsp,
        &text,
        Request::Anchor {
            place: "1700000000000:text-000200".into(),
            leading: 0,
        },
    )
    .unwrap();
    assert!(exact.origins.as_ref().unwrap().ready);
    assert_eq!(
        exact
            .origins
            .unwrap()
            .selected
            .unwrap()
            .origin
            .sender
            .unwrap()
            .name,
        "Stored retry name"
    );
    // An enclosing operation failure rolls source + index + epoch back together.
    let before: (i64,) = connection
        .one_as(
            "SELECT source_epoch FROM communications_origin_state WHERE \
        transport='text' AND conversation_id=?",
            [text.id()],
        )
        .unwrap()
        .unwrap();
    let failed: Result<()> = connection.transaction(|| {
        db.refile_communications_texts(
            &dsp,
            &[("text-000102".into(), Filed::Number("+12065550102".into()))],
        )?;
        Err(Error::new("fixture_rollback", 500))
    });
    assert!(failed.is_err());
    let after: (i64,) = connection
        .one_as(
            "SELECT source_epoch FROM communications_origin_state WHERE \
        transport='text' AND conversation_id=?",
            [text.id()],
        )
        .unwrap()
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        db.communications_text(&dsp, "text-000102")
            .unwrap()
            .unwrap()
            .conversation
            .as_deref(),
        Some(text.id())
    );
    // Storage errors still roll the entire source savepoint back; only an unsupported
    // derived calendar projection can leave the source successful and its index dirty.
    let boundaries = connection
        .all(
            "SELECT * FROM communications_text_origins ORDER BY conversation_id,generation,at,id",
            [],
        )
        .unwrap();
    let broken = db
        .phones_report(
            &dsp,
            &phone.0,
            vec![Reported {
                client_id: "origin-storage-error".into(),
                direction: PhoneDirection::In,
                number: "+12065550101".into(),
                body: "Storage refusal".into(),
                at: AT + 1_000_000,
                files: Vec::new(),
            }],
        )
        .unwrap();
    connection
        .exec(
            "CREATE TEMP TRIGGER origin_storage_error BEFORE INSERT ON communications_text_origins \
        BEGIN SELECT RAISE(ABORT,'fixture_origin_storage_error'); END",
            [],
        )
        .unwrap();
    let refused = db
        .place_communications_text(
            &dsp,
            &broken[0].id,
            &Filed::Number("+12065550101".into()),
            None,
        )
        .unwrap_err();
    assert_eq!(refused.code, "operation_failed");
    assert!(
        std::error::Error::source(&refused)
            .unwrap()
            .to_string()
            .contains("fixture_origin_storage_error")
    );
    connection
        .exec("DROP TRIGGER origin_storage_error", [])
        .unwrap();
    let unfiled: (Option<String>,) = connection
        .one_as(
            "SELECT conversation_id FROM phones_texts WHERE id=?",
            [&broken[0].id],
        )
        .unwrap()
        .unwrap();
    assert!(unfiled.0.is_none());
    let restored: (i64,) = connection
        .one_as(
            "SELECT source_epoch FROM communications_origin_state WHERE \
        transport='text' AND conversation_id=?",
            [text.id()],
        )
        .unwrap()
        .unwrap();
    assert_eq!(restored, after);
    assert_eq!(connection.all("SELECT * FROM communications_text_origins ORDER BY conversation_id,generation,at,id",[]).unwrap(),boundaries);
    assert!(
        read_origins(&db, &state, &dsp, &text, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );

    // Index readiness must not change which raw transport dates the filing pipeline accepts.
    let bad = db
        .phones_report(
            &dsp,
            &phone.0,
            vec![Reported {
                client_id: "bad-origin-date".into(),
                direction: PhoneDirection::In,
                number: "+12065550101".into(),
                body: "Filed without an index".into(),
                at: i64::MAX,
                files: Vec::new(),
            }],
        )
        .unwrap();
    assert_eq!(
        db.place_communications_text(
            &dsp,
            &bad[0].id,
            &Filed::Number("+12065550101".into()),
            None
        )
        .unwrap(),
        text.id()
    );
    let filed: (Option<String>,) = connection
        .one_as(
            "SELECT conversation_id FROM phones_texts WHERE id=?",
            [&bad[0].id],
        )
        .unwrap()
        .unwrap();
    assert_eq!(filed.0.as_deref(), Some(text.id()));
    let dirty: (i64, Option<i64>) = connection
        .one_as(
            "SELECT source_epoch,indexed_epoch FROM communications_origin_state WHERE \
        transport='text' AND conversation_id=?",
            [text.id()],
        )
        .unwrap()
        .unwrap();
    assert_eq!(dirty, (after.0 + 1, Some(after.0)));
    assert_eq!(connection.all("SELECT * FROM communications_text_origins ORDER BY conversation_id,generation,at,id",[]).unwrap(),boundaries);

    // Unknown/old writer invalidation is durable across a later known send.
    connection.exec("DELETE FROM communications_messages WHERE conversation_id=? AND ts='1700000000.000050'",[slack.id()]).unwrap();
    db.keep_communications_message(
        &dsp,
        &Written {
            team: "T0FIXTURE1".into(),
            user: "U0JORDAN".into(),
            channel: "D0JORDAN".into(),
            name: "Jordan".into(),
            ts: "1700000001.000000".into(),
            thread_ts: None,
            text: "After unknown write".into(),
            at: AT + 1000,
            edited_at: None,
            files: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        !read_origins(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );
    assert_eq!(
        read(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .messages
            .len(),
        150
    );
    build_origins(&db, &dsp, &slack, 702);
    assert!(
        read_origins(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );
    // Group restoration changes projected writer despite there being no message UPDATE.
    connection.exec("UPDATE communications_messages SET direction='out',group_id='restored-group',sent_by='user_1' \
        WHERE conversation_id=? AND ts='1700000000.000051'",[slack.id()]).unwrap();
    build_origins(&db, &dsp, &slack, 702);
    connection
        .exec(
            "INSERT INTO communications_groups(id,name,created_by,created_at,last_at) \
        VALUES ('restored-group','Current group','user_1',0,0)",
            [],
        )
        .unwrap();
    assert!(
        !read_origins(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );
    build_origins(&db, &dsp, &slack, 702);
    let grouped = read_origins(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Anchor {
            place: "1700000000.000051".into(),
            leading: 0,
        },
    )
    .unwrap();
    assert_eq!(
        grouped
            .origins
            .unwrap()
            .selected
            .unwrap()
            .origin
            .group
            .unwrap()
            .name,
        "Current group"
    );
    connection
        .exec(
            "DELETE FROM communications_groups WHERE id='restored-group'",
            [],
        )
        .unwrap();
    assert!(
        !read_origins(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .origins
            .unwrap()
            .ready
    );
}

#[tokio::test]
async fn exact_pause_threshold_and_recurring_slack_date_segments_keep_true_origins() {
    for gap in [599_999_i64, 600_000] {
        let (_root, db, state, dsp, slack, text) = fixture(701);
        let connection = db.dsp(&dsp).unwrap();
        connection.exec("UPDATE communications_messages SET direction='out',sent_by='user_1',sender_name=CASE \
            WHEN ts='1700000000.000000' THEN 'First' WHEN ts='1700000000.000100' THEN 'After pause' ELSE 'Interior' END, \
            at=CASE WHEN ts<'1700000000.000100' THEN ?1 ELSE ?2 END WHERE conversation_id=?3",params![AT,AT+gap,slack.id()]).unwrap();
        connection.exec("UPDATE phones_texts SET direction='out',sent_by='user_1',sender_name=CASE \
            WHEN id='text-000000' THEN 'First' WHEN id='text-000100' THEN 'After pause' ELSE 'Interior' END, \
            at=CASE WHEN id<'text-000100' THEN ?1 ELSE ?2 END WHERE conversation_id=?3",params![AT,AT+gap,text.id()]).unwrap();
        for reference in [&slack, &text] {
            build_origins(&db, &dsp, reference, 701);
            let anchor = match reference {
                Reference::Slack(_) => "1700000000.000100".to_owned(),
                Reference::Text(_) => format!("{:013}:text-000100", AT + gap),
            };
            let page = read_origins(
                &db,
                &state,
                &dsp,
                reference,
                Request::Anchor {
                    place: anchor,
                    leading: 0,
                },
            )
            .unwrap();
            let facts = page.origins.unwrap();
            assert!(facts.ready);
            let expected = if gap == 599_999 {
                "First"
            } else {
                "After pause"
            };
            for facts in [facts.selected.unwrap(), facts.latest.unwrap()] {
                assert_eq!(facts.origin.sender.unwrap().name, expected);
                assert_eq!(
                    facts.sent_before_origin.map(|sender| sender.name),
                    (gap == 600_000).then(|| "First".to_owned())
                );
            }
        }
    }
    // Slack's persisted at may move backwards relative to exact ts order. The same calendar
    // label can recur in a later segment; no global-date clamp or timestamp normalization fits.
    let (_root, db, state, dsp, slack, _text) = fixture(701);
    let name = db.find_dsp(&dsp).unwrap().name;
    db.keep_dsp_profile(&dsp, &name, "UTC", None, None).unwrap();
    let connection = db.dsp(&dsp).unwrap();
    let at = |value: &str| {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    };
    connection.exec("UPDATE communications_messages SET direction=CASE WHEN ts>='1700000000.000100' \
        AND ts<'1700000000.000200' THEN 'in' ELSE 'out' END,sent_by=CASE WHEN ts>='1700000000.000100' \
        AND ts<'1700000000.000200' THEN NULL ELSE 'user_1' END,sender_name=CASE \
        WHEN ts='1700000000.000050' THEN 'New day first' ELSE 'Interior' END,at=CASE \
        WHEN ts<'1700000000.000050' THEN ?1 WHEN ts<'1700000000.000100' THEN ?2 \
        WHEN ts<'1700000000.000200' THEN ?3 WHEN ts<'1700000000.000600' THEN ?4 \
        WHEN ts<'1700000000.000650' THEN ?5 ELSE ?6 END WHERE conversation_id=?7",params![
        at("2024-01-01T23:59:00Z"),at("2024-01-02T00:01:00Z"),at("2024-01-02T00:02:00Z"),
        at("2024-01-02T00:03:00Z"),at("2024-01-01T23:58:00Z"),at("2024-01-02T00:04:00Z"),slack.id()]).unwrap();
    build_origins(&db, &dsp, &slack, 701);
    let exact = read_origins(
        &db,
        &state,
        &dsp,
        &slack,
        Request::Anchor {
            place: "1700000000.000100".into(),
            leading: 0,
        },
    )
    .unwrap()
    .origins
    .unwrap();
    let selected = exact.selected.unwrap();
    let latest = exact.latest.unwrap();
    assert_eq!(selected.first_day.ts, "1700000000.000050");
    assert_eq!(selected.sent_before_origin.unwrap().name, "New day first");
    assert_eq!(latest.origin.ts, "1700000000.000650");
    assert_eq!(latest.first_day.ts, "1700000000.000650");
    assert!(latest.sent_before_origin.is_none());
}

#[tokio::test]
async fn origin_restarts_drain_obsolete_generations_without_deleting_source_history() {
    use crate::backend::origins::{Transport, backfill};
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    let zone = texts::zone(&db, &dsp).unwrap();
    for reference in [&slack, &text] {
        let transport = Transport::of(reference);
        let (source, boundaries, conversations, place, body) = match reference {
            Reference::Slack(_) => (
                "communications_messages",
                "communications_slack_origins",
                "communications_conversations",
                "ts='1700000000.000700'",
                "text",
            ),
            Reference::Text(_) => (
                "phones_texts",
                "communications_text_origins",
                "communications_text_conversations",
                "id='text-000700'",
                "body",
            ),
        };
        let ordinal = match reference {
            Reference::Slack(_) => "CAST(substr(ts,12) AS INTEGER)",
            Reference::Text(_) => "CAST(substr(id,6) AS INTEGER)",
        };
        connection.exec(&format!("UPDATE {source} SET direction=CASE WHEN {ordinal}%2=0 THEN 'in' ELSE 'out' END \
            WHERE conversation_id=?"),[reference.id()]).unwrap();
        build_origins(&db, &dsp, reference, 701);
        assert_eq!(
            connection
                .count(
                    &format!(
                        "SELECT count(*) FROM {boundaries} b JOIN communications_origin_state s \
             ON s.conversation_id=b.conversation_id AND s.active_generation=b.generation \
             WHERE s.transport=? AND b.conversation_id=?"
                    ),
                    params![transport.name(), reference.id()]
                )
                .unwrap(),
            701
        );
        // The earlier transport's real upkeep can have published the other transport's
        // one-boundary generation. Finish one bounded ready visit before the restart baseline.
        assert!(backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let source_rows = connection
            .all(
                &format!("SELECT id,at,{body} FROM {source} WHERE conversation_id=? ORDER BY id"),
                [reference.id()],
            )
            .unwrap();
        let count = || {
            connection
                .count(
                    &format!("SELECT count(*) FROM {boundaries} WHERE conversation_id=?"),
                    [reference.id()],
                )
                .unwrap()
        };
        assert_eq!(count(), 701);
        for attempt in 0..12 {
            connection
                .exec(
                    &format!("UPDATE {source} SET at=? WHERE conversation_id=? AND {place}"),
                    params![AT + (attempt % 2) + 1, reference.id()],
                )
                .unwrap();
            assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
            assert!(
                count() <= 701 + 256,
                "restarts must drain abandoned candidates as fast as they create boundaries"
            );
        }
        connection
            .exec(
                &format!("UPDATE {source} SET at=? WHERE conversation_id=? AND {place}"),
                params![AT, reference.id()],
            )
            .unwrap();
        build_origins(&db, &dsp, reference, 701);
        assert_eq!(
            connection
                .all(
                    &format!(
                        "SELECT id,at,{body} FROM {source} WHERE conversation_id=? ORDER BY id"
                    ),
                    [reference.id()]
                )
                .unwrap(),
            source_rows
        );
        assert!(count() <= 1402);
        for _ in 0..4 {
            let before = count();
            assert!(backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
            let after = count();
            assert!((0..=256).contains(&(before - after)));
        }
        assert_eq!(count(), 701);
        // Existing conversation removal may leave derived ownership behind. Each subsequent
        // maintenance unit drains at most one chunk, then removes the now-empty owner state.
        connection
            .exec(
                &format!("DELETE FROM {source} WHERE conversation_id=?"),
                [reference.id()],
            )
            .unwrap();
        connection
            .exec(
                &format!("DELETE FROM {conversations} WHERE id=?"),
                [reference.id()],
            )
            .unwrap();
        let cached = support::cached(&state, &dsp);
        for _ in 0..8 {
            let before = count();
            (backfill::MAINTENANCE.run)(state.clone(), true).await;
            assert!((0..=256).contains(&(before - count())));
        }
        assert!(
            cached(),
            "origin maintenance must not invalidate unchanged cached DSP reads"
        );
        assert_eq!(count(), 0);
        assert_eq!(connection.count("SELECT count(*) FROM communications_origin_state WHERE transport=? AND conversation_id=?",
            params![transport.name(),reference.id()]).unwrap(),0);
        assert_eq!(connection.count("SELECT count(*) FROM communications_origin_builds WHERE transport=? AND conversation_id=?",
            params![transport.name(),reference.id()]).unwrap(),0);
    }
}
