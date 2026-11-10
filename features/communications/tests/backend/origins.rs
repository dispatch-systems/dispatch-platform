//! Exact origins through the production reader and writers, over the window tests' long
//! history: what a backfill builds, which writes keep a generation ready, and what upkeep drains.
use super::{Transport, backfill};
use crate::backend::{
    model::Reference,
    phone::{
        model::Direction as PhoneDirection,
        storage::{Reported, TextStore as PhoneTextStore},
    },
    storage::messages::MessageStore,
    support::{
        self,
        windows::{AT, bounded, build_origins, fixture, ready, row_100},
        written,
    },
    texts::{self, model::Filed, storage::TextStore},
    windows::{Request, read, read_origins},
};
use dispatch_core::{
    Error, Result,
    db::{Db, Store},
};
use rusqlite::params;

/// A text with the fixture's number, by its phone's own ID of it `client`, as the phone reports it.
fn reported(client: &str, direction: PhoneDirection, body: &str, at: i64) -> Reported {
    Reported {
        client_id: client.into(),
        direction,
        number: "+12065550101".into(),
        body: body.into(),
        at,
        files: Vec::new(),
    }
}
/// A real source writer: Slack insertion or phone reporting followed by first filing.
fn place_for_candidate(db: &Store, dsp: &str, reference: &Reference, key: &str, at: i64) -> String {
    match reference {
        Reference::Slack(_) => {
            let kept = db
                .keep_communications_message(dsp, &written(key, at, "suffix"))
                .unwrap()
                .unwrap();
            assert_eq!(kept.conversation, reference.id());
            let (id,): (String,) = db
                .dsp(dsp)
                .unwrap()
                .one_as(
                    "SELECT id FROM communications_messages WHERE conversation_id=? AND ts=?",
                    params![reference.id(), key],
                )
                .unwrap()
                .unwrap();
            id
        }
        Reference::Text(_) => {
            let connection = db.dsp(dsp).unwrap();
            let (phone,): (String,) = connection
                .one_as(
                    "SELECT device_id FROM phones_texts WHERE id='text-000000'",
                    [],
                )
                .unwrap()
                .unwrap();
            let report = db
                .phones_report(
                    dsp,
                    &phone,
                    vec![reported(key, PhoneDirection::In, "suffix", at)],
                )
                .unwrap();
            assert_eq!(report.len(), 1);
            let id = report[0].id.clone();
            let (owner,): (Option<String>,) = connection
                .one_as("SELECT conversation_id FROM phones_texts WHERE id=?", [&id])
                .unwrap()
                .unwrap();
            assert!(owner.is_none());
            assert_eq!(db.place_communications_text(
                dsp, &id, &Filed::Number("+12065550101".into()), None,
            ).unwrap(), reference.id());
            id
        }
    }
}
/// Generation, accounted epoch, exact committed cursor, and current source epoch.
fn candidate_checkpoint(connection: &Db, reference: &Reference) -> (String, i64, String, i64) {
    let transport = Transport::of(reference);
    connection.one_as(
        "SELECT b.generation,b.source_epoch,b.previous,s.source_epoch FROM communications_origin_builds b \
        JOIN communications_origin_state s USING(transport,conversation_id) \
        WHERE b.transport=? AND b.conversation_id=?",
        params![transport.name(), reference.id()],
    ).unwrap().unwrap()
}

#[tokio::test]
async fn clipped_origins_use_stored_run_first_names_and_prior_run_first_for_both_independent_slices()
 {
    use crate::backend::model::Direction;
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
        // unaccounted topology mutation forces a fresh candidate, never partial readiness.
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let old_generation: (String,) = connection
            .one_as(
                "SELECT generation FROM communications_origin_builds \
            WHERE transport=? AND conversation_id=?",
                params![transport.name(), reference.id()],
            )
            .unwrap()
            .unwrap();
        let before = candidate_checkpoint(&connection, reference);
        let previous: serde_json::Value = serde_json::from_str(&before.2).unwrap();
        match reference {
            Reference::Slack(_) => assert!("1700000000.000900" > previous["ts"].as_str().unwrap()),
            Reference::Text(_) => assert!(AT + 1 > previous["at"].as_i64().unwrap()),
        }
        let id = place_for_candidate(&db, &dsp, reference, "1700000000.000900", AT + 1);
        assert_eq!(
            candidate_checkpoint(&connection, reference),
            (
                old_generation.0.clone(),
                before.1 + 1,
                before.2,
                before.1 + 1
            )
        );
        // Remove only this placement before the original source and slice assertions.
        let sql = match reference {
            Reference::Slack(_) => "DELETE FROM communications_messages WHERE id=?",
            Reference::Text(_) => "DELETE FROM phones_texts WHERE id=?",
        };
        connection.exec(sql, [id]).unwrap();
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
            let page = read_origins(&db, &state, &dsp, reference, row_100(reference, 0)).unwrap();
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
    let page = read_origins(&db, &state, &dsp, &text, row_100(&text, 0)).unwrap();
    let exact = page.origins.unwrap().selected.unwrap();
    assert_eq!(exact.origin.id, "text-000001");
    assert_eq!(exact.first_day.number.as_deref(), Some("+12065550999"));
    assert_eq!(
        exact.before_origin.unwrap().number.as_deref(),
        Some("+12065550999")
    );
}

