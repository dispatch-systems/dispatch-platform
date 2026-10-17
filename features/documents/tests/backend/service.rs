//! Documents keeping files for another feature through core's Files service, against fixture
//! mode's Drive: only while Documents is on and its account connected, only inside the DSP's
//! own folder, and named for whoever they came from; and pages written as Docs, and over them.
use super::*;
use crate::backend::{google::Google, storage::Connection, tree::Mime};
use axum::body::Bytes;
use dispatch_core::{
    db::iso,
    manifest::services::files::{self, Files, Page},
    server::http::upload::Upload,
    testing,
};
use futures_util::stream;
use serde_json::json;

/// A platform with one DSP, Documents on and not yet connected, and its server's state.
fn platform() -> (impl Drop, Arc<State>, String) {
    testing::install(&[], &[&crate::FEATURE]);
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), dsp)
}
/// Connects the DSP's Documents to fixture mode's Google, with a main folder of its own.
async fn connect(state: &Arc<State>, dsp: &str) {
    let google = Google::of(&state.config).unwrap();
    let granted = google
        .exchange(&state.config.origin, "fixture:documents@example.test", "-")
        .await
        .unwrap();
    let folder_id = google
        .create_folder(&granted.access, "Test Documents")
        .await
        .unwrap();
    let connection = Connection {
        broken: false,
        full: true,
        account: granted.account,
        folder_id,
        folder_name: "Test Documents".into(),
        connected_by: "user_1".into(),
        connected_at: iso(),
    };
    let (at, refresh) = (dsp.to_owned(), granted.refresh);
    state
        .run(move |db| db.save_documents_connection(&at, &connection, &refresh))
        .await
        .unwrap();
}
/// The file `name`, holding `text`, as it arrives from Slack, from `by`.
async fn incoming(name: &str, text: &'static str, by: Author) -> Incoming {
    let bytes = stream::iter([Ok(Bytes::from_static(text.as_bytes()))]);
    Incoming {
        name: name.into(),
        mime: "image/jpeg".into(),
        bytes: Upload::streamed(text.len() as u64, u64::MAX, bytes)
            .await
            .unwrap(),
        by,
        from: "Slack",
    }
}
/// The Files service for the DSP, as a feature using it finds it.
async fn offered(state: &Arc<State>, dsp: &str) -> Option<&'static dyn Files> {
    let at = dsp.to_owned();
    state.read(move |db| files::for_dsp(db, &at)).await.unwrap()
}

#[tokio::test]
async fn files_are_kept_inside_the_dsps_folder_once_connected_and_named_for_who_sent_them() {
    let (_root, state, dsp) = platform();
    let owner = state.read(|db| Ok(testing::a_user(db))).await.unwrap();
    let files = offered(&state, &dsp).await.expect("Documents is on");

    // Not connected yet, it says why it can't keep files.
    let unready = files.unready(&state, &dsp).await.unwrap();
    assert_eq!(unready.as_deref(), Some("Google Drive isn't connected"));
    connect(&state, &dsp).await;
    assert_eq!(files.unready(&state, &dsp).await.unwrap(), None);

    // A folder a member makes for them is at the top, found by its ID, and browsed into from
    // there; anything else isn't one.
    let member = Author::Member(owner);
    let folder = files
        .create_folder(&state, &dsp, "photos", &member)
        .await
        .unwrap();
    let found = files.folder(&state, &dsp, &folder.id).await.unwrap();
    assert_eq!(found, Some(folder.clone()));
    let top = files.browse(&state, &dsp, None).await.unwrap();
    assert_eq!(top.folders, std::slice::from_ref(&folder));
    let inside = files.browse(&state, &dsp, Some(&folder.id)).await.unwrap();
    assert_eq!(inside.path, [top.path[0].clone(), folder.clone()]);
    assert_eq!(files.folder(&state, &dsp, "elsewhere").await.unwrap(), None);
    let browsing = files.browse(&state, &dsp, Some("elsewhere")).await;
    assert_eq!(browsing.unwrap_err().code, "files_folder_not_found");

    // A file from someone known only by name goes into the folder, as theirs, logged with no
    // member as its actor.
    let dre = Author::Named("Dre Holloway".into());
    let file = incoming("van.jpg", "jpeg", dre.clone()).await;
    let saved = files.save(&state, &dsp, &folder.id, file).await.unwrap();
    let (_, tree) = crate::backend::files::tree(&state, &dsp).await.unwrap();
    let parent = tree.get(&saved.id).and_then(|row| row.parent.as_deref());
    assert_eq!(parent, Some(folder.id.as_str()));
    let at = dsp.clone();
    let (records, logged) = state
        .read(move |db| {
            let logged = testing::audits(db, Some(&at))?;
            Ok((db.documents_records(&at)?, logged))
        })
        .await
        .unwrap();
    assert_eq!(records[&saved.id].added_by, dre);
    let saving = logged
        .as_array()
        .into_iter()
        .flatten()
        .find(|event| event["action"] == "documents.saved")
        .expect("the save is logged");
    assert_eq!(saving["detail"], "van.jpg from Slack");
    assert!(saving["actorId"].is_null());

    // Nothing goes outside the DSP's folder.
    let file = incoming("van.jpg", "jpeg", dre).await;
    let refused = files
        .save(&state, &dsp, "elsewhere", file)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "files_folder_not_found");

    // Switched off for the DSP, Documents keeps no files for it.
    let at = dsp.clone();
    state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.set_feature(&at, "documents", false, &owner).map(drop)
        })
        .await
        .unwrap();
    assert!(offered(&state, &dsp).await.is_none());
}

