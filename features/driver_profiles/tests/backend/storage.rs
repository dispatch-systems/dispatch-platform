//! The roster and its photos, against a real store in a temporary directory; and photos staged
//! as their DSP is deleted, which make nothing of its folder again.
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

// Where Amazon served no photo, an offboarded associate's address isn't asked for again for a
// month; anyone else's is asked for at every collection. A photo kept for it later, or the
// roster no longer listing whoever it names, forgets it.
#[test]
fn an_address_amazon_serves_no_photo_at_waits_a_month_for_the_offboarded_alone() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    let address = |id: &str| format!("/workforce/api/v1/fetchDAPhoto?associate={id}");
    let none = ["driver-1", "driver-9", "driver-10"].map(address);
    let mut first = roster(&store, &dsp);
    first.photos.retain(|photo| !none.contains(&photo.address));
    first.photos_unserved = none.to_vec();
    publish(&store, &dsp, "job-1", first);
    let unserved = || store.kept_photos(&dsp, "company-fixture").unwrap().unserved;
    assert_eq!(unserved(), ["driver-9", "driver-10"].map(address).into());
    // A month on, an offboarded associate's is asked for again.
    let db = store.driver_profiles_db(&dsp).unwrap();
    db.exec(
        "UPDATE unserved_photos SET checked_at='2026-01-01T00:00:00.000Z' \
         WHERE address LIKE '%driver-9'",
        [],
    )
    .unwrap();
    drop(db);
    assert_eq!(unserved(), [address("driver-10")].into());
    // Amazon serves driver-10's photo after all, and stops listing driver-1.
    let mut next = roster(&store, &dsp);
    next.photos
        .retain(|photo| photo.address == address("driver-10"));
    next.associates.retain(|a| a.transporter_id != "driver-1");
    publish(&store, &dsp, "job-2", next);
    let db = store.driver_profiles_db(&dsp).unwrap();
    let (left,): (String,) = db
        .one_as("SELECT address FROM unserved_photos", [])
        .unwrap()
        .unwrap();
    assert_eq!(left, address("driver-9"));
    assert_eq!(
        db.count("SELECT count(*) FROM unserved_photos", [])
            .unwrap(),
        1
    );
    // A photo whose file a restore left out is asked for again: Amazon serving nothing there
    // then waits the month as well, though its row is still there.
    let (sha256,): (String,) = db
        .one_as(
            "SELECT sha256 FROM photos WHERE address LIKE '%driver-11'",
            [],
        )
        .unwrap()
        .unwrap();
    drop(db);
    std::fs::remove_file(
        photos::dir(&store, &dsp)
            .unwrap()
            .join(format!("{sha256}.png")),
    )
    .unwrap();
    let mut again = roster(&store, &dsp);
    again.photos.clear();
    again.photos_unserved = vec![address("driver-11")];
    publish(&store, &dsp, "job-3", again);
    assert!(unserved().contains(&address("driver-11")));
}

#[tokio::test]
async fn photos_staged_as_their_dsp_is_deleted_make_nothing_of_its_folder_again() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, _) = testing::bootstrapped();
    let owner = testing::platform_owner(&store);
    let dsp = store
        .new_dsp("Deleted DSP", "UTC", &owner, false)
        .unwrap()
        .id;
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    // A collection stages its photos: it has found where they go, and writes them later.
    let mut capture = roster(&store, &dsp);
    let dir = photos::dir(&store, &dsp).unwrap();
    // The DSP is disabled and deleted meanwhile, its folder with it.
    testing::set_dsp_status(&store, &dsp, "suspended").unwrap();
    store.delete_dsp(&dsp, "Deleted DSP", &owner).unwrap();
    let state = dispatch_core::State::new(store.config.clone()).unwrap();
    dispatch_core::tenancy::deletion::finish(&state).await;
    let folder = store.dsp_folder(&dsp).unwrap();
    assert!(!folder.try_exists().unwrap());
    // Written once it's gone, the photos make nothing of its folder again.
    let written = photos::write(&dir, &mut capture.photos);
    assert_eq!(written.unwrap_err().code, "dsp_not_found");
    assert!(!folder.try_exists().unwrap());
}
