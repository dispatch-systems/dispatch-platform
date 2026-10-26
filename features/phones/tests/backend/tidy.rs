//! Upkeep lets go of each picture never sent on its own: one whose bytes can't go leaves the
//! rest to go, and bytes a later file of the same picture keeps stay.
use super::let_go;
use crate::backend::{
    storage::{FileStore, ReportedFile},
    support::kept,
};
use dispatch_core::testing;

fn picture() -> ReportedFile {
    ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: 4,
    }
}

#[test]
fn a_picture_that_cannot_go_leaves_the_rest_to_go() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    let folder = kept(&db, &dsp);
    // The first's bytes can't be removed: something else stands in their place.
    std::fs::create_dir(folder.join("blocked")).unwrap();
    std::fs::write(folder.join("loose"), b"door").unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "blocked", 1_000)
        .unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "loose", 1_000)
        .unwrap();
    let_go(&db, &dsp, 2_000).unwrap();
    assert!(!folder.join("loose").exists(), "the rest still go");
    assert!(!db.phones_bytes_kept(&dsp, "blocked").unwrap());
}

#[test]
fn bytes_a_later_file_keeps_outlive_the_first_let_go() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    let folder = kept(&db, &dsp);
    // The same picture, added long ago and never sent, and again now.
    std::fs::write(folder.join("door-sha"), b"door").unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "door-sha", 1_000)
        .unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "door-sha", 5_000)
        .unwrap();
    let_go(&db, &dsp, 2_000).unwrap();
    assert!(
        folder.join("door-sha").exists(),
        "the later file keeps them"
    );
    // Once nothing keeps them, they go.
    let_go(&db, &dsp, 6_000).unwrap();
    assert!(!folder.join("door-sha").exists());
}
