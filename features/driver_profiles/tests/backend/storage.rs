//! The roster and its photos, against a real store in a temporary directory.
use super::*;
use crate::backend::{
    photos,
    support::{publish, roster},
};
use dispatch_core::testing;

#[test]
fn a_photo_stays_while_amazon_lists_whoever_it_shows() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert_eq!(
        bind_roster_request(&store, &dsp).unwrap_err().code,
        "driver_profiles_abbreviation_required"
    );
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    publish(&store, &dsp, "job-1", roster(&store, &dsp));
    let dir = photos::dir(&store, &dsp).unwrap();
    let files = || std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(files(), 11);
    // Every photo stands, so the next collection downloads none of them.
    let kept = store.kept_photos(&dsp, "company-fixture").unwrap();
    assert_eq!((kept.standing.len(), kept.versions.len()), (11, 0));
    // Amazon stops listing two; the inactive and the offboarded it still lists keep theirs.
    let next = || {
        let mut next = roster(&store, &dsp);
        next.associates
            .retain(|a| !["driver-7", "driver-8"].contains(&a.transporter_id.as_str()));
        next.photos.clear();
        next
    };
    publish(&store, &dsp, "job-2", next());
    assert_eq!(files(), 9);
    let db = store.driver_profiles_db(&dsp).unwrap();
    let count = |sql: &str| db.count(sql, []).unwrap();
    assert_eq!(
        (
            count("SELECT count(*) FROM associates WHERE listed=1 AND status='ACTIVE'"),
            count("SELECT count(*) FROM associates WHERE listed=1 AND status<>'ACTIVE'"),
            count("SELECT count(*) FROM photos")
        ),
        (5, 4, 9)
    );
    // Whoever the roster no longer lists keeps their name, for what still names them.
    assert_eq!(count("SELECT count(*) FROM associates WHERE listed=0"), 2);
    // A month on, a listed associate's photo is due a refresh in case they changed it, by the
    // ETag it was served under; one kept with none is downloaded again. An offboarded
    // associate's stands for good, unless it is a JPEG or PNG kept before photos were shrunk and
    // weighs more than any shrunk one. A WebP, never shrunk, stands at any weight.
    db.exec("UPDATE photos SET stored_at='2026-01-01T00:00:00.000Z'", [])
        .unwrap();
    db.exec(
        "UPDATE photos SET etag=NULL WHERE address LIKE '%driver-6'",
        [],
    )
    .unwrap();
    db.exec(
        "UPDATE photos SET bytes=? WHERE address LIKE '%driver-9'",
        [MAX_KEPT_PHOTO_BYTES as i64 + 1],
    )
    .unwrap();
    db.exec(
        "UPDATE photos SET bytes=?, format='webp' WHERE address LIKE '%driver-10'",
        [MAX_KEPT_PHOTO_BYTES as i64 + 1],
    )
    .unwrap();
    let (webp,): (String,) = db
        .one_as(
            "SELECT sha256 FROM photos WHERE address LIKE '%driver-10'",
            [],
        )
        .unwrap()
        .unwrap();
    std::fs::rename(
        dir.join(format!("{webp}.png")),
        dir.join(format!("{webp}.webp")),
    )
    .unwrap();
    drop(db);
    let address = |id: &str| format!("/workforce/api/v1/fetchDAPhoto?associate={id}");
    let kept = store.kept_photos(&dsp, "company-fixture").unwrap();
    assert_eq!(
        kept.standing,
        ["driver-10", "driver-11"].map(address).into()
    );
    assert_eq!(
        kept.versions,
        (1..=5)
            .map(|n| (
                address(&format!("driver-{n}")),
                format!("\"fixture-driver-{n}\"")
            ))
            .collect()
    );
    // Amazon hadn't changed one: it stands a month more, as it was kept.
    let mut unchanged = next();
    unchanged.photos_unchanged = vec![address("driver-1")];
    publish(&store, &dsp, "job-3", unchanged);
    let kept = store.kept_photos(&dsp, "company-fixture").unwrap();
    assert!(kept.standing.contains(&address("driver-1")));
    assert_eq!(kept.versions.len(), 4);
}
