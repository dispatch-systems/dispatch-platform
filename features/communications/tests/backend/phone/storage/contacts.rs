//! Each phone's contacts, for a line that keeps its contacts on its phones, against a real store in
//! a temporary directory: names saved at the DSP waiting on each of the line's phones, handed and
//! saved; what a phone says of its own; the name the line shows; and pairing and removing phones.
use super::*;
use crate::backend::{
    phone::{
        storage::{DeviceStore, Line, Pairing},
        support::store,
    },
    texts::storage::ContactStore,
};
use dispatch_core::testing;

const DESK: Line = Line {
    id: "desk",
    name: "Desk",
};
const DANA: &str = "+14255550199";
const SAM: &str = "+14255550123";

/// A phone paired to the line `DESK`, by the key `key`, at `at`.
fn desk_phone(db: &Store, dsp: &str, key: &str, at: i64) -> String {
    let owner = testing::a_user(db);
    let code = format!("code-{key}");
    db.phones_new_pairing(dsp, Some(DESK), &owner, &code, at)
        .unwrap();
    let pairing = Pairing {
        number: "",
        model: "Pixel 8a",
        app_version: "1.0.0",
    };
    db.phones_pair(dsp, &code, key, &pairing, at).unwrap().id
}
/// The names `DESK` shows, by number.
fn shown(db: &Store, dsp: &str) -> Vec<(String, String)> {
    db.communications_contacts(dsp, Some(DESK.id)).unwrap()
}
fn pair(number: &str, name: &str) -> (String, String) {
    (number.to_owned(), name.to_owned())
}

