//! The texts' pictures, against a real store in a temporary directory: those added and never sent
//! are let go, and those sent stay; and a video its phone keeps comes only once someone plays it.
use super::*;
use crate::backend::phone::{
    model::FETCH_WAIT_MS,
    storage::{DeviceStore, TextStore, devices::ONLINE_MS},
    support::{outgoing, paired, reported, store},
};
use dispatch_core::testing;

#[test]
fn pictures_never_sent_are_let_go() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    paired(&db, &dsp, "key", "2065550100");
    let file = |name: &str| ReportedFile {
        name: name.into(),
        mime: "image/png".into(),
        size: 1,
        on_phone: false,
    };
    let sent = db
        .phones_add_file(&dsp, &owner, &file("sent.png"), "sha-sent", 1_000)
        .unwrap();
    db.phones_send(
        &dsp,
        &outgoing("4255550199", vec![sent.id.clone()], &owner),
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

#[test]
fn a_video_kept_on_its_phone_comes_only_while_someone_plays_it() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let mut text = reported("mms-1", "4255550199", 3_000, 1);
    text.files[0] = ReportedFile {
        name: "defect.mp4".into(),
        mime: "video/mp4".into(),
        size: 1_900_000,
        on_phone: true,
    };
    let video = db.phones_report(&dsp, &phone.id, vec![text]).unwrap()[0].files[0].clone();
    let file = |db: &Store| db.phones_file(&dsp, &video).unwrap().unwrap();
    assert!(file(&db).file.on_phone && !file(&db).file.kept);
    // Its first frame's picture is kept, the bytes upkeep keeps with it.
    db.phones_preview_arrived(&dsp, &phone.id, &video, "sha-frame")
        .unwrap();
    assert!(file(&db).file.preview);
    assert!(db.phones_bytes_kept(&dsp, "sha-frame").unwrap());
    // Asked for while its phone is offline: nothing waits for it.
    let seen = 2_000;
    let late = seen + ONLINE_MS + 1;
    assert_eq!(
        db.phones_want_file(&dsp, &video, late).unwrap(),
        Some(Wanting::Offline)
    );
    assert!(db.phones_wanted(&dsp, &phone.id, 0).unwrap().is_empty());
    // Online, it's asked for, and handed to its phone once each time it's asked: played again
    // while its phone is sending it, it waits on the same.
    db.phones_seen(&dsp, &phone.id, 10_000).unwrap();
    assert_eq!(
        db.phones_want_file(&dsp, &video, 10_000).unwrap(),
        Some(Wanting::Asked)
    );
    assert_eq!(
        db.phones_wanted(&dsp, &phone.id, 0).unwrap(),
        [video.as_str()]
    );
    assert!(
        db.phones_wanted(&dsp, &phone.id, 10_001)
            .unwrap()
            .is_empty()
    );
    db.phones_handed(&dsp, std::slice::from_ref(&video), 10_100)
        .unwrap();
    assert!(db.phones_wanted(&dsp, &phone.id, 0).unwrap().is_empty());
    db.phones_want_file(&dsp, &video, 10_200).unwrap();
    assert!(db.phones_wanted(&dsp, &phone.id, 0).unwrap().is_empty());
    let again = 10_100 + FETCH_WAIT_MS + 1;
    db.phones_want_file(&dsp, &video, again).unwrap();
    assert_eq!(
        db.phones_wanted(&dsp, &phone.id, 0).unwrap(),
        [video.as_str()]
    );
    // Its bytes come: kept, and asked for no more.
    db.phones_file_arrived(&dsp, &phone.id, &video, "sha-video", 1_900_000, 11_000)
        .unwrap();
    assert!(file(&db).file.kept);
    assert!(db.phones_wanted(&dsp, &phone.id, 0).unwrap().is_empty());
    assert_eq!(
        db.phones_want_file(&dsp, &video, 11_500).unwrap(),
        Some(Wanting::Kept)
    );
    // Let go of once they came before the time given, the text changing as they go.
    assert!(!db.phones_fetched_before(&dsp, 11_000).unwrap());
    let before = db
        .phones_text(&dsp, &file(&db).text_id.unwrap())
        .unwrap()
        .unwrap();
    assert!(db.phones_let_go_fetched(&dsp, 11_000).unwrap().is_empty());
    assert_eq!(
        db.phones_let_go_fetched(&dsp, 11_001).unwrap(),
        [before.id.as_str()]
    );
    let after = db.phones_text(&dsp, &before.id).unwrap().unwrap();
    assert!(after.change > before.change);
    assert!(!file(&db).file.kept && file(&db).file.on_phone);
    assert!(!db.phones_bytes_kept(&dsp, "sha-video").unwrap());
    assert!(db.phones_bytes_kept(&dsp, "sha-frame").unwrap());
    // Its phone has it no more: asking says so, and it's never handed again.
    db.phones_file_gone(&dsp, &phone.id, &video, 12_000)
        .unwrap();
    assert!(file(&db).file.gone);
    assert_eq!(
        db.phones_want_file(&dsp, &video, 12_100).unwrap(),
        Some(Wanting::Gone)
    );
    assert!(db.phones_wanted(&dsp, &phone.id, 0).unwrap().is_empty());
    // Until it sends something of it after all.
    db.phones_preview_arrived(&dsp, &phone.id, &video, "sha-frame")
        .unwrap();
    assert!(!file(&db).file.gone);
}

#[test]
fn only_its_own_phone_says_of_a_video_it_keeps() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let picture = db
        .phones_report(
            &dsp,
            &phone.id,
            vec![reported("mms-1", "4255550199", 3_000, 1)],
        )
        .unwrap()[0]
        .files[0]
        .clone();
    // A picture whose bytes it sends has no first frame, nor goes.
    let refused = |result: dispatch_core::Result<String>| {
        assert_eq!(result.unwrap_err().code.as_str(), "phones_file_not_found")
    };
    refused(db.phones_preview_arrived(&dsp, &phone.id, &picture, "sha"));
    refused(db.phones_file_gone(&dsp, &phone.id, &picture, 4_000));
    assert_eq!(db.phones_want_file(&dsp, &picture, 4_000).unwrap(), None);
}
