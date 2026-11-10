//! Window traversal and authoritative refresh through the production reader, with fixture SQL
//! for long history, and the work its seeks are held to. Expected IDs come from the fixture's
//! explicit order, not the selector.
use super::*;
use crate::backend::{
    changes::changed_conversation,
    origins,
    phone::{storage::TextStore as PhoneTextStore, support::paired},
    storage::messages::{MessageStore, Written},
    support::{
        self, ReadBudget,
        windows::{AT, bounded, build_origins, fixture, ready, row_100},
        written,
    },
    texts::{
        model::{Filed, TextStatus},
        storage::TextStore,
    },
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
};

fn bodies(page: &ConversationWindow) -> Vec<usize> {
    page.messages
        .iter()
        .map(|message| message.text.parse().unwrap())
        .collect()
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
    let historical =
        |reference: &Reference| read(&db, &state, &dsp, reference, row_100(reference, 50)).unwrap();
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
                user: "U0ELSE".into(),
                channel: "D0ELSE".into(),
                name: "Else".into(),
                ..written("1700000000.000001", AT, "Else")
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
    let page = read_origins(&db, &state, &dsp, &slack, row_100(&slack, 0)).unwrap();
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
    let selected =
        |reference: &Reference| read(&db, &state, &dsp, reference, row_100(reference, 50)).unwrap();
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
            assert!(!ready(&db, &state, &dsp, &text));
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
async fn window_seeks_bound_work_at_both_ends_and_inside_a_long_tied_history() {
    support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let phone = paired(&store, &dsp, "window-budget-key", "+12065550100");
    let text = store
        .open_communications_text(&dsp, &Filed::Number("+12065550101".into()))
        .unwrap()
        .id;
    let slack = store
        .keep_communications_message(
            &dsp,
            &written("1700000000.000000", 1_700_000_000_000, "First"),
        )
        .unwrap()
        .unwrap()
        .conversation;
    let db = store.dsp(&dsp).unwrap();
    db.exec("WITH RECURSIVE history(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM history WHERE n<10000) \
        INSERT INTO communications_messages(id,conversation_id,ts,direction,text,at) \
        SELECT 'slack-'||n,?1,printf('1700000000.%06d',n),'in','History',1700000000000 FROM history", [&slack]).unwrap();
    db.exec("WITH RECURSIVE history(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM history WHERE n<10000) \
        INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number,body,at,change,conversation_id) \
        SELECT printf('text-%06d',n),?1,printf('text-%06d',n),'in','phone','+12065550101',\
        'History',1700000000000,n,?2 FROM history", rusqlite::params![phone.id, text]).unwrap();
    for n in [0, 5_000, 10_000] {
        for leading in [0, 75, 149] {
            let place = format!("1700000000.{n:06}");
            let id = format!("text-{n:06}");
            // Each complete window selector has 20,000 SQLite instructions of its own. This
            // is a work bound, never an elapsed-time test.
            let budget = ReadBudget::new(&db.0);
            let page = store
                .communications_message_window(&dsp, &slack, Some((&place, leading)))
                .unwrap();
            drop(budget);
            assert_eq!(page.messages.len(), 150);
            let budget = ReadBudget::new(&db.0);
            let page = store
                .communications_text_window(&dsp, &text, Some((1_700_000_000_000, &id, leading)))
                .unwrap();
            drop(budget);
            assert_eq!(page.messages.len(), 150);
        }
    }
    let budget = ReadBudget::new(&db.0);
    let page = store.communications_text_window(&dsp, &text, None).unwrap();
    drop(budget);
    assert!(page.has_before && !page.has_after);
    let budget = ReadBudget::new(&db.0);
    let page = store
        .communications_message_window(&dsp, &slack, None)
        .unwrap();
    drop(budget);
    assert!(page.has_before && !page.has_after);

    // 004900 is a raw left-side candidate, outside the selected [004925,005074].
    // Hydrating candidate attachments before selection would exceed the same work guard.
    db.exec(
        "WITH RECURSIVE files(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM files WHERE n<2000) \
        INSERT INTO phones_files(id,text_id,name,mime,size,added_at) \
        SELECT 'excluded-'||n,'text-004900','Excluded','image/png',1,0 FROM files",
        [],
    )
    .unwrap();
    db.exec(
        "INSERT INTO phones_files(id,text_id,name,mime,size,added_at) \
        VALUES ('selected-file','text-005000','Selected','image/png',1,0)",
        [],
    )
    .unwrap();
    let budget = ReadBudget::new(&db.0);
    let page = store
        .communications_text_window(&dsp, &text, Some((1_700_000_000_000, "text-005000", 75)))
        .unwrap();
    drop(budget);
    assert!(
        !page
            .messages
            .iter()
            .any(|message| message.id == "text-004900")
    );
    assert_eq!(
        page.messages
            .iter()
            .find(|message| message.id == "text-005000")
            .unwrap()
            .files[0]
            .name,
        "Selected"
    );
    // Sparse outgoing rows, in these same 10,001-row histories, under the same guard.
    db.exec(
        "UPDATE communications_messages SET direction='out',sent_by='user_1', \
        sender_name='Earlier sender' WHERE conversation_id=? AND ts='1700000000.000001'",
        [&slack],
    )
    .unwrap();
    db.exec(
        "UPDATE phones_texts SET direction='out' WHERE id='text-000001'",
        [],
    )
    .unwrap();
    // These belong to the prior outgoing row, far outside both full-body selections.
    db.exec(
        "WITH RECURSIVE files(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM files WHERE n<2000) \
        INSERT INTO phones_files(id,text_id,name,mime,size,added_at) SELECT \
        'render-excluded-'||n,'text-000001','Excluded','image/png',1,0 FROM files",
        [],
    )
    .unwrap();
    db.exec(
        "WITH RECURSIVE files(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM files WHERE n<2000) \
        INSERT INTO communications_files(id,message_id,name,mime,size,status,added_at) SELECT \
        'context-slack-excluded-'||n,'slack-1','Excluded','image/png',1,'waiting',0 FROM files",
        [],
    )
    .unwrap();
    let state = dispatch_core::State::new(store.config.clone()).unwrap();
    let people = People::read(&store, &state, &dsp).unwrap();
    let numbers = Numbers::read(&store, &dsp).unwrap();
    let selected_slack = conversations::messages(
        &store,
        &people,
        &dsp,
        store
            .communications_message_window(&dsp, &slack, Some(("1700000000.005000", 75)))
            .unwrap()
            .messages,
    )
    .unwrap();
    let latest_slack = conversations::messages(
        &store,
        &people,
        &dsp,
        store
            .communications_messages(&dsp, &slack, None, 50)
            .unwrap(),
    )
    .unwrap();
    let selected_text = texts::reading::messages(
        &people,
        &numbers,
        store
            .communications_text_window(&dsp, &text, Some((1_700_000_000_000, "text-005000", 75)))
            .unwrap()
            .messages,
    );
    let latest_text = texts::reading::messages(
        &people,
        &numbers,
        store.communications_texts(&dsp, &text, None, 50).unwrap(),
    );
    for (reference, messages, numbers, previous, next, sender) in [
        (
            Reference::Slack(slack.clone()),
            &selected_slack,
            None,
            "1700000000.004924",
            Some("1700000000.005075"),
            "Earlier sender",
        ),
        (
            Reference::Slack(slack.clone()),
            &latest_slack,
            None,
            "1700000000.009950",
            None,
            "Earlier sender",
        ),
        (
            Reference::Text(text.clone()),
            &selected_text,
            Some(&numbers),
            "1700000000000:text-004924",
            Some("1700000000000:text-005075"),
            numbers.phone(&phone.id).unwrap(),
        ),
        (
            Reference::Text(text.clone()),
            &latest_text,
            Some(&numbers),
            "1700000000000:text-009950",
            None,
            numbers.phone(&phone.id).unwrap(),
        ),
    ] {
        let budget = ReadBudget::new(&db.0);
        let context = context::read(&store, &dsp, &reference, messages, numbers, None).unwrap();
        drop(budget);
        assert_eq!(context.previous.unwrap().ts, previous);
        assert_eq!(context.next.map(|row| row.ts).as_deref(), next);
        assert_eq!(context.sent_before.unwrap().name, sender);
    }
    // Build actual sparse boundaries from source metadata, outside only the existing context
    // guard's setup. One guard covers readiness AND both independent full legacy+origin contexts.
    let zone = texts::zone(&store, &dsp).unwrap();
    for reference in [
        Reference::Slack(slack.clone()),
        Reference::Text(text.clone()),
    ] {
        build_origins(&store, &dsp, &reference, 10_001);
    }
    for (reference, selected, latest, numbers, expected) in [
        (
            Reference::Slack(slack.clone()),
            &selected_slack,
            &latest_slack,
            None,
            "1700000000.000002",
        ),
        (
            Reference::Text(text.clone()),
            &selected_text,
            &latest_text,
            Some(&numbers),
            "1700000000000:text-000002",
        ),
    ] {
        let budget = ReadBudget::new(&db.0);
        let _ = context::read(&store, &dsp, &reference, selected, numbers, Some(zone)).unwrap();
        let _ = context::read(&store, &dsp, &reference, latest, numbers, Some(zone)).unwrap();
        let exact =
            origins::read(&db, &reference, zone, selected, latest, &people, numbers).unwrap();
        drop(budget);
        assert!(exact.ready);
        for facts in [exact.selected.unwrap(), exact.latest.unwrap()] {
            assert_eq!(facts.origin.ts, expected);
            assert!(facts.sent_before_origin.is_some());
        }
    }

    // An entirely absent outgoing prefix must also be a bounded index seek, not a scan.
    db.exec("UPDATE communications_messages SET direction='in' WHERE conversation_id=? AND direction='out'", [&slack]).unwrap();
    db.exec(
        "UPDATE phones_texts SET direction='in' WHERE conversation_id=? AND direction='out'",
        [&text],
    )
    .unwrap();
    for (reference, messages, numbers) in [
        (Reference::Slack(slack.clone()), &selected_slack, None),
        (
            Reference::Text(text.clone()),
            &selected_text,
            Some(&numbers),
        ),
    ] {
        let budget = ReadBudget::new(&db.0);
        let context = context::read(&store, &dsp, &reference, messages, numbers, None).unwrap();
        drop(budget);
        assert!(context.sent_before.is_none());
    }
    // Dense starts on one day: Slack alternates +600000ms and negative gaps in ts order;
    // texts alternate incoming numbers at tied numeric at. Sparse subsets skip the prefix.
    db.exec(
        "UPDATE communications_messages SET direction='in',sent_by=NULL,at=1700000000000 + \
        (CAST(substr(ts,instr(ts,'.')+1) AS INTEGER)%2)*600000 WHERE conversation_id=?",
        [&slack],
    )
    .unwrap();
    db.exec(
        "UPDATE phones_texts SET direction='in',sent_by=NULL,number=CASE WHEN \
        CAST(substr(id,6) AS INTEGER)%2=0 THEN '+12065550101' ELSE '+12065550102' END \
        WHERE conversation_id=?",
        [&text],
    )
    .unwrap();
    for reference in [
        Reference::Slack(slack.clone()),
        Reference::Text(text.clone()),
    ] {
        build_origins(&store, &dsp, &reference, 10_001);
    }
    let latest_text = texts::reading::messages(
        &people,
        &numbers,
        store.communications_texts(&dsp, &text, None, 50).unwrap(),
    );
    let latest_slack = conversations::messages(
        &store,
        &people,
        &dsp,
        store
            .communications_messages(&dsp, &slack, None, 50)
            .unwrap(),
    )
    .unwrap();
    for (reference, latest, numbers, first_day) in [
        (
            Reference::Slack(slack),
            &latest_slack,
            None,
            "1700000000.000000",
        ),
        (
            Reference::Text(text),
            &latest_text,
            Some(&numbers),
            "1700000000000:text-000000",
        ),
    ] {
        let budget = ReadBudget::new(&db.0);
        let _ = context::read(&store, &dsp, &reference, latest, numbers, Some(zone)).unwrap();
        let exact = origins::read(&db, &reference, zone, latest, latest, &people, numbers).unwrap();
        drop(budget);
        assert!(exact.ready);
        let facts = exact.selected.unwrap();
        assert!(facts.sent_before_origin.is_none());
        assert_eq!(facts.first_day.ts, first_day);
    }
}