#[tokio::test]
async fn candidate_progress_requires_audited_suffix_and_never_acknowledges_an_unknown_gap() {
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    let zone = texts::zone(&db, &dsp).unwrap();
    for reference in [&slack, &text] {
        let transport = Transport::of(reference);
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let before = candidate_checkpoint(&connection, reference);
        place_for_candidate(&db, &dsp, reference, "1700000000.100001", AT + 1);
        let appended = candidate_checkpoint(&connection, reference);
        assert_eq!(
            appended,
            (
                before.0.clone(),
                before.1 + 1,
                before.2.clone(),
                before.3 + 1
            )
        );

        // A real late placement is not a suffix, even though it is a known single write.
        place_for_candidate(&db, &dsp, reference, "1699999999.999999", AT - 1);
        let late = candidate_checkpoint(&connection, reference);
        assert_eq!(
            late,
            (
                appended.0.clone(),
                appended.1,
                appended.2.clone(),
                appended.3 + 1
            )
        );
        assert!(!ready(&db, &state, &dsp, reference));
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let restarted = candidate_checkpoint(&connection, reference);
        assert_ne!(restarted.0, late.0);
        assert_eq!(restarted.1, late.3);

        // Unknown topology changes cannot be hidden by a later legitimate suffix.
        match reference {
            Reference::Slack(_) => connection.exec(
                "UPDATE communications_messages SET at=at+86400000 WHERE conversation_id=? AND ts='1700000000.000050'",
                [reference.id()],
            ).unwrap(),
            Reference::Text(_) => connection.exec(
                "UPDATE phones_texts SET number='+12065550999' WHERE conversation_id=? AND id='text-000050'",
                [reference.id()],
            ).unwrap(),
        };
        place_for_candidate(&db, &dsp, reference, "1700000000.100002", AT + 2);
        let gap = candidate_checkpoint(&connection, reference);
        assert_eq!(
            gap,
            (
                restarted.0.clone(),
                restarted.1,
                restarted.2,
                restarted.3 + 2
            )
        );
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        let rebuilt = candidate_checkpoint(&connection, reference);
        assert_ne!(rebuilt.0, gap.0);

        // Source and candidate changes share the actual caller's transaction.
        let rolled: dispatch_core::Result<()> = connection.transaction(|| {
            place_for_candidate(&db, &dsp, reference, "1700000000.100003", AT + 3);
            Err(dispatch_core::Error::new("fixture_rollback", 400))
        });
        assert_eq!(rolled.unwrap_err().code, "fixture_rollback");
        assert_eq!(candidate_checkpoint(&connection, reference), rebuilt);

        // An unreadable derived cursor must not prevent a normal source write.
        connection.exec(
            "UPDATE communications_origin_builds SET previous='{' WHERE transport=? AND conversation_id=?",
            params![transport.name(), reference.id()],
        ).unwrap();
        place_for_candidate(&db, &dsp, reference, "1700000000.100004", AT + 4);
        let invalid = candidate_checkpoint(&connection, reference);
        assert_eq!(
            invalid,
            (rebuilt.0.clone(), rebuilt.1, "{".into(), rebuilt.3 + 1)
        );
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());

        let current = candidate_checkpoint(&connection, reference);
        connection.exec(
            "UPDATE communications_origin_builds SET algorithm='unsupported-fixture' WHERE transport=? AND conversation_id=?",
            params![transport.name(), reference.id()],
        ).unwrap();
        place_for_candidate(&db, &dsp, reference, "1700000000.100005", AT + 5);
        let unsupported = candidate_checkpoint(&connection, reference);
        assert_eq!(
            unsupported,
            (current.0, current.1, current.2, current.3 + 1)
        );
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());
        // An additional trigger write inside a legitimate placement is still unaccounted.
        let counted = candidate_checkpoint(&connection, reference);
        let trigger = match reference {
            Reference::Slack(_) => {
                "CREATE TRIGGER fixture_extra_slack AFTER INSERT ON communications_messages \
                WHEN NEW.ts='1700000000.100006' BEGIN \
                UPDATE communications_messages SET at=at+86400000 \
                WHERE conversation_id=NEW.conversation_id AND ts='1700000000.000060'; END"
            }
            Reference::Text(_) => {
                "CREATE TRIGGER fixture_extra_text AFTER UPDATE OF conversation_id ON phones_texts \
                WHEN OLD.conversation_id IS NULL AND NEW.client_id='1700000000.100006' BEGIN \
                UPDATE phones_texts SET number='+12065550888' \
                WHERE conversation_id=NEW.conversation_id AND id='text-000060'; END"
            }
        };
        connection.exec(trigger, []).unwrap();
        place_for_candidate(&db, &dsp, reference, "1700000000.100006", AT + 6);
        connection
            .exec(
                match reference {
                    Reference::Slack(_) => "DROP TRIGGER fixture_extra_slack",
                    Reference::Text(_) => "DROP TRIGGER fixture_extra_text",
                },
                [],
            )
            .unwrap();
        assert_eq!(
            candidate_checkpoint(&connection, reference),
            (counted.0, counted.1, counted.2, counted.3 + 2)
        );
        assert!(!backfill::chunk(&connection, transport, reference.id(), zone).unwrap());

        let sql = match reference {
            Reference::Slack(_) => {
                "SELECT count(*) FROM communications_messages WHERE conversation_id=?"
            }
            Reference::Text(_) => "SELECT count(*) FROM phones_texts WHERE conversation_id=?",
        };
        assert_eq!(connection.count(sql, [reference.id()]).unwrap(), 707);
        build_origins(&db, &dsp, reference, 707);
        assert!(ready(&db, &state, &dsp, reference));
    }
}

