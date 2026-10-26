//! The texts' pictures, against a real store in a temporary directory: those added and never sent
//! are let go, and those sent stay.
use super::*;
use crate::backend::{
    storage::TextStore,
    support::{outgoing, paired, store},
};
use dispatch_core::testing;
use dispatch_phones_contract::Outgoing;

#[test]
fn pictures_never_sent_are_let_go() {
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
    db.phones_tidy_files(&dsp, 2_000).unwrap();
    assert!(db.phones_file(&dsp, &unsent.id).unwrap().is_none());
    assert!(db.phones_file(&dsp, &sent.id).unwrap().is_some());
    assert!(db.phones_file(&dsp, &recent.id).unwrap().is_some());
    assert!(!db.phones_bytes_kept(&dsp, "sha-unsent").unwrap());
    assert!(db.phones_bytes_kept(&dsp, "sha-sent").unwrap());
}
