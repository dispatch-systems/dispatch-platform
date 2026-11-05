//! The texts the DSP's phones relay, against a real store in a temporary directory: the texts a
//! phone reports, each kept once, and how Dispatch's texts go.
use super::*;
use crate::backend::phone::{
    model::{Direction, EXPIRY_MS, Origin, PHONE_OFFLINE, Status},
    storage::{DeviceStore, FileStore, Pairing, ReportedFile},
    support::{outgoing, paired, reported, store},
};
use dispatch_core::testing;

#[test]
fn a_text_reported_twice_is_kept_once() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let first = db
        .phones_report(
            &dsp,
            &phone.id,
            vec![reported("a", "+14255550199", 5_000, 2)],
        )
        .unwrap();
    let again = db
        .phones_report(
            &dsp,
            &phone.id,
            vec![
                reported("a", "+14255550199", 5_000, 2),
                reported("b", "+14255550199", 6_000, 0),
            ],
        )
        .unwrap();
    assert_eq!(again[0].id, first[0].id);
    assert_eq!(again[0].files, first[0].files);
    assert_eq!(first[0].files.len(), 2);
    // Only what a report keeps anew is new, for filing.
    let new: Vec<_> = first.iter().chain(&again).map(|kept| kept.new).collect();
    assert_eq!(new, [true, false, true]);
    let text = db.phones_text(&dsp, &first[0].id).unwrap().unwrap();
    assert_eq!(
        (text.direction, text.origin),
        (Direction::In, Origin::Phone)
    );
    assert!(text.files.iter().all(|file| !file.kept), "bytes come after");
    // Its picture's bytes arrive: the text changes again, after the other, which arriving answers.
    let of = db
        .phones_file_arrived(&dsp, &phone.id, &first[0].files[0], "sha", 100)
        .unwrap();
    assert_eq!(of, first[0].id);
    let later = db.phones_text(&dsp, &first[0].id).unwrap().unwrap();
    let other = db.phones_text(&dsp, &again[1].id).unwrap().unwrap();
    assert!(later.files[0].kept);
    assert!(later.change > other.change);
    // Another phone can't send bytes for it.
    db.phones_new_pairing(&dsp, &testing::a_user(&db), "other", 7_000)
        .unwrap();
    let pairing = Pairing {
        number: "",
        model: "Pixel",
        app_version: "1",
    };
    let other = db
        .phones_pair(&dsp, "other", "key-2", &pairing, 7_000)
        .unwrap();
    let foreign = db.phones_file_arrived(&dsp, &other.id, &first[0].files[1], "sha", 100);
    assert_eq!(foreign.unwrap_err().code.as_str(), "phones_file_not_found");
}

#[test]
fn how_a_text_went_only_moves_on() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let owner = testing::a_user(&db);
    // The attempt the phone was given for a text waiting for it.
    let attempt_of = |id: &str| {
        let outbox = db.phones_outbox(&dsp, &phone.id).unwrap();
        let found = outbox.into_iter().find(|(text, _)| text.id == id);
        found.expect("the text waits for the phone").1
    };
    let report = |id: &str, attempt: i64, status| StatusReport {
        id: id.into(),
        attempt,
        status,
        at: 9_000,
        error: Some("no signal".into()),
    };
    let status = |id: &str, attempt: i64, status| {
        db.phones_status(&dsp, &phone.id, &report(id, attempt, status))
            .unwrap()
    };
    let text = db
        .phones_send(&dsp, &outgoing("(425) 555-0199", vec![], &owner), 8_000)
        .unwrap();
    assert_eq!(text.status, Some(Status::Waiting));
    let first = attempt_of(&text.id);
    assert!(status(&text.id, first, Status::Sent));
    assert!(
        db.phones_outbox(&dsp, &phone.id).unwrap().is_empty(),
        "sent leaves the outbox"
    );
    assert!(
        !status(&text.id, first, Status::Sent),
        "nothing moves twice"
    );
    assert!(status(&text.id, first, Status::Delivered));
    assert!(
        !status(&text.id, first, Status::Failed),
        "delivered stays delivered"
    );
    let kept = db.phones_text(&dsp, &text.id).unwrap().unwrap();
    assert_eq!((kept.status, kept.error), (Some(Status::Delivered), None));

    let failing = db
        .phones_send(&dsp, &outgoing("4255550199", vec![], &owner), 8_000)
        .unwrap();
    let before = attempt_of(&failing.id);
    assert!(status(&failing.id, before, Status::Failed));
    assert!(
        !status(&failing.id, before, Status::Sent),
        "a failure isn't undone by sent"
    );
    let failed = db.phones_text(&dsp, &failing.id).unwrap().unwrap();
    assert_eq!(failed.error.as_deref(), Some("no signal"));
    // Tried again in the same millisecond, it waits for the phone as a new attempt.
    let retried = db.phones_retry(&dsp, &failing.id, 8_000).unwrap();
    assert_eq!(
        (retried.status, retried.error),
        (Some(Status::Waiting), None)
    );
    let again = attempt_of(&failing.id);
    assert_ne!(again, before);
    // The phone repeating the first attempt's failure leaves the new attempt waiting.
    assert!(!status(&failing.id, before, Status::Failed));
    assert_eq!(db.phones_outbox(&dsp, &phone.id).unwrap().len(), 1);
    assert!(
        status(&failing.id, before, Status::Delivered),
        "a delivery report counts, whichever attempt"
    );
    // A phone says nothing of another's texts.
    let other = db
        .phones_send(&dsp, &outgoing("4255550198", vec![], &owner), 8_000)
        .unwrap();
    let theirs = attempt_of(&other.id);
    let foreign = report(&other.id, theirs, Status::Sent);
    assert!(!db.phones_status(&dsp, "phone_other", &foreign).unwrap());
    // With no phone paired, a text isn't tried again: no phone would send it.
    assert!(status(&other.id, theirs, Status::Failed));
    db.phones_remove(&dsp, &phone.id, 11_000).unwrap();
    let orphan = db.phones_retry(&dsp, &other.id, 12_000);
    assert_eq!(orphan.unwrap_err().code.as_str(), "phones_none");
}

