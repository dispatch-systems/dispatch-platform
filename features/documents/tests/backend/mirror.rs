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

#[test]
fn semantic_publication_ignores_seen_and_layout_but_tracks_paths_counts_and_targets() {
    let mirror = Mirror::default();
    let folder = item("folder", "Folder", Some("main"), true, "text-date");
    let file = item("file", "File.pdf", Some("folder"), false, "text-date");
    mirror.replace("main", vec![folder.clone(), file.clone()], "a".into());
    let (first, semantic) = mirror.snapshot("main").unwrap();
    let mut observed = file.clone();
    observed.seen = first.get("file").unwrap().seen + Duration::from_secs(1);
    mirror.replace("main", vec![observed.clone(), folder.clone()], "b".into());
    let (same, current) = mirror.snapshot("main").unwrap();
    assert_eq!(
        current, semantic,
        "seen and packed positions are not content"
    );
    assert_eq!(same.get("file").unwrap().seen, observed.seen);
    mirror.put("main", observed.clone());
    assert!(
        mirror.matches("main", semantic),
        "confirmation reobservation is not content"
    );
    observed.seen += Duration::from_secs(1);
    assert!(!mirror.follow("main", changes(vec![kept(observed.clone())], "c")));
    let (seen, current) = mirror.snapshot("main").unwrap();
    assert_eq!(current, semantic, "feed reobservation is not content");
    assert_eq!(seen.get("file").unwrap().seen, observed.seen);
    let mut stale = observed.clone();
    stale.modified_time = "a-before".into();
    stale.name = "Rejected older feed".into();
    assert!(!mirror.follow("main", changes(vec![kept(stale)], "d")));
    assert!(
        mirror.matches("main", semantic),
        "rejected older metadata is not published"
    );
    mirror.follow("main", changes(Vec::new(), "e"));
    mirror.remove("main", "absent");
    assert!(!mirror.follow(
        "main",
        changes(
            vec![
                kept(item("outside", "Outside", Some("other-dsp"), false, "date")),
                Changed::Gone("absent".into()),
            ],
            "f"
        )
    ));
    assert!(
        mirror.matches("main", semantic),
        "discarded outside records are not content"
    );

    // The last confirmation wins without treating an intermediate rename as the final tree.
    let mut intermediate = observed.clone();
    intermediate.name = "Intermediate".into();
    mirror.put_many("main", [intermediate, observed.clone()]);
    assert!(
        mirror.matches("main", semantic),
        "restored batch has the same final semantics"
    );
    mirror.put(
        "main",
        item("second", "Second", Some("folder"), false, "text-date"),
    );
    let (_, counted) = mirror.snapshot("main").unwrap();
    assert_ne!(counted, semantic);
    let mut renamed = folder;
    renamed.name = "New path".into();
    mirror.put("main", renamed);
    let (path, renamed_at) = mirror.snapshot("main").unwrap();
    assert_ne!(renamed_at, counted);
    assert_eq!(&*path.path("file")[0].name, "New path");
    let mut moved = observed.clone();
    moved.parents = vec!["main".into()];
    mirror.put("main", moved.clone());
    let (parents, moved_at) = mirror.snapshot("main").unwrap();
    assert_ne!(moved_at, renamed_at);
    assert_eq!(parents.count("folder"), 1);
    assert_eq!(parents.path("file").len(), 1);
    mirror.remove("main", "folder");
    let (removed, removed_at) = mirror.snapshot("main").unwrap();
    assert_ne!(removed_at, moved_at);
    assert!(
        removed.get("second").is_none(),
        "subtree removal changes membership"
    );
    assert_eq!(
        first.count("folder"),
        1,
        "captured readers remain immutable"
    );
    drop((first, same, seen, path, parents, removed));

    // A weak or strong artificial reader would force make_mut to replace this allocation.
    // Retain only its address; never dereference it after dropping the real reader.
    let (unique, before) = mirror.snapshot("main").unwrap();
    let address = Arc::as_ptr(&unique);
    drop(unique);
    moved.name = "Actually changed.pdf".into();
    mirror.put("main", moved.clone());
    let (unique, after) = mirror.snapshot("main").unwrap();
    assert_eq!(
        Arc::as_ptr(&unique),
        address,
        "a unique owner must mutate without cloning Tree"
    );
    assert_ne!(after, before);
    drop(unique);
    moved.seen += Duration::from_secs(1);
    mirror.follow("main", changes(vec![kept(moved)], "g"));
    let (unique, observed_at) = mirror.snapshot("main").unwrap();
    assert_eq!(Arc::as_ptr(&unique), address);
    assert_eq!(observed_at, after);
    drop(unique);

    let mut shortcut = Item {
        mime_type: super::super::drive::SHORTCUT.into(),
        shortcut_details: Some(super::super::drive::Shortcut {
            target_id: "target".into(),
            target_mime_type: "application/pdf".into(),
        }),
        ..item("shortcut", "Alias", Some("main"), false, "date")
    };
    let mut target = item("target", "Target.pdf", None, false, "date");
    mirror.put_many("main", [shortcut.clone(), target.clone()]);
    let (_, reachable) = mirror.snapshot("main").unwrap();
    shortcut.seen += Duration::from_secs(1);
    target.seen += Duration::from_secs(1);
    mirror.put_many("main", [shortcut.clone(), target.clone(), target.clone()]);
    assert!(
        mirror.matches("main", reachable),
        "raw shortcut has no resolved stands_for"
    );
    assert!(!mirror.follow(
        "main",
        changes(vec![kept(target.clone()), kept(target.clone())], "h")
    ));
    let (current, observed_at) = mirror.snapshot("main").unwrap();
    assert_eq!(
        observed_at, reachable,
        "duplicate external observations compare canonical records"
    );
    assert_eq!(current.get("shortcut").unwrap().shown().seen, target.seen);
    drop(current);
    target.thumbnail_link = Some("fixture:new-target".into());
    target.thumbnail_version = Some("2".into());
    mirror.put("main", target.clone());
    let (_, pictured) = mirror.snapshot("main").unwrap();
    assert_ne!(
        pictured, reachable,
        "off-row target thumbnail metadata changes semantics"
    );
    target.name = "Renamed target.pdf".into();
    target.size = Some("42".into());
    mirror.follow("main", changes(vec![kept(target.clone())], "i"));
    let (current, named) = mirror.snapshot("main").unwrap();
    assert_ne!(named, pictured);
    assert_eq!(
        &*current.get("shortcut").unwrap().shown().name,
        "Renamed target.pdf"
    );
    drop(current);
    mirror.follow("main", changes(vec![Changed::Gone("target".into())], "j"));
    let (current, missing) = mirror.snapshot("main").unwrap();
    assert_ne!(
        missing, named,
        "target availability changes query continuity"
    );
    assert!(current.get("shortcut").unwrap().unavailable());
    drop(current);
    mirror.put("main", target);
    assert!(
        !mirror.matches("main", missing),
        "a newly reachable target also changes continuity"
    );

    // Keep the existing feed's duplicate-ID behavior: losing one duplicate changes counts
    // even when the final canonical record for that ID is unchanged.
    let duplicates = Mirror::default();
    duplicates.replace("main", Vec::new(), "start".into());
    let repeated = item("repeat", "Repeated", Some("main"), false, "same");
    duplicates.follow(
        "main",
        changes(vec![kept(repeated.clone()), kept(repeated)], "duplicates"),
    );
    let (both, before) = duplicates.snapshot("main").unwrap();
    assert_eq!(both.count("main"), 2);
    duplicates.remove("main", "repeat");
    let (one, after) = duplicates.snapshot("main").unwrap();
    assert_eq!(one.count("main"), 1);
    assert_ne!(
        after, before,
        "a changed count must invalidate even with identical ID metadata"
    );

    let repeated = item("repeat", "Repeated", Some("main"), false, "same");
    duplicates.replace(
        "main",
        vec![repeated.clone(), repeated.clone()],
        "two".into(),
    );
    let (_, before) = duplicates.snapshot("main").unwrap();
    duplicates.replace(
        "main",
        vec![repeated, item("new", "New", Some("main"), false, "same")],
        "distinct".into(),
    );
    assert!(
        !duplicates.matches("main", before),
        "a canonical ID match must not hide replacing one duplicate with another ID"
    );
}
