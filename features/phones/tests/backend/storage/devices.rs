//! The DSP's phones, against a real store in a temporary directory: pairing by a code, once, while
//! it lasts, and a removed phone's key working no more.
use super::*;
use crate::backend::support::{CODE, paired, store};
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
