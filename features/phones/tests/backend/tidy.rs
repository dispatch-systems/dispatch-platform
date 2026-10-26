//! Upkeep lets go of each picture never sent on its own: one whose bytes can't go leaves the
//! rest to go.
use super::let_go;
use crate::backend::{
    files::folder,
    storage::{PhonesStore, ReportedFile},
};
use dispatch_core::testing;

#[test]
fn a_picture_that_cannot_go_leaves_the_rest_to_go() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    let file = ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: 4,
    };
    let kept = folder(&db, &dsp).unwrap();
    // The first's bytes can't be removed: something else stands in their place.
    std::fs::create_dir(kept.join("blocked")).unwrap();
    std::fs::write(kept.join("loose"), b"door").unwrap();
    db.phones_add_file(&dsp, &owner, &file, "blocked", 1_000)
        .unwrap();
    db.phones_add_file(&dsp, &owner, &file, "loose", 1_000)
        .unwrap();
    let_go(&db, &dsp, 2_000).unwrap();
    assert!(!kept.join("loose").exists(), "the rest still go");
    assert!(!db.phones_bytes_kept(&dsp, "blocked").unwrap());
}
