//! The copy kept of a DSP's Drive follows what Google says changed: files made or moved into
//! the main folder appear, and files moved out, trashed or deleted go, with whatever a folder
//! held. A change Dispatch made itself stays when the feed describes the file from before it.
//! A folder that came in from elsewhere in the account asks for the Drive to be listed again,
//! since the copy doesn't know what it holds; and a copy answers only for the main folder it
//! was made from.
use super::*;
use crate::backend::{drive::Changed, tree::tests::item};

fn kept(item: Item) -> Changed {
    Changed::Kept(Box::new(item))
}
fn ids(mirror: &Mirror, folder: &str) -> Vec<String> {
    let tree = mirror.tree("main").unwrap();
    tree.children(folder)
        .into_iter()
        .map(|row| row.id.to_string())
        .collect()
}
fn changes(changed: Vec<Changed>, next: &str) -> Changes {
    Changes {
        changed,
        next: next.into(),
    }
}

#[test]
fn the_copy_follows_drive_and_keeps_what_dispatch_changed() {
    let mirror = Mirror::default();
    mirror.replace(
        "main",
        vec![
            item("main", "Northline Documents", None, true, "2026-10-01"),
            item("safety", "Safety", Some("main"), true, "2026-10-01"),
            item("drill", "Fire drill", Some("safety"), false, "2026-10-01"),
            item("plan", "Plan", Some("main"), false, "2026-10-02"),
            item("old", "Old plan", Some("main"), false, "2026-10-03"),
            item("other", "Summit Documents", None, true, "2026-10-01"),
            item("theirs", "Their plan", Some("other"), false, "2026-10-04"),
        ],
        "1".into(),
    );
    assert_eq!(ids(&mirror, "main"), ["safety", "old", "plan"]);

    // Dispatch renames the plan; the feed then describes it as it was before.
    mirror.put(
        "main",
        item("plan", "Safety plan", Some("main"), false, "2026-10-06"),
    );
    let relist = mirror.follow(
        "main",
        changes(
            vec![
                kept(item("plan", "Plan", Some("main"), false, "2026-10-02")),
                // Made in Drive, and moved in from the other folder.
                kept(item("new", "New plan", Some("main"), false, "2026-10-05")),
                kept(item(
                    "theirs",
                    "Their plan",
                    Some("main"),
                    false,
                    "2026-10-07",
                )),
                // Trashed or deleted in Drive, a file and a folder with what it held.
                Changed::Gone("old".into()),
                Changed::Gone("safety".into()),
                // A folder another DSP made with the same account.
                kept(item(
                    "theirs-new",
                    "Routes",
                    Some("other"),
                    true,
                    "2026-10-07",
                )),
            ],
            "2",
        ),
    );
    assert!(!relist);
    assert_eq!(ids(&mirror, "main"), ["theirs", "plan", "new"]);
    let tree = mirror.tree("main").unwrap();
    assert_eq!(&*tree.get("plan").unwrap().name, "Safety plan");
    assert!(tree.get("drill").is_none());
    assert_eq!(mirror.changes("main").as_deref(), Some("2"));

    // Moved out to the other folder; and a folder moved in, whose files the copy can't know.
    let relist = mirror.follow(
        "main",
        changes(
            vec![
                kept(item("new", "New plan", Some("other"), false, "2026-10-08")),
                kept(item("archive", "Archive", Some("main"), true, "2026-10-08")),
            ],
            "3",
        ),
    );
    assert!(relist);
    assert_eq!(ids(&mirror, "main"), ["archive", "theirs", "plan"]);

    // A DSP connected again with a new main folder isn't answered from the old one's copy.
    assert!(mirror.age("fresh").is_none() && mirror.tree("fresh").is_none());
    mirror.put(
        "fresh",
        item("stray", "Stray", Some("fresh"), false, "2026-10-09"),
    );
    assert!(mirror.tree("fresh").is_none());
}

#[test]
fn confirmations_are_ordered_before_later_renames_removals_and_advanced_feed_cursors() {
    for removal in [false, true] {
        let mirror = Mirror::default();
        mirror.replace(
            "main",
            vec![item("a", "Before", Some("main"), false, "2026-10-01")],
            "before".into(),
        );
        // A's picker response confirms first. B's request is still pending while another
        // request or the feed changes A. The later B confirmation must not replay A.
        mirror.put(
            "main",
            item("a", "Picked", Some("main"), false, "2026-10-02"),
        );
        let changed = if removal {
            Changed::Gone("a".into())
        } else {
            kept(item("a", "Renamed", Some("main"), false, "2026-10-03"))
        };
        mirror.follow("main", changes(vec![changed], "advanced"));
        mirror.put(
            "main",
            item("b", "Second", Some("main"), false, "2026-10-04"),
        );
        let tree = mirror.tree("main").unwrap();
        if removal {
            assert!(tree.get("a").is_none());
        } else {
            assert_eq!(&*tree.get("a").unwrap().name, "Renamed");
        }
        assert!(tree.get("b").is_some());
        assert_eq!(mirror.changes("main").as_deref(), Some("advanced"));
    }
    // A direct trash has the same ordering, including while successful picker results wait.
    let mirror = Mirror::default();
    mirror.replace("main", Vec::new(), "before".into());
    mirror.put(
        "main",
        item("a", "Picked", Some("main"), false, "2026-10-02"),
    );
    mirror.remove("main", "a");
    mirror.put(
        "main",
        item("b", "Second", Some("main"), false, "2026-10-04"),
    );
    assert!(mirror.tree("main").unwrap().get("a").is_none());

    // The file a shortcut stands for can disappear before a later selected file answers.
    let mirror = Mirror::default();
    mirror.replace("main", Vec::new(), "before".into());
    let shortcut = Item {
        mime_type: crate::backend::drive::SHORTCUT.into(),
        shortcut_details: Some(crate::backend::drive::Shortcut {
            target_id: "target".into(),
            target_mime_type: crate::backend::drive::SHEET.into(),
        }),
        ..item("shortcut", "Shortcut", Some("main"), false, "2026-10-02")
    };
    mirror.put_many(
        "main",
        [
            shortcut,
            item("target", "Target", None, false, "2026-10-02"),
        ],
    );
    mirror.follow(
        "main",
        changes(vec![Changed::Gone("target".into())], "advanced"),
    );
    mirror.put(
        "main",
        item("b", "Second", Some("main"), false, "2026-10-04"),
    );
    let tree = mirror.tree("main").unwrap();
    assert!(tree.get("shortcut").unwrap().unavailable());
    assert!(tree.get("b").is_some());
}