/// The page `name`, holding `html`, as Slack writes a canvas, from `by`.
fn page(name: &str, html: &str, by: Author) -> Page {
    Page {
        name: name.into(),
        html: html.into(),
        by,
        from: "Slack",
    }
}

#[tokio::test]
async fn a_page_is_written_as_a_doc_in_the_dsps_folder_and_over_it_where_it_is() {
    let (_root, state, dsp) = platform();
    let files = offered(&state, &dsp).await.expect("Documents is on");
    connect(&state, &dsp).await;
    let owner = state.read(|db| Ok(testing::a_user(db))).await.unwrap();
    let folder = files
        .create_folder(&state, &dsp, "canvases", &Author::Member(owner.clone()))
        .await
        .unwrap();

    // Written, it's a Doc in the folder, added by who wrote it and logged as saved from Slack.
    let dre = Author::Named("Dre Holloway".into());
    let written = files
        .write(
            &state,
            &dsp,
            &folder.id,
            page("Write-ups", "<p>One</p>", dre.clone()),
        )
        .await
        .unwrap();
    let (_, tree) = crate::backend::files::tree(&state, &dsp).await.unwrap();
    let row = tree.get(&written.id).expect("the Doc is in Documents");
    assert_eq!(row.mime, Mime::Doc);
    assert_eq!(row.parent.as_deref(), Some(folder.id.as_str()));
    assert_eq!(&*row.name, "Write-ups");

    // Written over, it stays the same Doc where it is, named as the page is now, changed by
    // whoever changed the page last, and not logged again.
    let member = Author::Member(owner);
    let again = page("Write-ups, October", "<p>Two</p>", member.clone());
    let rewritten = files
        .rewrite(&state, &dsp, &written.id, again)
        .await
        .unwrap();
    let rewritten = rewritten.expect("the Doc is still there");
    assert_eq!(rewritten.id, written.id);
    assert_eq!(rewritten.name, "Write-ups, October");
    let at = dsp.clone();
    let (records, logged) = state
        .read(move |db| Ok((db.documents_records(&at)?, testing::audits(db, Some(&at))?)))
        .await
        .unwrap();
    assert_eq!(records[&written.id].added_by, dre);
    assert_eq!(records[&written.id].changed_by, member);
    let saves: Vec<_> = logged
        .as_array()
        .into_iter()
        .flatten()
        .filter(|event| event["action"] == "documents.saved")
        .map(|event| event["detail"].clone())
        .collect();
    assert_eq!(saves, [json!("Write-ups from Slack")]);

    // Only a Doc among the DSP's files is written over: a folder, or one elsewhere, is gone.
    for id in [folder.id.as_str(), "elsewhere"] {
        let over = page("Write-ups", "<p>Three</p>", dre.clone());
        assert!(
            files
                .rewrite(&state, &dsp, id, over)
                .await
                .unwrap()
                .is_none()
        );
    }
    // Nothing is written outside the DSP's folder, nor a page larger than Google takes.
    let outside = page("Write-ups", "<p>One</p>", dre.clone());
    let refused = files.write(&state, &dsp, "elsewhere", outside).await;
    assert_eq!(refused.unwrap_err().code, "files_folder_not_found");
    let huge = page("Write-ups", &"x".repeat(PAGE_LIMIT + 1), dre);
    let refused = files.write(&state, &dsp, &folder.id, huge).await;
    assert_eq!(refused.unwrap_err().code, "upload_too_large");
}
