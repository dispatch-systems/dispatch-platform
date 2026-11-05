//! The DSP's phones, against a real store in a temporary directory: pairing by a code, once, while
//! it lasts; checking in, noted at most every half minute however its calls overlap, online
//! throughout; and a removed phone's key working no more, nor its texts still waiting.
use super::*;
use crate::backend::phone::{
    model::{PHONE_REMOVED, Status},
    seen,
    storage::{StatusReport, TextStore},
    support::{CODE, outgoing, paired, store},
};
use dispatch_core::testing;

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
    // A new code replaces one not used yet, and expires, as each says it stands.
    let owner = testing::a_user(&db);
    let old = db.phones_new_pairing(&dsp, &owner, "old", 10_000).unwrap();
    let new = db.phones_new_pairing(&dsp, &owner, "new", 10_000).unwrap();
    let status = |id: &str, at: i64| db.phones_pairing(&dsp, id, at).unwrap();
    assert_eq!(status(&old, 11_000), PairingStatus::Replaced);
    assert_eq!(status(&new, 11_000), PairingStatus::Waiting);
    let replaced = db.phones_pair(&dsp, "old", "key-2", &pairing, 11_000);
    assert_eq!(
        replaced.unwrap_err().code.as_str(),
        "phones_pairing_invalid"
    );
    let late = db.phones_pair(&dsp, "new", "key-2", &pairing, 10_000 + PAIRING_MS);
    assert_eq!(late.unwrap_err().code.as_str(), "phones_pairing_invalid");
    assert_eq!(status(&new, 10_000 + PAIRING_MS), PairingStatus::Expired);
    let third = db
        .phones_new_pairing(&dsp, &owner, "third", 20_000)
        .unwrap();
    let second = db
        .phones_pair(&dsp, "third", "key-2", &pairing, 21_000)
        .unwrap();
    assert_eq!(second.name, "Phone 2");
    assert_eq!(status(&third, 21_000), PairingStatus::Paired);
    assert_eq!(status("pairing_unknown", 21_000), PairingStatus::Expired);
    // Removed, a phone's key works no more.
    db.phones_remove(&dsp, &second.id, 22_000).unwrap();
    assert_eq!(db.phones_device_by_key(&dsp, "key-2").unwrap(), None);
    assert!(db.phones_key_removed(&dsp, "key-2").unwrap());
}

#[test]
fn a_phone_waiting_on_its_outbox_is_noted_at_most_every_half_minute_and_online_throughout() {
    let (_root, db, dsp) = store();
    let device = paired(&db, &dsp, "key", "2065550100");
    // As each check-in's write reads it.
    let phone = || db.phones_device(&dsp, &device.id).unwrap().unwrap();
    // It checks in as each twenty-second wait on its outbox ends, for ten minutes: noted only once
    // half a minute passed since it last was, and online until it checks in again.
    let waits: Vec<i64> = (1..=30).map(|n| device.paired_at + n * 20_000).collect();
    let mut noted = vec![device.paired_at];
    for &at in &waits {
        assert!(
            phone().online(at - 1),
            "online just before checking in at {at}"
        );
        seen(&db, &dsp, &phone(), at).unwrap();
        let last = phone().last_seen_at.unwrap();
        if last != *noted.last().unwrap() {
            noted.push(last);
        }
    }
    assert!(noted.windows(2).all(|two| two[1] - two[0] >= SEEN_MS));
    assert!(noted.len() < waits.len(), "fewer writes than check-ins");
    // Two calls of its alongside, as its outbox wait and a report of how texts went, each find it
    // due as they first read it; the second's write finds the first noted it, and notes nothing.
    let last = *noted.last().unwrap();
    let (first, second) = (last + SEEN_MS, last + SEEN_MS + 1);
    let read = phone();
    assert!(read.noting(first) && read.noting(second));
    seen(&db, &dsp, &phone(), first).unwrap();
    seen(&db, &dsp, &phone(), second).unwrap();
    assert_eq!(phone().last_seen_at, Some(first));
    // Once it stops, offline within two minutes of its last check-in, as the card says.
    assert!(phone().online(second + ONLINE_MS - SEEN_MS));
    assert!(!phone().online(first + ONLINE_MS + 1));
}

#[test]
fn a_phone_removed_fails_the_texts_still_waiting_on_it() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let phone = paired(&db, &dsp, "key", "2065550100");
    let waiting = db
        .phones_send(&dsp, &outgoing("4255550199", vec![], &owner), 3_000)
        .unwrap();
    let sent = db
        .phones_send(&dsp, &outgoing("4255550198", vec![], &owner), 3_000)
        .unwrap();
    let (_, attempt) = db
        .phones_outbox(&dsp, &phone.id)
        .unwrap()
        .into_iter()
        .find(|(text, _)| text.id == sent.id)
        .expect("it waits for the phone");
    let report = StatusReport {
        id: sent.id.clone(),
        attempt,
        status: Status::Sent,
        at: 4_000,
        error: None,
    };
    assert!(db.phones_status(&dsp, &phone.id, &report).unwrap());
    let (_, failed) = db.phones_remove(&dsp, &phone.id, 5_000).unwrap();
    // The one waiting fails, which removing it answers, for filing; the one sent stays sent.
    assert_eq!(failed, std::slice::from_ref(&waiting.id));
    let now = |id: &str| {
        let text = db.phones_text(&dsp, id).unwrap().unwrap();
        (text.status, text.error)
    };
    assert_eq!(
        now(&waiting.id),
        (Some(Status::Failed), Some(PHONE_REMOVED.to_owned()))
    );
    assert_eq!(now(&sent.id), (Some(Status::Sent), None));
}
