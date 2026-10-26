//! Bytes take their place and are let go only with the files that keep them, so letting go of
//! one file never removes the bytes another, the same picture, keeps.
use super::{Arrived, folder, forget};
use crate::backend::{
    storage::{PhonesStore, ReportedFile},
    tidy::let_go,
};
use dispatch_core::testing;

#[test]
fn bytes_another_file_keeps_outlive_the_first_let_go() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    let kept = folder(&db, &dsp).unwrap();
    let file = ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: 4,
    };
    // The same picture arrives twice: once long ago, never sent, and again now.
    let arrive = |name: &str| {
        let arriving = kept.join(name);
        std::fs::write(&arriving, b"door").unwrap();
        Arrived {
            sha256: "door-sha".into(),
            size: 4,
            arriving,
        }
    };
    let old = arrive(".arriving-old");
    old.place(&db, &dsp).unwrap();
    db.phones_add_file(&dsp, &owner, &file, &old.sha256, 1_000)
        .unwrap();
    let new = arrive(".arriving-new");
    new.place(&db, &dsp).unwrap();
    db.phones_add_file(&dsp, &owner, &file, &new.sha256, 5_000)
        .unwrap();
    // Letting go of the old one leaves the bytes the new one keeps.
    let_go(&db, &dsp, 2_000).unwrap();
    assert!(kept.join("door-sha").exists());
    assert!(db.phones_bytes_kept(&dsp, "door-sha").unwrap());
    // Once nothing keeps them, they go.
    let_go(&db, &dsp, 6_000).unwrap();
    assert!(!kept.join("door-sha").exists());
    forget(&db, &dsp, "door-sha").unwrap();
}
