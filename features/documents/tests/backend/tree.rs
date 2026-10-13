//! A DSP's Documents is its main folder's tree and nothing else the account holds: another
//! DSP's folder made with the same account, and anything Dispatch made outside the folder, is
//! never listed, found or named.
use super::*;
use crate::backend::{drive::Person, google::FOLDER};

/// A file or folder as Drive describes it, in the folder `parent`.
pub(crate) fn item(
    id: &str,
    name: &str,
    parent: Option<&str>,
    folder: bool,
    modified: &str,
) -> Item {
    Item {
        id: id.into(),
        name: name.into(),
        mime_type: if folder { FOLDER } else { "application/pdf" }.into(),
        parents: parent.map(str::to_owned).into_iter().collect(),
        modified_time: modified.into(),
        last_modifying_user: Some(Person {
            display_name: None,
            email_address: None,
        }),
        size: None,
        web_view_link: String::new(),
        thumbnail_link: None,
        thumbnail_version: None,
        owned_by_me: true,
        shortcut_details: None,
        seen: std::time::Instant::now(),
    }
}

#[test]
fn the_tree_is_the_main_folder_and_only_what_it_holds() {
    let tree = Tree::new(
        "main",
        vec![
            item(
                "main",
                "Northline Logistics Documents",
                None,
                true,
                "2026-10-01",
            ),
            item("safety", "Safety", Some("main"), true, "2026-10-02"),
            item("fleet", "fleet", Some("main"), true, "2026-10-03"),
            item("old", "Old plan", Some("safety"), false, "2026-10-04"),
            item("new", "New plan", Some("safety"), false, "2026-10-05"),
            item("top", "Plan for the top", Some("main"), false, "2026-10-06"),
            // Another DSP's folder in the same account, and a file of it.
            item(
                "other",
                "Summit Delivery Documents",
                None,
                true,
                "2026-10-01",
            ),
            item("theirs", "Their plan", Some("other"), false, "2026-10-07"),
            // Files whose parents loop, reaching no main folder.
            item("a", "Loop plan", Some("b"), false, "2026-10-08"),
            item("b", "Loop folder", Some("a"), true, "2026-10-08"),
        ],
    );
    // Folders first, by name in any case; then files, the newest first.
    let ids = |rows: Vec<&Row>| {
        rows.into_iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(tree.children("main")), ["fleet", "safety", "top"]);
    assert_eq!(ids(tree.children("safety")), ["new", "old"]);
    assert_eq!((tree.count("main"), tree.count("safety")), (3, 2));
    assert!(tree.get("theirs").is_none() && tree.get("a").is_none() && !tree.is_folder("other"));
    assert!(tree.is_folder("main") && tree.is_folder("safety") && !tree.is_folder("top"));
    assert_eq!(ids(tree.path("old")), ["safety", "old"]);
    assert!(tree.path("main").is_empty());
    // A search reaches every depth of the folder it starts in, and nothing outside it.
    assert_eq!(ids(tree.search("main", "PLAN")), ["top", "new", "old"]);
    assert_eq!(ids(tree.search("safety", "plan")), ["new", "old"]);
    assert_eq!(ids(tree.search("fleet", "plan")), Vec::<String>::new());
    assert_eq!(ids(tree.search("main", "new plan")), ["new"]);
}

#[test]
fn a_shortcut_shows_as_the_file_it_stands_for_and_one_to_a_folder_opens_in_drive() {
    let shortcut = |id: &str, target: &str, mime: &str| Item {
        mime_type: drive::SHORTCUT.into(),
        shortcut_details: Some(drive::Shortcut {
            target_id: target.into(),
            target_mime_type: mime.into(),
        }),
        ..item(id, "Shared", Some("main"), false, "2026-10-02")
    };
    let tree = Tree::new(
        "main",
        vec![
            item("main", "Documents", None, true, "2026-10-01"),
            shortcut("to-sheet", "sheet", drive::SHEET),
            shortcut("to-folder", "folder", FOLDER),
        ],
    );
    let sheet = tree.get("to-sheet").unwrap();
    assert_eq!(sheet.mime, Mime::Sheet);
    assert_eq!(
        &*sheet.link,
        "https://docs.google.com/spreadsheets/d/sheet/edit"
    );
    assert_eq!(sheet.target.as_deref(), Some("sheet"));
    let folder = tree.get("to-folder").unwrap();
    assert!(!folder.folder());
    assert_eq!(
        &*folder.link,
        "https://drive.google.com/drive/folders/folder"
    );
    assert!(tree.stands_for("sheet") && tree.stands_for("folder"));
    assert!(!tree.stands_for("to-sheet"));
}
