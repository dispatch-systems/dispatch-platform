//! Texting's storage across its upgrades, from a database as the build before migration 9 left it:
//! the phone side's rows of the texts, without what filing keeps on them, and a copy of each text
//! filed (`communications_texts`) up to the last of the phone side's changes it read
//! (`communications_texting`). Once migrations 6 and 9 run, each text reads as it did, its Done,
//! its block and its sender's name as they were kept; and a text the copy never read is filed by
//! the next upkeep.
use super::*;
use crate::backend::{
    inbox,
    model::ConversationKind,
    phone::{
        self,
        model::{Direction, Outgoing, Status},
        storage::{FileStore, Kept, Reported, ReportedFile, StatusReport, TextStore as _},
    },
    support::{self, DRIVERS, at, filed, migrate, phoned, phones, started, today},
    texts::{contacts, filing, model::Filed, reading},
};
use crate::{ConversationState, FileStatus, Message, TextStatus};
use dispatch_core::{
    State,
    db::{Db, Store, now},
    testing,
};
use rusqlite::params;
use std::sync::Arc;

/// Puts the DSP's database back as the build before migration 9 left it: the phone side's texts
/// and files as Communications' migration 7 makes them, with nothing filing keeps on them.
fn before_filing(db: &Store, dsp: &str) {
    let database = db.dsp(dsp).unwrap();
    database
        .0
        .execute_batch("DROP TABLE phones_files; DROP TABLE phones_texts;")
        .unwrap();
    drop(database);
    migrate(db, dsp, &[7]);
}
/// The text a phone reported from `number` at `at`, saying `body`, with `pictures` pictures whose
/// bytes haven't come: its ID, and its pictures'.
fn reported(db: &Store, dsp: &str, number: &str, body: &str, at: i64, pictures: usize) -> Kept {
    let phone = phone::phones(db, dsp).unwrap().remove(0).id;
    let text = Reported {
        client_id: format!("sms-{number}-{at}"),
        direction: Direction::In,
        number: number.into(),
        body: body.into(),
        at,
        files: (0..pictures)
            .map(|index| ReportedFile {
                name: format!("door{index}.jpg"),
                mime: "image/jpeg".into(),
                size: 100,
            })
            .collect(),
    };
    db.phones_report(dsp, &phone, vec![text]).unwrap().remove(0)
}
/// Keeps the text `id` in the copy as the build before migration 9 filed it: in `conversation`,
/// under `sender_name`, blocked as it came or not, its pictures as the copy had them (`files`,
/// JSON).
fn copied(
    db: &Db,
    id: &str,
    conversation: &str,
    sender_name: Option<&str>,
    blocked: bool,
    files: &str,
) {
    db.exec(
        "INSERT INTO communications_texts (id,conversation_id,phone,direction,number,body,at,\
         sent_by,sender_name,status,error,files,blocked) SELECT id,?2,device_id,direction,number,\
         body,at,sent_by,?3,CASE origin WHEN 'dispatch' THEN status END,error,?4,?5 \
         FROM phones_texts WHERE id=?1",
        params![id, conversation, sender_name, files, blocked],
    )
    .unwrap();
}
/// Notes the copy read the phone side's changes up to now.
fn read_to_now(db: &Db) {
    db.exec(
        "INSERT INTO communications_texting (id,read_to) \
         VALUES (1,(SELECT last FROM phones_change)) \
         ON CONFLICT(id) DO UPDATE SET read_to=excluded.read_to",
        [],
    )
    .unwrap();
}
/// The conversation the number `number`'s texts are filed in as itself, made if it's new.
fn number(db: &Store, dsp: &str, number: &str) -> String {
    let filed = Filed::Number(number.into());
    db.open_communications_text(dsp, &filed).unwrap().id
}
/// The text `id` as the page reads it.
async fn message(state: &Arc<State>, dsp: &str, id: &str) -> Message {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    state
        .read(move |db| reading::message(db, &shared, &dsp, &id))
        .await
        .unwrap()
}
/// The conversation `id` as the page reads it: whether it waits, and since when.
async fn standing(state: &Arc<State>, dsp: &str, id: &str) -> (ConversationState, Option<i64>) {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    let shown = state
        .read(move |db| reading::one(db, &shared, &dsp, &id))
        .await
        .unwrap();
    (shown.state, shown.waiting_since)
}