#[test]
fn a_name_saved_waits_on_each_phone_until_it_says_it_saved_it() {
    let (_root, db, dsp) = store();
    let none = db.phones_save_contact(&dsp, DESK.id, DANA, "Dana", 1_000);
    assert_eq!(none.unwrap_err().code.as_str(), "phones_none");
    let (first, second) = (
        desk_phone(&db, &dsp, "a", 1_000),
        desk_phone(&db, &dsp, "b", 1_000),
    );
    let (before, phones) = db
        .phones_save_contact(&dsp, DESK.id, DANA, "Dana", 5_000)
        .unwrap()
        .unwrap();
    assert_eq!((before, phones.len()), (None, 2));
    // Shown at once, and saved again as it is, nothing moves.
    assert_eq!(shown(&db, &dsp), [pair(DANA, "Dana")]);
    let again = db.phones_save_contact(&dsp, DESK.id, DANA, "Dana", 6_000);
    assert_eq!(again.unwrap(), None);
    // Handed, it waits `SAVE_WAIT_MS` before it's handed again.
    let savings = db.phones_contact_savings(&dsp, &first, 5_000).unwrap();
    let saving = Saving {
        number: DANA.into(),
        name: "Dana".into(),
        at: 5_000,
    };
    assert_eq!(savings, std::slice::from_ref(&saving));
    db.phones_contacts_handed(&dsp, &first, &savings, 7_000)
        .unwrap();
    assert!(
        db.phones_contact_savings(&dsp, &first, 6_999)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.phones_contact_savings(&dsp, &first, 7_000)
            .unwrap()
            .len(),
        1
    );
    // Saved anew before the phone said it saved the first, the first's word counts for nothing.
    db.phones_save_contact(&dsp, DESK.id, DANA, "Dana Reyes", 8_000)
        .unwrap();
    let late = ContactsReport {
        saved: vec![(DANA.into(), 5_000)],
        ..ContactsReport::default()
    };
    db.phones_contacts_reported(&dsp, &first, &late, 9_000)
        .unwrap();
    let waiting = db.phones_contact_savings(&dsp, &first, 9_000).unwrap();
    assert_eq!(waiting[0].name, "Dana Reyes", "handed again, at once");
    // Said saved, it waits no more, on that phone alone.
    let saved = ContactsReport {
        saved: vec![(DANA.into(), 8_000)],
        ..ContactsReport::default()
    };
    let moved = db
        .phones_contacts_reported(&dsp, &first, &saved, 9_000)
        .unwrap();
    assert!(moved.is_empty(), "its name didn't change");
    assert!(
        db.phones_contact_savings(&dsp, &first, 9_000)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.phones_contact_savings(&dsp, &second, 9_000)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(shown(&db, &dsp), [pair(DANA, "Dana Reyes")]);
}

#[test]
fn what_a_phone_says_of_its_contacts_is_what_the_line_shows() {
    let (_root, db, dsp) = store();
    let (first, second) = (
        desk_phone(&db, &dsp, "a", 1_000),
        desk_phone(&db, &dsp, "b", 1_000),
    );
    let report = |changed: &[(&str, &str)], removed: &[&str]| ContactsReport {
        saved: Vec::new(),
        changed: changed
            .iter()
            .map(|(number, name)| pair(number, name))
            .collect(),
        removed: removed.iter().map(|number| (*number).to_owned()).collect(),
    };
    let moved = db
        .phones_contacts_reported(
            &dsp,
            &first,
            &report(&[(DANA, "Dana"), (SAM, "Sam")], &[]),
            2_000,
        )
        .unwrap();
    assert_eq!(moved, [DANA, SAM]);
    assert_eq!(shown(&db, &dsp), [pair(SAM, "Sam"), pair(DANA, "Dana")]);
    // Said again as it was, nothing moves.
    let same = db
        .phones_contacts_reported(&dsp, &first, &report(&[(DANA, "Dana")], &[]), 3_000)
        .unwrap();
    assert!(same.is_empty());
    // Of two phones, the name changed last shows.
    db.phones_contacts_reported(&dsp, &second, &report(&[(DANA, "Dana R")], &[]), 4_000)
        .unwrap();
    assert_eq!(shown(&db, &dsp)[1], pair(DANA, "Dana R"));
    // Its phone's name over one saved at the DSP that it never saved.
    db.phones_save_contact(&dsp, DESK.id, SAM, "Samuel", 5_000)
        .unwrap();
    db.phones_contacts_reported(&dsp, &first, &report(&[(SAM, "Sam O")], &[]), 6_000)
        .unwrap();
    assert!(
        db.phones_contact_savings(&dsp, &first, 6_000)
            .unwrap()
            .is_empty()
    );
    // Removed from both, it shows no more; removed again, nothing moves.
    db.phones_contacts_reported(&dsp, &second, &report(&[], &[DANA, SAM]), 7_000)
        .unwrap();
    assert_eq!(shown(&db, &dsp)[0], pair(SAM, "Sam O"));
    db.phones_contacts_reported(&dsp, &first, &report(&[], &[DANA]), 8_000)
        .unwrap();
    let again = db
        .phones_contacts_reported(&dsp, &first, &report(&[], &[DANA]), 9_000)
        .unwrap();
    assert!(again.is_empty());
    assert_eq!(shown(&db, &dsp), [pair(SAM, "Sam O")]);
    // Dispatch Phone's own line shows none of it.
    assert!(db.communications_contacts(&dsp, None).unwrap().is_empty());
}

#[test]
fn a_phone_paired_is_handed_its_lines_names_and_one_removed_takes_its_own() {
    let (_root, db, dsp) = store();
    let first = desk_phone(&db, &dsp, "a", 1_000);
    let own = ContactsReport {
        changed: vec![pair(DANA, "Dana")],
        ..ContactsReport::default()
    };
    db.phones_contacts_reported(&dsp, &first, &own, 2_000)
        .unwrap();
    let second = desk_phone(&db, &dsp, "b", 3_000);
    let handed = db.phones_contact_savings(&dsp, &second, 3_000).unwrap();
    let saving = Saving {
        number: DANA.into(),
        name: "Dana".into(),
        at: 2_000,
    };
    assert_eq!(handed, [saving]);
    // A phone of Dispatch Phone's own line is handed nothing.
    let owner = testing::a_user(&db);
    db.phones_new_pairing(&dsp, None, &owner, "own", 3_000)
        .unwrap();
    let pairing = Pairing {
        number: "",
        model: "Pixel 8a",
        app_version: "1.0.0",
    };
    let theirs = db.phones_pair(&dsp, "own", "c", &pairing, 3_000).unwrap();
    assert!(
        db.phones_contact_savings(&dsp, &theirs.id, 3_000)
            .unwrap()
            .is_empty()
    );
    // Removing both, the line shows none of their names.
    db.phones_remove(&dsp, Some(DESK.id), &first, 4_000)
        .unwrap();
    assert_eq!(shown(&db, &dsp), [pair(DANA, "Dana")]);
    db.phones_remove(&dsp, Some(DESK.id), &second, 4_000)
        .unwrap();
    assert!(shown(&db, &dsp).is_empty());
}
