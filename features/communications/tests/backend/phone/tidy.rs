//! Upkeep lets go of each picture never sent on its own: one whose bytes can't go leaves the
//! rest to go, and goes the next time; bytes a later file of the same picture keeps stay, with
//! their preview, which goes with them; and bytes and previews left arriving go once abandoned.
//! Each job runs on its own: codes expired go though no bytes can be looked at.
use super::let_go;
use crate::{
    api::types::PairingStatus,
    backend::phone::{
        files::BYTES,
        storage::{DeviceStore, FileStore, ReportedFile},
        support::{CODE, kept, store},
    },
};
use dispatch_core::testing;
use std::time::{Duration, SystemTime};

fn picture() -> ReportedFile {
    ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: 4,
        on_phone: false,
    }
}

#[test]
fn a_picture_that_cannot_go_leaves_the_rest_to_go() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let folder = kept(&db, &dsp);
    // The first's bytes can't be removed: something else stands in their place.
    std::fs::create_dir(folder.join("blocked")).unwrap();
    std::fs::write(folder.join("loose"), b"door").unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "blocked", 1_000)
        .unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "loose", 1_000)
        .unwrap();
    let_go(&db, &dsp, 2_000);
    assert!(!folder.join("loose").exists(), "the rest still go");
    assert!(!db.phones_bytes_kept(&dsp, "blocked").unwrap());
    // Once nothing stands in their place, the next pass lets them go, though no file names them.
    std::fs::remove_dir(folder.join("blocked")).unwrap();
    std::fs::write(folder.join("blocked"), b"door").unwrap();
    let_go(&db, &dsp, 2_000);
    assert!(!folder.join("blocked").exists(), "tried again, they go");
}

#[test]
fn bytes_a_later_file_keeps_outlive_the_first_let_go() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let folder = kept(&db, &dsp);
    // The same picture, added long ago and never sent, and again now, and its preview.
    std::fs::write(folder.join("door-sha"), b"door").unwrap();
    let preview = BYTES
        .cache(&db, &dsp, "previews")
        .unwrap()
        .join("door-sha.jpg");
    std::fs::write(&preview, b"do").unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "door-sha", 1_000)
        .unwrap();
    db.phones_add_file(&dsp, &owner, &picture(), "door-sha", 5_000)
        .unwrap();
    let_go(&db, &dsp, 2_000);
    assert!(
        folder.join("door-sha").exists() && preview.exists(),
        "the later file keeps them"
    );
    // Once nothing keeps them, they go, with their preview.
    let_go(&db, &dsp, 6_000);
    assert!(!folder.join("door-sha").exists());
    assert!(!preview.exists());
}

#[test]
fn bytes_left_arriving_an_hour_go() {
    let (_root, db, dsp) = store();
    let folder = kept(&db, &dsp);
    // One upload and one preview a restart cut short an hour and more ago, and one of each under
    // way.
    let previews = BYTES.cache(&db, &dsp, "previews").unwrap();
    let cut = [folder.join(".arriving_cut"), previews.join(".preview_cut")];
    let arriving = [folder.join(".arriving_now"), previews.join(".preview_now")];
    let long_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    for path in cut.iter().chain(&arriving) {
        std::fs::write(path, b"do").unwrap();
    }
    for path in &cut {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
    }
    let_go(&db, &dsp, 2_000);
    assert!(cut.iter().all(|path| !path.exists()), "abandoned, they go");
    assert!(
        arriving.iter().all(|path| path.exists()),
        "still arriving, they stay"
    );
}

#[test]
fn codes_expired_go_though_no_bytes_can_be_looked_at() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    let code = db.phones_new_pairing(&dsp, &owner, CODE, 1_000).unwrap();
    // Something else stands where the bytes are kept, so the folder can't be read.
    let folder = kept(&db, &dsp);
    std::fs::remove_dir(&folder).unwrap();
    std::fs::write(&folder, b"").unwrap();
    let_go(&db, &dsp, 2_000);
    // Asked how it stood while it still worked, a code kept is waiting; one let go, expired.
    let status = db.phones_pairing(&dsp, &code, 1_000).unwrap();
    assert_eq!(status, PairingStatus::Expired, "the code goes anyway");
}