#[test]
fn a_text_goes_from_the_phone_that_last_texted_its_number_one_online_first() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let none = db.phones_send(&dsp, &outgoing("4255550199", vec![], &owner), 1_000);
    assert_eq!(none.unwrap_err().code.as_str(), "phones_none");
    let first = paired(&db, &dsp, "key-1", "2065550100");
    db.phones_new_pairing(&dsp, &owner, "second", 3_000)
        .unwrap();
    let pairing = Pairing {
        number: "2065550177",
        model: "Galaxy",
        app_version: "1",
    };
    let second = db
        .phones_pair(&dsp, "second", "key-2", &pairing, 3_000)
        .unwrap();
    // Neither has texted the number: the one seen last.
    db.phones_seen(&dsp, &first.id, 9_000).unwrap();
    let to = |number: &str| {
        db.phones_send(&dsp, &outgoing(number, vec![], &owner), 10_000)
            .unwrap()
            .phone
    };
    assert_eq!(to("4255550199"), first.id);
    // Once the second hears from it, its replies go from the second.
    db.phones_report(
        &dsp,
        &second.id,
        vec![reported("x", "+14255550199", 11_000, 0)],
    )
    .unwrap();
    assert_eq!(to("(425) 555-0199"), second.id);
    // Offline, it gives way to one online.
    db.phones_seen(&dsp, &first.id, 200_000).unwrap();
    let online = db
        .phones_send(&dsp, &outgoing("4255550199", vec![], &owner), 200_000)
        .unwrap();
    assert_eq!(online.phone, first.id);
    // Failed, it's tried again as a new text goes: from the one online now.
    let failed = StatusReport {
        id: online.id.clone(),
        attempt: db
            .phones_outbox(&dsp, &first.id)
            .unwrap()
            .into_iter()
            .find(|(text, _)| text.id == online.id)
            .unwrap()
            .1,
        status: Status::Failed,
        at: 200_001,
        error: None,
    };
    assert!(db.phones_status(&dsp, &first.id, &failed).unwrap());
    db.phones_seen(&dsp, &second.id, 400_000).unwrap();
    let retried = db.phones_retry(&dsp, &online.id, 400_000).unwrap();
    assert_eq!(retried.phone, second.id);
    // The first had the earlier attempt: its delivery report of it still counts.
    let delivered = StatusReport {
        status: Status::Delivered,
        at: 400_001,
        ..failed
    };
    assert!(db.phones_status(&dsp, &first.id, &delivered).unwrap());
    let went = db.phones_text(&dsp, &online.id).unwrap().unwrap();
    assert_eq!(went.status, Some(Status::Delivered));
    // Pictures go only with the text of who added them, once.
    let file = ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: 10,
    };
    let picture = db
        .phones_add_file(&dsp, &owner, &file, "sha", 12_000)
        .unwrap();
    let someone = db.phones_send(
        &dsp,
        &outgoing("4255550199", vec![picture.id.clone()], "usr_other"),
        12_000,
    );
    assert_eq!(someone.unwrap_err().code.as_str(), "phones_file_not_found");
    let sent = db
        .phones_send(
            &dsp,
            &outgoing("4255550199", vec![picture.id.clone()], &owner),
            12_000,
        )
        .unwrap();
    assert_eq!(sent.files[0].id, picture.id);
    let twice = db.phones_send(
        &dsp,
        &outgoing("4255550199", vec![picture.id], &owner),
        12_000,
    );
    assert_eq!(twice.unwrap_err().code.as_str(), "phones_file_not_found");
}

