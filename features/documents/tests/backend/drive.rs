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
