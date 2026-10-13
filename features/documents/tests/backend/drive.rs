//! How Documents uploads to Google: Google describes the file an upload made with the fields
//! the upload's start asked for, or with only its kind, id, name and type when it asked for
//! none, whatever the request carrying the bytes asks.
use super::*;

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
fn a_change_feed_page_reads_as_files_kept_and_files_gone() {
    let page: ChangePage = serde_json::from_value(json!({
        "newStartPageToken": "42",
        "changes": [
            {"fileId": "kept", "removed": false, "file": {
                "id": "kept",
                "name": "Fuel receipts.pdf",
                "mimeType": "application/pdf",
                "parents": ["folder"],
                "modifiedTime": "2026-10-09T18:04:55.000Z",
                "webViewLink": "https://drive.google.com/file/d/kept/view",
                "trashed": false,
            }},
            {"fileId": "trashed", "removed": false, "file": {
                "id": "trashed",
                "name": "Old plan",
                "mimeType": "application/pdf",
                "modifiedTime": "2026-10-09T18:04:55.000Z",
                "webViewLink": "https://drive.google.com/file/d/trashed/view",
                "trashed": true,
            }},
            {"fileId": "deleted", "removed": true},
            // A change to a shared drive itself.
            {"removed": false},
        ],
    }))
    .expect("Google's page reads");
    let mut changed = Vec::new();
    assert!(matches!(page.read(&mut changed), Some(Then::Done(next)) if next == "42"));
    let read: Vec<(bool, &str)> = changed
        .iter()
        .map(|change| match change {
            Changed::Kept(item) => (true, item.id.as_str()),
            Changed::Gone(id) => (false, id.as_str()),
        })
        .collect();
    assert_eq!(
        read,
        [(true, "kept"), (false, "trashed"), (false, "deleted")]
    );
    assert!(matches!(&changed[0], Changed::Kept(item) if item.parents == ["folder"]));
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