#[tokio::test]
async fn known_inserts_placement_moves_and_retry_preserve_readiness_but_unknown_writes_do_not() {
    let (_root, db, state, dsp, slack, text) = fixture(701);
    let connection = db.dsp(&dsp).unwrap();
    build_origins(&db, &dsp, &slack, 701);
    build_origins(&db, &dsp, &text, 701);
    // Leave a canonical microsecond gap inside a huge same-writer run, rebuild the dirty
    // fixture, then insert through the real normal writer. Only x and its successor change.
    connection.exec("DELETE FROM communications_messages WHERE conversation_id=? AND ts='1700000000.000050'",[slack.id()]).unwrap();
    build_origins(&db, &dsp, &slack, 700);
    db.keep_communications_message(&dsp, &written("1700000000.000050", AT, "Inserted"))
        .unwrap();
    assert!(ready(&db, &state, &dsp, &slack));
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
            vec![reported(
                "origin-placement",
                PhoneDirection::Out,
                "Phone origin",
                AT,
            )],
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
        assert!(ready(&db, &state, &dsp, reference));
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
            vec![reported(
                "origin-storage-error",
                PhoneDirection::In,
                "Storage refusal",
                AT + 1_000_000,
            )],
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
    assert!(ready(&db, &state, &dsp, &text));

    // Index readiness must not change which raw transport dates the filing pipeline accepts.
    let bad = db
        .phones_report(
            &dsp,
            &phone.0,
            vec![reported(
                "bad-origin-date",
                PhoneDirection::In,
                "Filed without an index",
                i64::MAX,
            )],
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
        &written("1700000001.000000", AT + 1000, "After unknown write"),
    )
    .unwrap();
    assert!(!ready(&db, &state, &dsp, &slack));
    assert_eq!(
        read(&db, &state, &dsp, &slack, Request::Latest)
            .unwrap()
            .messages
            .len(),
        150
    );
    build_origins(&db, &dsp, &slack, 702);
    assert!(ready(&db, &state, &dsp, &slack));
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
    assert!(!ready(&db, &state, &dsp, &slack));
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
    assert!(!ready(&db, &state, &dsp, &slack));
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
    let exact = read_origins(&db, &state, &dsp, &slack, row_100(&slack, 0))
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
