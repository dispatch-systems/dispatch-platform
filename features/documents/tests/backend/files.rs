//! A DSP disabled while Google renews its access token, whose Drive is reached no more; and a
//! deleted DSP's token and pictures, forgotten, with none kept of a renewal or a folder shown as
//! it was deleted.
use super::*;
use crate::backend::{
    connection,
    description::Described,
    drive::DOC,
    files::make,
    google::{self, Google},
    storage::DocumentsStore,
    support::{ACCOUNT, connect, disabling, platform},
    tree::{Tree, tests::item},
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

#[tokio::test]
async fn an_unchanged_folder_renews_thumbnail_admission_without_renewing_its_provider_observation()
{
    use crate::backend::{files::folder_read, mirror, support::context};
    use dispatch_core::server::http::route::Dsp;
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let c = context(&state, &dsp).await;
    let at = dsp.clone();
    let connection = state
        .read(move |db| Ok(db.documents_connection(&at)?.unwrap()))
        .await
        .unwrap();
    let given = Instant::now() - PICTURE_LINK_LASTS - Duration::from_secs(10);
    let mut target = item("target", "Plan.pdf", None, false, "2026-11-01");
    target.thumbnail_link = Some("fixture:target".into());
    target.thumbnail_version = Some("7".into());
    target.seen = given;
    let shortcut = crate::backend::drive::Item {
        mime_type: crate::backend::drive::SHORTCUT.into(),
        shortcut_details: Some(crate::backend::drive::Shortcut {
            target_id: "target".into(),
            target_mime_type: "application/pdf".into(),
        }),
        ..item(
            "shortcut",
            "Plan",
            Some(&connection.folder_id),
            false,
            "2026-11-01",
        )
    };
    mirror::of(&dsp).replace(&connection.folder_id, vec![shortcut, target], "0".into());
    let first = folder_read(&state, c.clone(), Dsp("documents.use"), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        first.folder.as_ref().unwrap().items[0].thumbnail.as_deref(),
        Some("/api/dsp/documents/items/target/thumbnail?v=7")
    );
    // Expire shown admission using the cache's actual std::Instant, with no sleep or timer.
    PICTURES
        .lock()
        .unwrap()
        .get_mut(&(dsp.clone(), "target".into()))
        .unwrap()
        .shown = Instant::now() - PICTURE_LINK_LASTS;
    assert!(shown(&dsp, "target").is_none());
    let again = folder_read(
        &state,
        c,
        Dsp("documents.use"),
        None,
        None,
        Some(first.revision.clone()),
    )
    .await
    .unwrap();
    assert!(again.folder.is_none());
    assert_eq!(again.revision, first.revision);
    let renewed = shown(&dsp, "target").expect("unchanged must renew the shown target");
    assert_eq!(renewed.given, given);
    assert!(shown(&dsp, "shortcut").is_none());
}
