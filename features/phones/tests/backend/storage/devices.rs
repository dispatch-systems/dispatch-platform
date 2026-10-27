//! The DSP's phones, against a real store in a temporary directory: pairing by a code, once, while
//! it lasts, and a removed phone's key working no more, nor its texts still waiting.
use super::*;
use crate::backend::{
    storage::{StatusReport, TextStore},
    support::{CODE, outgoing, paired, store},
};
use dispatch_core::testing;
use dispatch_phones_contract::{PHONE_REMOVED, Status};

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
    let read = db.phones_changes(&dsp, 0).unwrap().next;
    db.phones_remove(&dsp, &phone.id, 5_000).unwrap();
    // The one waiting fails, a change its readers find; the one sent stays sent.
    let changed = db.phones_changes(&dsp, read).unwrap();
    let failed: Vec<_> = changed
        .texts
        .iter()
        .map(|text| (text.id.as_str(), text.status, text.error.as_deref()))
        .collect();
    assert_eq!(
        failed,
        [(
            waiting.id.as_str(),
            Some(Status::Failed),
            Some(PHONE_REMOVED)
        )]
    );
}
