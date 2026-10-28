//! Slack's photos and files: a member's whose bytes can't take their place is tried again, as a
//! download that failed is, until it's given up. As upkeep tidies them each hour: a photo added a
//! day ago and never sent goes with its bytes, and bytes a photo keeps stay; bytes no file keeps
//! go, as a failure once in place or a removal that failed leave them; and bytes left arriving go
//! once abandoned an hour, as a restart leaves them, those under way staying.
use super::*;
use crate::{backend::support, integrations::messages::incoming};
use dispatch_core::testing;
use std::time::SystemTime;

const TEAM: &str = "T0FIXTURE1";

/// Cameron Hayes shares Slack's photo of a flat tire with the app at `at`, as Slack tells of it:
/// downloaded at once.
async fn shared(state: &Arc<State>, dsp: &str, at: i64) {
    let event = json!({"type": "message", "subtype": "file_share", "channel_type": "im",
        "channel": "D0FIXCAM", "user": "U0FIXCAM", "text": "", "ts": incoming::ts(at),
        "files": [{"id": "F0FIXDMPHOTO", "name": "Flat tire.png", "mimetype": "image/png",
            "size": 69}]});
    incoming::heard(state, dsp, TEAM, &event).await.unwrap();
}

#[tokio::test]
async fn a_download_whose_bytes_cannot_take_their_place_is_tried_again_until_given_up() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    // The photo shared is kept at once; then something else stands where its bytes are kept.
    let start = now();
    shared(&state, &dsp, start).await;
    let at = dsp.clone();
    state
        .run(move |db| {
            let kept = "SELECT sha256 FROM communications_files WHERE status='kept'";
            let (sha256,): (String,) = db.dsp(&at)?.one_as(kept, [])?.unwrap();
            let bytes = support::files_kept(db, &at).join(sha256);
            std::fs::remove_file(&bytes)?;
            std::fs::create_dir(&bytes)?;
            Ok(())
        })
        .await
        .unwrap();
    // Shared again, its bytes come and can't take their place: a try that failed, held no more.
    shared(&state, &dsp, start + 1).await;
    let tried = || {
        let at = dsp.clone();
        state.read(move |db| {
            let tried = "SELECT status,attempts,reason,claimed_until FROM communications_files \
                 WHERE status!='kept'";
            let found: Option<(FileStatus, i64, Option<String>, Option<i64>)> =
                db.dsp(&at)?.one_as(tried, [])?;
            Ok(found.unwrap())
        })
    };
    let (status, attempts, reason, claimed) = tried().await.unwrap();
    assert_eq!((status, attempts, claimed), (FileStatus::Waiting, 1, None));
    assert!(reason.is_some(), "why it failed is kept");
    // Each time it's due again it fails again, until there's no more trying.
    for _ in RETRIES_MS {
        let at = dsp.clone();
        state
            .run(move |db| {
                let due = "UPDATE communications_files SET due_at=? WHERE status='waiting'";
                db.dsp(&at)?.exec(due, [now()]).map(drop)
            })
            .await
            .unwrap();
        download_due(&state, &dsp).await;
    }
    assert_eq!(tried().await.unwrap().0, FileStatus::Failed);
}

#[tokio::test]
async fn upkeep_lets_go_of_photos_never_sent_and_of_bytes_no_file_keeps_or_left_arriving() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let at = dsp.clone();
    let folder = state
        .run(move |db| {
            let folder = support::files_kept(db, &at);
            // A photo added two days ago and never sent, one added now, and bytes no file keeps.
            let owner = testing::a_user(db);
            for (sha256, bytes) in [("old-sha", b"gate"), ("door-sha", b"door")] {
                std::fs::write(folder.join(sha256), bytes)?;
                db.add_communications_upload(&at, &owner, "photo.png", "image/png", 4, sha256)?;
            }
            db.dsp(&at)?.exec(
                "UPDATE communications_files SET added_at=? WHERE sha256='old-sha'",
                [now() - 2 * UNSENT_MS],
            )?;
            std::fs::write(folder.join("loose-sha"), b"dock")?;
            Ok(folder)
        })
        .await
        .unwrap();
    // One upload a restart cut short an hour and more ago, and one under way.
    let (cut, arriving) = (folder.join(".arriving_cut"), folder.join(".arriving_now"));
    std::fs::write(&cut, b"do").unwrap();
    std::fs::write(&arriving, b"do").unwrap();
    let long_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&cut)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    (MAINTENANCE.run)(state.clone(), true).await;
    assert!(
        !folder.join("old-sha").exists(),
        "never sent, it goes with them"
    );
    assert!(folder.join("door-sha").exists(), "the photo keeps them");
    assert!(
        !folder.join("loose-sha").exists(),
        "nothing keeps them, they go"
    );
    assert!(!cut.exists(), "abandoned, they go");
    assert!(arriving.exists(), "still arriving, they stay");
}
