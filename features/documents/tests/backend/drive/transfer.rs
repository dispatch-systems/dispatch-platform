//! How Documents uploads to Google: Google describes the file an upload made with the fields
//! the upload's start asked for, or with only its kind, id, name and type when it asked for
//! none, whatever the request carrying the bytes asks; and how a page goes up to be a Doc.
use super::{Item, page, sized, upload_start};
use serde_json::json;

/// Google's answer at the end of an upload started with `start`: the file it made, with the
/// fields `start` asked for.
fn answer(start: &reqwest::Request) -> serde_json::Value {
    let file = json!({
        "kind": "drive#file",
        "id": "1a2b3c",
        "name": "Fuel receipts.pdf",
        "mimeType": "application/pdf",
        "parents": ["folder"],
        "modifiedTime": "2026-10-09T18:04:55.000Z",
        "lastModifyingUser": {"displayName": "Keisha Brown", "emailAddress": "keisha@example.com"},
        "size": "48213",
        "webViewLink": "https://drive.google.com/file/d/1a2b3c/view",
    });
    let asked = start
        .url()
        .query_pairs()
        .find(|(key, _)| key == "fields")
        .map_or_else(
            || "kind,id,name,mimeType".to_owned(),
            |(_, f)| f.into_owned(),
        );
    // Each field asked for, without what's asked of it in parentheses.
    let mut depth = 0;
    let names: Vec<&str> = asked
        .split(|c| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            c == ',' && depth == 0
        })
        .map(|field| field.split('(').next().unwrap_or(field))
        .collect();
    let mut file = file.as_object().cloned().unwrap_or_default();
    file.retain(|key, _| names.contains(&key.as_str()));
    serde_json::Value::Object(file)
}

#[test]
fn an_upload_asks_at_its_start_for_everything_documents_reads_of_the_file_it_made() {
    let start = upload_start(
        "token",
        "Fuel receipts.pdf",
        "application/pdf",
        "folder",
        48213,
    )
    .build()
    .unwrap();
    let made: Item = serde_json::from_value(answer(&start)).expect("Google's answer reads");
    assert_eq!(made.parents, ["folder"]);
    assert_eq!(
        made.web_view_link,
        "https://drive.google.com/file/d/1a2b3c/view"
    );
}

#[test]
fn a_picture_is_asked_of_google_at_the_size_the_page_asked_for_and_only_of_google() {
    let link = "https://lh3.googleusercontent.com/drive-storage/AJQWtB=s220";
    assert_eq!(
        sized(link, 300).as_deref(),
        Some("https://lh3.googleusercontent.com/drive-storage/AJQWtB=s300")
    );
    assert_eq!(
        sized(link, 600).as_deref(),
        Some("https://lh3.googleusercontent.com/drive-storage/AJQWtB=s600")
    );
    assert_eq!(sized("https://example.com/picture=s220", 300), None);
}

#[test]
fn a_page_goes_to_google_with_its_description_first_parted_from_it_by_a_boundary_it_lacks() {
    let html = "<p>--page- runs through a canvas</p>";
    let (kind, body) = page(&json!({"name": "Write-ups"}), html).unwrap();
    let boundary = kind
        .strip_prefix("multipart/related; boundary=")
        .expect("Google reads both parts at once");
    assert!(!html.contains(boundary));
    let body = String::from_utf8(body).unwrap();
    let parts: Vec<&str> = body.split(&format!("--{boundary}")).collect();
    assert_eq!(parts.len(), 4, "the description, the page, and the end");
    assert_eq!(
        parts[1],
        "\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{\"name\":\"Write-ups\"}\r\n"
    );
    assert_eq!(
        parts[2],
        format!("\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n{html}\r\n")
    );
    assert_eq!(parts[3], "--");
}