#[test]
fn a_text_still_waiting_on_an_offline_phone_fails_as_it_expires_and_counts_if_it_went_after_all() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let owner = testing::a_user(&db);
    let queued = 10_000;
    let text = db
        .phones_send(&dsp, &outgoing("4255550199", vec![], &owner), queued)
        .unwrap();
    let attempt = db.phones_outbox(&dsp, &phone.id).unwrap()[0].1;
    let expired = |at| db.phones_expired(&dsp, at).unwrap();

    // The phone never checks in: once it expires, it fails as offline, and leaves the outbox.
    assert!(expired(queued + EXPIRY_MS - 1).is_empty());
    assert_eq!(
        db.phones_expire(&dsp, queued + EXPIRY_MS).unwrap(),
        std::slice::from_ref(&text.id)
    );
    let failed = db.phones_text(&dsp, &text.id).unwrap().unwrap();
    assert_eq!(
        (failed.status, failed.error.as_deref()),
        (Some(Status::Failed), Some(PHONE_OFFLINE))
    );
    assert!(db.phones_outbox(&dsp, &phone.id).unwrap().is_empty());
    // It had it before it went offline and sent it: that attempt counts after all.
    let sent = StatusReport {
        id: text.id.clone(),
        attempt,
        status: Status::Sent,
        at: queued + EXPIRY_MS + 1,
        error: None,
    };
    assert!(db.phones_status(&dsp, &phone.id, &sent).unwrap());
    let went = db.phones_text(&dsp, &text.id).unwrap().unwrap();
    assert_eq!((went.status, went.error), (Some(Status::Sent), None));

    // One whose phone is online as it expires waits for the phone to start it or say it expired;
    // once the phone is offline, it fails, though it checked in after the text was queued.
    let later = 100 * EXPIRY_MS;
    let waiting = db
        .phones_send(&dsp, &outgoing("4255550198", vec![], &owner), later)
        .unwrap();
    db.phones_seen(&dsp, &phone.id, later + EXPIRY_MS).unwrap();
    assert!(expired(later + EXPIRY_MS + ONLINE_MS).is_empty());
    assert_eq!(
        expired(later + EXPIRY_MS + ONLINE_MS + 1),
        std::slice::from_ref(&waiting.id)
    );
    // Failed and tried again, it expires anew from then.
    let failing = StatusReport {
        id: waiting.id.clone(),
        attempt: db.phones_outbox(&dsp, &phone.id).unwrap()[0].1,
        status: Status::Failed,
        at: later + 2,
        error: Some("no signal".into()),
    };
    assert!(db.phones_status(&dsp, &phone.id, &failing).unwrap());
    let retried = later + 3 * EXPIRY_MS;
    db.phones_retry(&dsp, &waiting.id, retried).unwrap();
    assert!(expired(retried + EXPIRY_MS - 1).is_empty());
    assert_eq!(expired(retried + EXPIRY_MS), [waiting.id]);
}

#[test]
fn a_reported_texts_time_is_kept_from_the_apps_look_back_before_pairing_to_clock_skew_after_its_report()
 {
    let (minute, day) = (60_000, 24 * 60 * 60 * 1000);
    let (paired, received) = (50 * day, 100 * day);
    // A day before the phone paired, as the app looks back that far; days old, as a phone
    // offline held it; or a few minutes ahead, as a clock may be: as the phone says.
    for at in [
        paired - day,
        received - 3 * day,
        received,
        received + 4 * minute,
    ] {
        assert_eq!(written(at, paired, received), at);
    }
    // Further off, as a clock set wrong: when its report came.
    for at in [0, paired - day - 1, received + 10 * minute] {
        assert_eq!(written(at, paired, received), received);
    }
}
