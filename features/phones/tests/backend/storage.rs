//! Phones' storage, against a real store in a temporary directory: pairing, the texts a phone
//! reports, how Dispatch's texts go, the order readers find changes in, and letting go.
use super::*;
use dispatch_core::testing;
use dispatch_phones_contract::{Direction, Origin, Outgoing, Status};

const CODE: &str = "code-sha";

fn store() -> (tempfile::TempDir, Store, String) {
    testing::install(&[], &[&crate::FEATURE]);
    testing::bootstrapped()
}
/// A phone paired by a code the DSP's owner made, with the key `key`.
fn paired(db: &Store, dsp: &str, key: &str, number: &str) -> Device {
    let owner = testing::a_user(db);
    db.phones_new_pairing(dsp, &owner, CODE, 1_000).unwrap();
    let pairing = Pairing {
        number,
        model: "Pixel 8a",
        app_version: "1.0.0",
    };
    db.phones_pair(dsp, CODE, key, &pairing, 2_000).unwrap()
}
fn reported(id: &str, number: &str, at: i64, files: usize) -> Reported {
    Reported {
        client_id: id.into(),
        direction: Direction::In,
        number: number.into(),
        body: format!("text {id}"),
        at,
        files: (0..files)
            .map(|index| ReportedFile {
                name: format!("photo{index}.jpg"),
                mime: "image/jpeg".into(),
                size: 100,
            })
            .collect(),
    }
}
fn outgoing(to: &str, files: Vec<String>, by: &str) -> Outgoing {
    Outgoing {
        phone: None,
        to: to.into(),
        body: "On my way".into(),
        files,
        by: by.into(),
    }
}

#[test]
fn a_code_pairs_one_phone_once_while_it_lasts() {
    let (_root, db, dsp) = store();
    let first = paired(&db, &dsp, "key-1", "(206) 555-0100");
    assert_eq!(first.name, "Dispatch");
    assert_eq!(first.number, "+12065550100");
    assert_eq!(db.phones_device_by_key(&dsp, "key-1").unwrap(), Some(first));
    let pairing = Pairing {
        number: "",
        model: "Galaxy A35",
        app_version: "1.0.0",
    };
    // Used once, the code pairs nothing more.
    let again = db.phones_pair(&dsp, CODE, "key-2", &pairing, 3_000);
    assert_eq!(again.unwrap_err().code.as_str(), "phones_pairing_invalid");
    // A new code replaces one not used yet, and expires.
    let owner = testing::a_user(&db);
    db.phones_new_pairing(&dsp, &owner, "old", 10_000).unwrap();
    db.phones_new_pairing(&dsp, &owner, "new", 10_000).unwrap();
    let old = db.phones_pair(&dsp, "old", "key-2", &pairing, 11_000);
    assert_eq!(old.unwrap_err().code.as_str(), "phones_pairing_invalid");
    let late = db.phones_pair(&dsp, "new", "key-2", &pairing, 10_000 + PAIRING_MS);
    assert_eq!(late.unwrap_err().code.as_str(), "phones_pairing_invalid");
    db.phones_new_pairing(&dsp, &owner, "third", 20_000)
        .unwrap();
    let second = db
        .phones_pair(&dsp, "third", "key-2", &pairing, 21_000)
        .unwrap();
    assert_eq!(second.name, "Phone 2");
    // Removed, a phone's key works no more.
    db.phones_remove(&dsp, &second.id, 22_000).unwrap();
    assert_eq!(db.phones_device_by_key(&dsp, "key-2").unwrap(), None);
    assert!(db.phones_key_removed(&dsp, "key-2").unwrap());
}

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
    let changes = db.phones_changes(&dsp, 0, 100).unwrap();
    let ids: Vec<_> = changes.texts.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, [first[0].id.as_str(), again[1].id.as_str()]);
    let text = &changes.texts[0];
    assert_eq!(
        (text.direction, text.origin),
        (Direction::In, Origin::Phone)
    );
    assert!(text.files.iter().all(|file| !file.kept), "bytes come after");
    // Its picture's bytes arrive: the text changes again, after the other.
    db.phones_file_arrived(&dsp, &phone.id, &first[0].files[0], "sha")
        .unwrap();
    let later = db.phones_changes(&dsp, changes.last, 100).unwrap();
    assert_eq!(later.texts.len(), 1);
    assert!(later.texts[0].files[0].kept);
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
    let foreign = db.phones_file_arrived(&dsp, &other.id, &first[0].files[1], "sha");
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
    // A text whose phone was removed isn't tried again: no phone would send it.
    assert!(status(&other.id, theirs, Status::Failed));
    db.phones_remove(&dsp, &phone.id, 11_000).unwrap();
    let orphan = db.phones_retry(&dsp, &other.id, 12_000);
    assert_eq!(orphan.unwrap_err().code.as_str(), "phones_text_not_found");
}

#[test]
fn a_text_goes_from_the_phone_that_last_texted_its_number() {
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
fn pictures_never_sent_and_codes_expired_are_let_go() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let phone = paired(&db, &dsp, "key", "2065550100");
    let file = |name: &str| ReportedFile {
        name: name.into(),
        mime: "image/png".into(),
        size: 1,
    };
    let sent = db
        .phones_add_file(&dsp, &owner, &file("sent.png"), "sha-sent", 1_000)
        .unwrap();
    db.phones_send(
        &dsp,
        &Outgoing {
            phone: Some(phone.id),
            ..outgoing("4255550199", vec![sent.id.clone()], &owner)
        },
        1_000,
    )
    .unwrap();
    let unsent = db
        .phones_add_file(&dsp, &owner, &file("unsent.png"), "sha-unsent", 1_000)
        .unwrap();
    let recent = db
        .phones_add_file(&dsp, &owner, &file("recent.png"), "sha-recent", 5_000)
        .unwrap();
    assert_eq!(db.phones_tidy(&dsp, 2_000).unwrap(), ["sha-unsent"]);
    assert!(db.phones_file(&dsp, &unsent.id).unwrap().is_none());
    assert!(db.phones_file(&dsp, &sent.id).unwrap().is_some());
    assert!(db.phones_file(&dsp, &recent.id).unwrap().is_some());
    assert!(!db.phones_bytes_kept(&dsp, "sha-unsent").unwrap());
    assert!(db.phones_bytes_kept(&dsp, "sha-sent").unwrap());
}

#[test]
fn readers_page_through_every_change_in_order() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    for batch in 0..3 {
        let texts = (0..MOST_TEXTS)
            .map(|index| reported(&format!("{batch}-{index}"), "+14255550199", 1_000, 0))
            .collect();
        db.phones_report(&dsp, &phone.id, texts).unwrap();
    }
    db.phones_report(
        &dsp,
        &phone.id,
        vec![reported("last", "+14255550199", 2_000, 0)],
    )
    .unwrap();
    let (mut after, mut read) = (0, Vec::new());
    loop {
        let page = db.phones_changes(&dsp, after, MOST_TEXTS).unwrap();
        if page.texts.is_empty() {
            break;
        }
        read.extend(page.texts.iter().map(|text| text.change));
        after = page.last;
    }
    assert_eq!(read.len(), 3 * MOST_TEXTS + 1, "none missed between pages");
    assert!(read.windows(2).all(|pair| pair[0] < pair[1]));
}