#[tokio::test]
async fn a_picture_waiting_for_a_permit_cannot_use_a_replaced_or_removed_connection() {
    use super::super::fixture::fixture;
    use crate::backend::{
        access, grant,
        storage::DocumentsStore,
        support::{connect_as, platform},
        tree::tests::item,
    };
    use dispatch_core::foundation::crypto;
    use std::{
        future::Future,
        task::{Context, Waker},
    };

    for reconnect in [false, true] {
        let (_root, state, dsp) = platform();
        let account = format!(
            "fixture:picture-{}@example.test",
            crypto::token().unwrap().to_ascii_lowercase()
        );
        connect_as(&state, &dsp, &account).await;
        let owner = grant::capture(&state, &dsp, None).await.unwrap();
        let drive = access::open_for(&state, &owner, false).await.unwrap();
        fixture(&account, |remote| {
            let mut image = item(
                "image",
                "Image.png",
                Some(&owner.connection.folder_id),
                false,
                "2026-11-01",
            );
            image.mime_type = "image/png".into();
            remote.files.insert("image".into(), image);
            remote.contents.insert("image".into(), vec![1, 2, 3]);
            Ok(())
        })
        .unwrap();
        let before = fixture(&account, |remote| Ok(remote.requests)).unwrap();
        let permit = super::PICTURES.acquire_many(8).await.unwrap();
        let mut picture = Box::pin(drive.google.picture(&account, "fixture:image", 300));
        assert!(
            picture
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        let (at, connection) = (dsp.clone(), owner.connection.clone());
        state
            .run(move |db| {
                if reconnect {
                    let refresh = db.documents_refresh_token(&at)?.unwrap();
                    db.save_documents_connection(&at, &connection, &refresh)?;
                } else {
                    db.remove_documents_connection(&at)?;
                }
                Ok(())
            })
            .await
            .unwrap();
        drop(permit);
        assert_eq!(picture.await.err().unwrap().code, "documents_not_connected");
        assert_eq!(
            fixture(&account, |remote| Ok(remote.requests)).unwrap(),
            before
        );
        if reconnect {
            let current = grant::capture(&state, &dsp, None).await.unwrap();
            let current = access::open_for(&state, &current, false).await.unwrap();
            assert_eq!(
                current
                    .google
                    .picture(&account, "fixture:image", 300)
                    .await
                    .unwrap()
                    .bytes
                    .as_ref(),
                &[1, 2, 3]
            );
        }
    }
}

#[tokio::test]
async fn fixture_upload_rechecks_after_consuming_the_waited_body_before_mutating_drive() {
    use super::super::fixture::fixture;
    use crate::backend::{
        access, grant,
        storage::DocumentsStore,
        support::{connect_as, platform},
    };
    use axum::body::Bytes;
    use dispatch_core::{foundation::crypto, server::http::upload::Upload};
    let (_root, state, dsp) = platform();
    let account = format!(
        "fixture:upload-{}@example.test",
        crypto::token().unwrap().to_ascii_lowercase()
    );
    connect_as(&state, &dsp, &account).await;
    let owner = grant::capture(&state, &dsp, None).await.unwrap();
    let drive = access::open_for(&state, &owner, false).await.unwrap();
    let session = drive
        .google
        .start_upload(
            &account,
            "Held.pdf",
            "application/pdf",
            &owner.connection.folder_id,
            4,
        )
        .await
        .unwrap();
    let before = fixture(&account, |remote| Ok(remote.requests)).unwrap();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let bytes = futures_util::stream::once(async move {
        entered.send(()).unwrap();
        released.await.unwrap();
        Ok::<_, std::io::Error>(Bytes::from_static(b"held"))
    });
    let upload = Upload::streamed(4, 4, bytes).await.unwrap();
    let (token, held_session) = (account.clone(), session.clone());
    let work = tokio::spawn(async move {
        drive
            .google
            .finish_upload(&token, &held_session, upload)
            .await
    });
    waiting.await.unwrap();
    state
        .run(move |db| db.remove_documents_connection(&dsp))
        .await
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(
        work.await.unwrap().unwrap_err().code,
        "documents_not_connected"
    );
    fixture(&account, |remote| {
        assert_eq!(remote.requests, before);
        assert!(remote.uploads.contains_key(&session));
        assert!(!remote.files.values().any(|file| file.name == "Held.pdf"));
        Ok(())
    })
    .unwrap();
}
