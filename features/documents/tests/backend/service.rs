//! Documents keeping files for another feature through the `files` contract it exposes, against fixture
//! mode's Drive: only while Documents is on and its account connected, only inside the DSP's
//! own folder, and named for whoever they came from, and taken back; and pages written as Docs,
//! and over them.
use super::*;
use crate::backend::{
    support::{connect, platform},
    tree::Mime,
};
use axum::body::Bytes;
use dispatch_core::{server::http::upload::Upload, testing};
use dispatch_documents_contract::{self as files, Files, NewFolder, Page};
use futures_util::stream;
use serde_json::json;

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
/// The `files` contract's offering for the DSP, as a feature using it finds it.
async fn offered(state: &Arc<State>, dsp: &str) -> Option<&'static dyn Files> {
    let at = dsp.to_owned();
    state.read(move |db| files::for_dsp(db, &at)).await.unwrap()
}

#[tokio::test]
async fn files_are_kept_inside_the_dsps_folder_once_connected_named_for_who_sent_them_and_taken_back()
 {
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
    let named = |parent, name, logged_as| NewFolder {
        parent,
        name,
        by: &member,
        logged_as,
    };
    let folder = files
        .create_folder(&state, &dsp, named(None, "photos", None))
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

    // One made inside it is there, logged as what it's to be called where its name is someone's,
    // and none is made inside what isn't a folder of the DSP's.
    let theirs = named(Some(&folder.id), "Dre Holloway", Some("a driver's folder"));
    let made = files.create_folder(&state, &dsp, theirs).await.unwrap();
    let inside = files.browse(&state, &dsp, Some(&folder.id)).await.unwrap();
    assert_eq!(inside.folders, std::slice::from_ref(&made));
    // Read as the platform reads it: its owner made them, whose doings a DSP's own log leaves out.
    let logged = state.read(|db| testing::audits(db, None)).await.unwrap();
    let created: Vec<&str> = logged
        .as_array()
        .into_iter()
        .flatten()
        .filter(|event| event["action"] == "documents.created")
        .filter_map(|event| event["detail"].as_str())
        .collect();
    assert_eq!(created.len(), 2);
    assert!(created.contains(&"photos") && created.contains(&"a driver's folder"));
    let refused = files
        .create_folder(&state, &dsp, named(Some("elsewhere"), "vans", None))
        .await;
    assert_eq!(refused.unwrap_err().code, "files_folder_not_found");

    // A file from someone known only by name goes into the folder, as theirs, logged with no
    // member as its actor.
    let dre = Author::Named("Dre Holloway".into());
    let file = incoming("van.jpg", "jpeg", dre.clone()).await;
    let saved = files.save(&state, &dsp, &folder.id, file).await.unwrap();
    let captured = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap();
    let tree = crate::backend::access::tree_for(&state, &captured)
        .await
        .unwrap();
    let parent = tree.get(&saved.id).and_then(|row| row.parent.as_deref());
    assert_eq!(parent, Some(folder.id.as_str()));
    let at = dsp.clone();
    let id = saved.id.clone();
    let (records, logged) = state
        .read(move |db| {
            let logged = testing::audits(db, Some(&at))?;
            Ok((db.documents_records(&at, &[id])?, logged))
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

    // Taken back, it goes to the trash, logged with no member as its actor for someone known
    // only by name; one already gone, a folder, or a file elsewhere isn't.
    assert!(files.remove(&state, &dsp, &saved.id, &dre).await.unwrap());
    let tree = crate::backend::access::tree_for(&state, &captured)
        .await
        .unwrap();
    assert!(tree.get(&saved.id).is_none());
    let at = dsp.clone();
    let logged = state
        .read(move |db| testing::audits(db, Some(&at)))
        .await
        .unwrap();
    let trashing = logged
        .as_array()
        .into_iter()
        .flatten()
        .find(|event| event["action"] == "documents.trashed")
        .expect("taking it back is logged");
    assert_eq!(trashing["detail"], "van.jpg");
    assert!(trashing["actorId"].is_null());
    for id in [saved.id.as_str(), folder.id.as_str(), "elsewhere"] {
        assert!(!files.remove(&state, &dsp, id, &dre).await.unwrap());
    }
    assert!(tree.get(&folder.id).is_some());

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
    let canvases = NewFolder {
        parent: None,
        name: "canvases",
        by: &Author::Member(owner.clone()),
        logged_as: None,
    };
    let folder = files.create_folder(&state, &dsp, canvases).await.unwrap();

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
    let captured = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap();
    let tree = crate::backend::access::tree_for(&state, &captured)
        .await
        .unwrap();
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
    let id = written.id.clone();
    let (records, logged) = state
        .read(move |db| {
            Ok((
                db.documents_records(&at, &[id])?,
                testing::audits(db, Some(&at))?,
            ))
        })
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
