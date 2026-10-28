//! Slack's photos and files, as upkeep tidies them each hour: a photo added a day ago and never
//! sent goes with its bytes, and bytes a photo keeps stay; bytes no file keeps go, as a failure
//! once in place or a removal that failed leave them; and bytes left arriving go once abandoned an
//! hour, as a restart leaves them, those under way staying.
use super::*;
use crate::backend::support;
use dispatch_core::testing;
use std::time::SystemTime;

const TEAM: &str = "T0FIXTURE1";

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