#[tokio::test]
async fn a_done_from_before_migration_6_still_covers_its_texts_once_theyre_filed_again() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    // From 0101, no work phone yet, texts at 6:00 and 6:30, then one at 7:30, filed under its
    // number in the copy; that conversation was marked done at 7:00 as Done was kept before
    // migration 6, on the conversation alone. Migrations 6 and 9 then run over it, as an upgrade
    // does.
    let (at_dsp, done_at) = (dsp.clone(), at(zone, day, 7, 0));
    let texts = state
        .run(move |db| {
            before_filing(db, &at_dsp);
            let conversation = number(db, &at_dsp, "+12065550101");
            let database = db.dsp(&at_dsp)?;
            let mut texts = Vec::new();
            for (body, hour, minute) in [
                ("Which van?", 6, 0),
                ("Any van?", 6, 30),
                ("Van 214 is flat", 7, 30),
            ] {
                let written = at(zone, day, hour, minute);
                let text = reported(db, &at_dsp, "+12065550101", body, written, 0);
                copied(&database, &text.id, &conversation, None, false, "[]");
                texts.push(text.id);
            }
            read_to_now(&database);
            database.exec(
                "UPDATE communications_text_conversations SET done_at=?,done_by='me' WHERE id=?",
                params![done_at, conversation],
            )?;
            drop(database);
            migrate(db, &at_dsp, &[6, 9]);
            Ok(texts)
        })
        .await
        .unwrap();

    // A read shows Avery with 0101, so its texts are theirs. Those the Done covered stay
    // handled; the 7:30 one waits.
    let on_0101 = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&day.to_string()), on_0101, Vec::new()).await;
    let avery = filed(&state, &dsp, &texts[0]).await;
    let theirs = Some((DRIVERS.as_str().to_owned(), "A1".to_owned()));
    assert_eq!(avery.driver, theirs);
    assert_eq!(
        standing(&state, &dsp, &avery.id).await,
        (ConversationState::Waiting, Some(at(zone, day, 7, 30)))
    );
}

#[tokio::test]
async fn texts_filed_before_migration_9_read_as_they_did_their_done_block_and_sender_kept() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test")
        .await
        .auth
        .user
        .id;
    let minute = 60_000;
    // As the build before migration 9 left it, each text filed in the copy:
    let at_dsp = dsp.clone();
    let (asked, answer, done, spam) = state
        .run(move |db| {
            before_filing(db, &at_dsp);
            // a question from Lakeside, and the owner's answer, still waiting for the phone,
            // under the name they had then;
            let lakeside = number(db, &at_dsp, "+14255550134");
            let asked = reported(
                db,
                &at_dsp,
                "+14255550134",
                "Van 208?",
                now() - 9 * minute,
                0,
            );
            let outgoing = Outgoing {
                phone: None,
                to: "+14255550134".into(),
                body: "Van 214 instead".into(),
                files: Vec::new(),
                by: owner,
            };
            let answer = db.phones_send(&at_dsp, &outgoing, now() - 8 * minute)?.id;
            // a text from 0101, no work phone yet, its number's conversation marked done;
            let unheld = number(db, &at_dsp, "+12065550101");
            let done = reported(
                db,
                &at_dsp,
                "+12065550101",
                "Which van?",
                at(zone, day, 6, 30),
                0,
            );
            // and a number that texted, was blocked, and texted again while it was.
            let spam = number(db, &at_dsp, "+18885550142");
            let first = reported(
                db,
                &at_dsp,
                "+18885550142",
                "Can I swap?",
                now() - 3 * minute,
                0,
            );
            db.block_communications_number(&at_dsp, "+18885550142", Some("me"))?;
            let blocked = reported(db, &at_dsp, "+18885550142", "Never mind", now() - minute, 0);

            let database = db.dsp(&at_dsp)?;
            copied(&database, &asked.id, &lakeside, None, false, "[]");
            copied(
                &database,
                &answer,
                &lakeside,
                Some("Sam Owner"),
                false,
                "[]",
            );
            copied(&database, &done.id, &unheld, None, false, "[]");
            database.exec(
                "UPDATE communications_texts SET done_at=?1,done_by='me' WHERE id=?2",
                params![now(), done.id],
            )?;
            copied(&database, &first.id, &spam, None, false, "[]");
            copied(&database, &blocked.id, &spam, None, true, "[]");
            database.exec(
                "UPDATE communications_text_conversations SET state='waiting',waiting_since=?, \
                 last_at=? WHERE id=?",
                params![now() - 3 * minute, now() - minute, spam],
            )?;
            read_to_now(&database);
            drop(database);
            migrate(db, &at_dsp, &[9]);
            Ok((asked.id, answer, done.id, spam))
        })
        .await
        .unwrap();

    // The answer reads under the name its sender had as it was filed, still waiting for the
    // phone, which still has it to send; in Lakeside's conversation, with the question.
    let shown = message(&state, &dsp, &answer).await;
    assert_eq!(
        (shown.sender.unwrap().name.as_str(), shown.status),
        ("Sam Owner", Some(TextStatus::Waiting))
    );
    let went: Vec<_> = support::sent(&state, &dsp).await;
    assert_eq!(went.len(), 1, "still in the phone's outbox");
    assert_eq!(
        filed(&state, &dsp, &answer).await.id,
        filed(&state, &dsp, &asked).await.id
    );

    // The text the Done covered stays handled as a read files it under Avery.
    let on_0101 = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&day.to_string()), on_0101, Vec::new()).await;
    let avery = filed(&state, &dsp, &done).await;
    assert_eq!(avery.kind, ConversationKind::Work);
    assert_eq!(standing(&state, &dsp, &avery.id).await.1, None);

    // Unblocked, the number alerts with what it sent before it was blocked, never with what it
    // sent while it was.
    let (shared, at_dsp, unblocked) = (state.clone(), dsp.clone(), spam.clone());
    state
        .run(move |db| contacts::block(db, &shared, &at_dsp, &unblocked, None))
        .await
        .unwrap();
    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let waiting = state
        .read(move |db| inbox::waiting(db, &shared, &at_dsp))
        .await
        .unwrap();
    let latest: Vec<_> = waiting
        .latest
        .iter()
        .filter(|each| each.conversation_id == spam)
        .map(|each| each.message.text.as_str())
        .collect();
    assert_eq!(latest, ["Can I swap?"]);
}

