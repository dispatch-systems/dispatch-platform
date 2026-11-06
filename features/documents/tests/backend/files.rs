//! A DSP disabled while Google renews its access token, whose Drive is reached no more; and a
//! deleted DSP's token and pictures, forgotten, with none kept of a renewal or a folder shown as
//! it was deleted.
use super::*;
use crate::backend::{
    drive::DOC,
    google,
    support::{ACCOUNT, connect, disabling, platform},
    tree::tests::item,
};
use dispatch_core::{Code, testing};

#[tokio::test]
async fn a_dsp_disabled_while_its_token_is_renewed_makes_nothing_in_its_drive() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let at = dsp.clone();
    let (connection, refresh) = state
        .read(move |db| {
            Ok((
                db.documents_connection(&at)?,
                db.documents_refresh_token(&at)?,
            ))
        })
        .await
        .unwrap();
    let (folder, refresh) = (connection.unwrap().folder_id, refresh.unwrap());
    google::fixture_on_renewal(&refresh, disabling(&state, &dsp));
    let refused = make(&state, &dsp, &folder, DOC, "Made while disabled").await;
    assert!(refused.unwrap_err().is(Code::DspUnavailable));
    let google = Google::of(&state.config).unwrap();
    let made = google.inside(ACCOUNT, &folder).await.unwrap();
    assert!(!made.iter().any(|item| item.name == "Made while disabled"));
}

#[test]
fn a_deleted_dsps_token_and_pictures_are_forgotten_and_none_kept_after_and_no_other_dsps() {
    // Each DSP's access token, and what its Documents showed of a file.
    let lasts = Instant::now() + Duration::from_secs(3600);
    for dsp in ["dsp_deleted", "dsp_other"] {
        keep(dsp, &format!("token:{dsp}"), lasts);
        let now = Instant::now();
        let shown = Shown {
            link: format!("fixture:{dsp}"),
            given: now,
            shown: now,
        };
        PICTURES
            .lock()
            .unwrap()
            .insert((dsp.to_owned(), "file".to_owned()), shown);
    }
    // A folder of the deleted DSP's was read before it was deleted, and is shown once it's
    // forgotten; and Google renewed its token as it was deleted, which is kept once it is.
    let tree = Tree::new("main", Vec::new());
    let (records, names) = (BTreeMap::new(), BTreeMap::new());
    let listing = Described {
        dsp: "dsp_deleted",
        tree: &tree,
        records: &records,
        names: &names,
        account: "documents@example.test",
    };
    let mut listed = item("listed", "Listed.pdf", Some("main"), false, "2026-11-05");
    listed.thumbnail_link = Some("fixture:dsp_deleted:listed".into());
    connection::forgotten("dsp_deleted");
    listing.item(&Row::from(listed), false);
    keep("dsp_deleted", "token:renewed", lasts);
    assert!(kept("dsp_deleted").is_none());
    assert!(shown("dsp_deleted", "file").is_none());
    assert!(shown("dsp_deleted", "listed").is_none());
    assert_eq!(kept("dsp_other").unwrap(), "token:dsp_other");
    assert_eq!(
        shown("dsp_other", "file").unwrap().link,
        "fixture:dsp_other"
    );
}

#[tokio::test]
async fn a_delete_whose_request_went_still_forgets_the_dsps_token() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db) = testing::seeded();
    let dsp = testing::dsp_named(&db, "Northline Logistics");
    let owner = testing::platform_owner(&db);
    testing::set_dsp_status(&db, &dsp, "suspended").unwrap();
    keep(
        &dsp,
        "token:held",
        Instant::now() + Duration::from_secs(3600),
    );
    db.delete_dsp(&dsp, "Northline Logistics", &owner).unwrap();
    let config = db.config.clone();
    drop(db);
    // The request that deleted it went before finishing it; upkeep finishes it.
    let state = dispatch_core::State::new(config).unwrap();
    dispatch_core::tenancy::deletion::finish(&state).await;
    assert!(kept(&dsp).is_none());
}
