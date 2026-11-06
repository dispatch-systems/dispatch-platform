//! Change-feed decoding.
use super::{ChangePage, Changed, Then};
use serde_json::json;

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