#[tokio::test]
async fn texts_the_copy_never_read_before_migration_9_are_filed_by_the_next_upkeep() {
    let (_root, state, dsp) = started();
    let minute = 60_000;
    let at_dsp = dsp.clone();
    let (pictured, picture, failed, failing_in, unread) = state
        .run(move |db| {
            before_filing(db, &at_dsp);
            let phone = phone::phones(db, &at_dsp)?.remove(0).id;
            let owner = testing::a_user(db);
            // Read by the copy: a text with a picture whose bytes hadn't come, and a question
            // with its answer, sent, in another conversation.
            let lakeside = number(db, &at_dsp, "+14255550134");
            let pictured = reported(db, &at_dsp, "+14255550134", "Door", now() - 9 * minute, 1);
            let gate = number(db, &at_dsp, "+14255550188");
            let asked = reported(
                db,
                &at_dsp,
                "+14255550188",
                "Gate code?",
                now() - 8 * minute,
                0,
            );
            let outgoing = Outgoing {
                phone: None,
                to: "+14255550188".into(),
                body: "4410".into(),
                files: Vec::new(),
                by: owner,
            };
            let answer = db.phones_send(&at_dsp, &outgoing, now() - 7 * minute)?.id;
            let database = db.dsp(&at_dsp)?;
            let waiting = format!(
                r#"[{{"id":"{}","name":"door0.jpg","mime":"image/jpeg","size":100,"kept":false}}]"#,
                pictured.files[0]
            );
            copied(&database, &pictured.id, &lakeside, None, false, &waiting);
            copied(&database, &asked.id, &gate, None, false, "[]");
            copied(&database, &answer, &gate, Some("Test Owner"), false, "[]");
            database.exec(
                "UPDATE communications_text_conversations SET last_at=? WHERE id=?",
                params![now() - 7 * minute, gate],
            )?;
            read_to_now(&database);
            drop(database);
            // After the copy last read: the picture's bytes come, the answer fails, and a text
            // comes from a number new to the DSP.
            db.phones_file_arrived(&at_dsp, &phone, &pictured.files[0], "sha", 4)?;
            let attempt = db.phones_outbox(&at_dsp, &phone)?[0].1;
            let report = StatusReport {
                id: answer.clone(),
                attempt,
                status: Status::Failed,
                at: now(),
                error: Some("No service".into()),
            };
            db.phones_status(&at_dsp, &phone, &report)?;
            let unread = reported(db, &at_dsp, "+14255550177", "Is this dispatch?", now(), 0);
            migrate(db, &at_dsp, &[9]);
            Ok((
                pictured.id,
                pictured.files[0].clone(),
                answer,
                gate,
                unread.id,
            ))
        })
        .await
        .unwrap();

    // The picture reads as the phone side keeps it; the conversation whose answer failed waits
    // again, on its question.
    let files = message(&state, &dsp, &pictured).await.files;
    let kept: Vec<_> = files
        .iter()
        .map(|file| (file.id.as_str(), file.size, file.status))
        .collect();
    assert_eq!(kept, [(picture.as_str(), 4, FileStatus::Kept)]);
    let shown = message(&state, &dsp, &failed).await;
    assert_eq!(shown.status, Some(TextStatus::Failed));
    assert_eq!(
        standing(&state, &dsp, &failing_in).await.0,
        ConversationState::Waiting
    );

    // The text the copy never read waits to be filed, until the next upkeep files it.
    let (at_dsp, id) = (dsp.clone(), unread.clone());
    let waiting = state
        .read(move |db| db.communications_text(&at_dsp, &id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(waiting.conversation, None);
    (filing::MAINTENANCE.run)(state.clone(), false).await;
    let number = filed(&state, &dsp, &unread).await;
    assert_eq!(
        (number.kind, number.number.as_deref(), number.state),
        (
            ConversationKind::Number,
            Some("+14255550177"),
            ConversationState::Waiting
        )
    );
}
