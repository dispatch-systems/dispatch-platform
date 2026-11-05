//! A delete cut short once the DSP's rows are gone holds nothing up: the server starts again
//! without the DSP, and its first upkeep finishes what the delete left.
use super::*;
use crate::testing;

#[tokio::test]
async fn a_delete_cut_short_is_finished_once_the_server_starts_again() {
    testing::install(&[], &[]);
    let (_root, db) = testing::seeded();
    let (id,): (String,) = db
        .platform
        .one_as("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let owner = testing::platform_owner(&db);
    testing::set_dsp_status(&db, &id, "suspended").unwrap();
    db.jobs
        .exec(
            "INSERT INTO live_days(reader,dsp_id,day,started_at) VALUES ('r',?,'2026-11-05',0)",
            [&id],
        )
        .unwrap();
    // The server stops once the DSP's rows are gone, before anything else of it is.
    db.delete_dsp(&id, "Northline Logistics", &owner).unwrap();
    let folder = db.dsp_folder(&id).unwrap();
    assert!(folder.is_dir());
    let config = db.config.clone();
    drop(db);

    let state = crate::State::new(config).unwrap();
    finish(&state).await;
    assert!(!folder.exists());
    let left = state
        .read(move |db| {
            Ok((
                db.platform
                    .count("SELECT count(*) FROM dsp_deletions", [])?,
                db.jobs
                    .count("SELECT count(*) FROM live_days WHERE dsp_id=?", [&id])?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(left, (0, 0));
}
