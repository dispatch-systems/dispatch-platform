//! A DSP disabled while Google renews its access token, whose Drive is reached no more; and a
//! deleted DSP's pictures, forgotten, with none kept of a folder shown as it was deleted.
use super::*;
use crate::backend::{
    drive::DOC,
    google,
    support::{ACCOUNT, connect, disabling, platform},
    tree::tests::item,
};
use dispatch_core::Code;

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
fn a_deleted_dsps_pictures_are_forgotten_and_none_kept_after_and_no_other_dsps() {
    // What each DSP's Documents showed of a file.
    for dsp in ["dsp_deleted", "dsp_other"] {
        let now = Instant::now();
        let shown = Shown {
            link: format!("fixture:{dsp}"),
            given: now,
            shown: now,
        };
        PICTURES
            .lock()
            .unwrap()
            .shown
            .insert((dsp.to_owned(), "file".to_owned()), shown);
    }
    // A folder of the deleted DSP's was read before it was deleted, and is shown once it's
    // forgotten.
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
    assert!(shown("dsp_deleted", "file").is_none());
    assert!(shown("dsp_deleted", "listed").is_none());
    assert_eq!(
        shown("dsp_other", "file").unwrap().link,
        "fixture:dsp_other"
    );
}
