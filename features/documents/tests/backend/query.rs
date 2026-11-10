//! Real compact-index and Files reads, with authored expectations rather than the comparator.
use super::*;
use crate::api::types::{
    DocumentsDescriptionRead, DocumentsQueryPage, DocumentsQueryRead, DocumentsResetReason,
};
use crate::backend::{
    cache, drive, files, mirror,
    storage::DocumentsStore,
    support::{connect, context, platform},
    tree::tests::item,
};
use dispatch_core::{db::Store, server::http::route::Dsp, testing};
use dispatch_documents_contract::Author;

#[test]
fn admitted_index_is_built_once_and_a_continuation_miss_does_not_build() {
    let cache = ReadCache::default();
    let tree = Tree::new(
        "main",
        vec![item("file", "File", Some("main"), false, "date")],
    );
    let scope = Scope::tenant(CACHED, "one");
    let first = index(
        &cache,
        scope.clone(),
        "key".into(),
        19,
        &tree,
        ("main", None),
        true,
    )
    .unwrap()
    .unwrap();
    let joined = index(
        &cache,
        scope.clone(),
        "key".into(),
        19,
        &tree,
        ("main", None),
        true,
    )
    .unwrap()
    .unwrap();
    assert!(Arc::ptr_eq(&first, &joined));
    let continued = index(
        &cache,
        scope.clone(),
        "key".into(),
        19,
        &tree,
        ("main", None),
        false,
    )
    .unwrap()
    .unwrap();
    assert!(Arc::ptr_eq(&first, &continued));
    assert!(
        index(
            &cache,
            Scope::tenant(CACHED, "two"),
            "key".into(),
            19,
            &tree,
            ("main", None),
            false
        )
        .unwrap()
        .is_none()
    );
    cache.invalidate_dsp("one");
    assert!(
        index(
            &cache,
            scope,
            "key".into(),
            19,
            &tree,
            ("main", None),
            false
        )
        .unwrap()
        .is_none()
    );
}

fn request(filter: DocumentsFilter, page: u32) -> QueryRequest {
    QueryRequest {
        scope: DocumentsQueryScope {
            id: None,
            q: None,
            filter,
        },
        page,
        size: 100,
        snapshot: None,
        manifest_since: None,
        since: None,
    }
}
fn full(answer: DocumentsQueryRead) -> (DocumentsQueryPage, DocumentsManifest, String) {
    match answer {
        DocumentsQueryRead::Full {
            reading,
            manifest,
            content_revision,
        } => (
            *reading,
            manifest.expect("opening includes complete manifest"),
            content_revision,
        ),
        other => panic!("expected full opening, got {other:?}"),
    }
}

#[tokio::test]
async fn full_order_search_filters_and_counts_survive_page_boundaries_and_shortcuts() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let c = context(&state, &dsp).await;
    let at = dsp.clone();
    let connection = state
        .read(move |db| Ok(db.documents_connection(&at)?.unwrap()))
        .await
        .unwrap();
    let root = &connection.folder_id;
    let mut all = vec![
        item("a", "Case", Some(root), true, "0"),
        item("b", "case", Some(root), true, "0"),
        item("digits", "10", Some(root), true, "0"),
        item("bmp", "\u{e000}", Some(root), true, "0"),
        item("supp", "\u{10000}", Some(root), true, "0"),
        item("inside", "Plan needle", Some("a"), false, "z"),
        item("elsewhere", "Plan needle", Some("other-dsp"), false, "zz"),
        item("text-new", "Literal time", Some(root), false, "2026-9"),
        item("text-old", "Literal time", Some(root), false, "2026-10"),
    ];
    for n in (0..300).rev() {
        let mut row = item(
            &format!("f{n:03}"),
            "Plan needle",
            Some(root),
            false,
            "same",
        );
        row.mime_type = match n % 3 {
            0 => drive::DOC,
            1 => drive::SHEET,
            _ => drive::SLIDES,
        }
        .into();
        all.push(row);
    }
    all.push(drive::Item {
        mime_type: drive::SHORTCUT.into(),
        shortcut_details: Some(drive::Shortcut {
            target_id: "outside-target".into(),
            target_mime_type: drive::SHEET.into(),
        }),
        ..item("shortcut", "Alias", Some(root), false, "0")
    });
    all.push(drive::Item {
        mime_type: drive::SHEET.into(),
        ..item("outside-target", "Plan needle target", None, false, "zz")
    });
    let key = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap()
        .key;
    let owner = mirror::of(&key);
    owner.replace(root, all, "cursor".into());
    let (tree, _) = owner.snapshot(root).unwrap();
    let before = Arc::strong_count(&tree);
    let (first, manifest, _) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        Arc::strong_count(&tree),
        before,
        "index must not retain a hidden Tree"
    );
    assert_eq!(first.total, 308);
    assert_eq!(first.folder.items, 308);
    assert_eq!(
        (
            first.counts.folders,
            first.counts.docs,
            first.counts.sheets,
            first.counts.slides,
            first.counts.uploads
        ),
        (5, 100, 101, 100, 2)
    );
    let mut expected: Vec<String> = ["digits", "a", "b", "bmp", "supp", "shortcut"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    expected.extend((0..300).map(|n| format!("f{n:03}")));
    expected.extend(["text-new".into(), "text-old".into()]);
    assert_eq!(
        manifest
            .entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(first.items.first().unwrap().id, "digits");
    assert_eq!(first.items.last().unwrap().id, "f093");
    let mut joined = first
        .items
        .iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for page in 1..4 {
        let mut req = request(DocumentsFilter::All, page);
        req.snapshot = Some(first.snapshot.clone());
        req.manifest_since = Some(first.manifest_revision.clone());
        match query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap()
        {
            DocumentsQueryRead::Full {
                reading, manifest, ..
            } => {
                assert!(manifest.is_none());
                assert_eq!(reading.page, page);
                joined.extend(reading.items.into_iter().map(|row| row.id));
            }
            other => panic!("page continuation reset: {other:?}"),
        }
    }
    assert_eq!(joined, expected);
    let (docs, compact, _) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::Docs, 99),
        )
        .await
        .unwrap(),
    );
    assert_eq!(docs.page, 0);
    assert_eq!(docs.total, 100);
    assert_eq!(
        compact.entries.len(),
        308,
        "filtered result retains complete unfiltered universe"
    );
    assert_eq!(
        docs.items
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        (0..100)
            .map(|n| format!("f{:03}", n * 3))
            .collect::<Vec<_>>()
    );
    let mut req = request(DocumentsFilter::All, 0);
    req.scope.id = Some("a".into());
    req.scope.q = Some(" PLAN  needle ".into());
    let (nested, _, _) = full(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
    );
    assert_eq!(
        nested
            .items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["inside"]
    );
    assert_eq!(nested.items[0].location.as_deref(), Some("Case"));
    assert_eq!(nested.folder.items, 1);
    let mut req = request(DocumentsFilter::All, 0);
    req.scope.q = Some("case".into());
    req.scope.id = Some("a".into());
    assert!(
        full(
            query(&state, c.clone(), Dsp("documents.use"), req)
                .await
                .unwrap()
        )
        .0
        .items
        .is_empty(),
        "nested search excludes itself"
    );

    // Legacy feeds can retain two raw rows for one ID. Paging must not replace one
    // description with the canonical other's metadata, even when search selects only one.
    owner.replace(root, Vec::new(), "empty".into());
    owner.follow(
        root,
        drive::Changes {
            changed: vec![
                drive::Changed::Kept(Box::new(item("repeat", "First", Some(root), false, "same"))),
                drive::Changed::Kept(Box::new(item(
                    "repeat",
                    "Second",
                    Some(root),
                    false,
                    "same",
                ))),
            ],
            next: "duplicates".into(),
        },
    );
    for q in [None, Some("First")] {
        let mut req = request(DocumentsFilter::All, 0);
        req.scope.q = q.map(str::to_owned);
        assert!(matches!(
            query(&state, c.clone(), Dsp("documents.use"), req)
                .await
                .unwrap(),
            DocumentsQueryRead::Reset {
                reason: DocumentsResetReason::Missing
            }
        ));
    }
    let legacy = files::folder(&state, &c, None, None).await.unwrap();
    let mut names: Vec<_> = legacy.items.iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["First", "Second"],
        "fallback keeps the original complete projection"
    );
}

#[tokio::test]
async fn continuations_project_only_the_page_and_current_authors_then_reset_without_rebuilding() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let c = context(&state, &dsp).await;
    let at = dsp.clone();
    let (connection, author) = state
        .run(move |db| {
            let author = testing::owner_of(db, &at, "query-author@example.test", "Avery")?;
            db.record_documents_change(&at, "f000", &author)?;
            db.record_documents_change_by(&at, "f100", &Author::Named("Outside author".into()))?;
            Ok((db.documents_connection(&at)?.unwrap(), author))
        })
        .await
        .unwrap();
    let given = std::time::Instant::now() - std::time::Duration::from_secs(4000);
    let rows: Vec<drive::Item> = (0..101)
        .map(|n| {
            let mut row = item(
                &format!("f{n:03}"),
                "Page.pdf",
                Some(&connection.folder_id),
                false,
                "same",
            );
            row.thumbnail_link = Some(format!("fixture:query:{n}"));
            row.seen = given;
            row
        })
        .collect();
    let key = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap()
        .key;
    let owner = mirror::of(&key);
    owner.replace(&connection.folder_id, rows.clone(), "cursor".into());
    let (first, _, revision) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(first.items[0].added_by.as_deref(), Some("Avery Owner"));
    assert!(cache::shown(&key, "f099").is_some());
    assert!(
        cache::shown(&key, "f100").is_none(),
        "off-page projection admits an unwanted thumbnail"
    );
    assert_eq!(cache::shown(&key, "f000").unwrap().given, given);
    let mut req = request(DocumentsFilter::All, 0);
    req.snapshot = Some(first.snapshot.clone());
    req.manifest_since = Some(first.manifest_revision.clone());
    req.since = Some(revision.clone());
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Unchanged { .. }
    ));
    // Use the real Store fixture boundary without invalidating the compact index. A tree-only
    // digest or retained author descriptions would incorrectly answer unchanged.
    let db = Store::open(state.config.clone(), state.key.clone()).unwrap();
    testing::set_user_name(&db, Some(&dsp), &author, "Renamed", "Owner").unwrap();
    testing::set_user_status(&db, Some(&dsp), &author, "disabled").unwrap();
    drop(db);
    let mut req = request(DocumentsFilter::All, 0);
    req.snapshot = Some(first.snapshot.clone());
    req.manifest_since = Some(first.manifest_revision.clone());
    req.since = Some(revision.clone());
    match query(&state, c.clone(), Dsp("documents.use"), req)
        .await
        .unwrap()
    {
        DocumentsQueryRead::Full {
            reading,
            content_revision,
            manifest,
        } => {
            assert!(manifest.is_none());
            assert_eq!(reading.snapshot, first.snapshot);
            assert_eq!(reading.items[0].added_by.as_deref(), Some("Renamed Owner"));
            assert_ne!(content_revision, revision);
        }
        other => panic!("current author must change the page: {other:?}"),
    }
    let ids = vec!["f100".into(), "f000".into()];
    match describe(
        &state,
        c.clone(),
        Dsp("documents.use"),
        request(DocumentsFilter::All, 0).scope,
        first.snapshot.clone(),
        ids,
    )
    .await
    .unwrap()
    {
        DocumentsDescriptionRead::Full { items, .. } => {
            assert_eq!(
                items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                ["f100", "f000"]
            );
            assert_eq!(items[0].added_by.as_deref(), Some("Outside author"));
        }
        other => panic!("description reset: {other:?}"),
    }
    assert_eq!(cache::shown(&key, "f100").unwrap().given, given);
    state.read_cache.invalidate_dsp(&dsp);
    let mut req = request(DocumentsFilter::All, 1);
    req.snapshot = Some(first.snapshot.clone());
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Reset {
            reason: DocumentsResetReason::Missing
        }
    ));
    assert!(matches!(
        describe(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0).scope,
            first.snapshot.clone(),
            vec!["f000".into()]
        )
        .await
        .unwrap(),
        DocumentsDescriptionRead::Reset {
            reason: DocumentsResetReason::Missing
        }
    ));
    let (opened, _, _) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    let mut changed = item(
        "f000",
        "Changed",
        Some(&connection.folder_id),
        false,
        "same",
    );
    changed.thumbnail_link = Some("fixture:changed".into());
    owner.put(&connection.folder_id, changed.clone());
    let mut req = request(DocumentsFilter::All, 1);
    req.snapshot = Some(opened.snapshot.clone());
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Reset {
            reason: DocumentsResetReason::Changed
        }
    ));
    let (latest, _, _) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    mirror::forget(&key.namespace, &dsp);
    let replacement = mirror::of(&key);
    replacement.replace(&connection.folder_id, rows, "new incarnation".into());
    replacement.put(&connection.folder_id, changed);
    assert_eq!(
        owner.snapshot(&connection.folder_id).unwrap().1,
        replacement.snapshot(&connection.folder_id).unwrap().1,
        "same semantic sequence/data/root still requires a different incarnation"
    );
    assert!(matches!(
        describe(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0).scope,
            latest.snapshot,
            vec!["f000".into()]
        )
        .await
        .unwrap(),
        DocumentsDescriptionRead::Reset {
            reason: DocumentsResetReason::Changed
        }
    ));

    // Exercise continuation identity through the real Files path after only observations
    // and packed-layout changes, then change an external target without changing its alias.
    let mut shortcut = drive::Item {
        mime_type: drive::SHORTCUT.into(),
        shortcut_details: Some(drive::Shortcut {
            target_id: "external-target".into(),
            target_mime_type: "application/pdf".into(),
        }),
        ..item("alias", "Alias", Some(&connection.folder_id), false, "same")
    };
    let mut target = item("external-target", "External.pdf", None, false, "same");
    target.thumbnail_link = Some("fixture:external-target".into());
    target.thumbnail_version = Some("1".into());
    replacement.replace(
        &connection.folder_id,
        vec![shortcut.clone(), target.clone()],
        "external".into(),
    );
    let (external, manifest, revision) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(manifest.entries[0].id, "alias");
    assert_eq!(external.items[0].name, "External.pdf");
    shortcut.seen += std::time::Duration::from_secs(1);
    target.seen += std::time::Duration::from_secs(1);
    replacement.put_many(&connection.folder_id, [shortcut.clone(), target.clone()]);
    replacement.follow(
        &connection.folder_id,
        drive::Changes {
            changed: vec![
                drive::Changed::Kept(Box::new(target.clone())),
                drive::Changed::Kept(Box::new(target.clone())),
            ],
            next: "observed".into(),
        },
    );
    // Reverse the complete listing too: equivalent publication must still find the index.
    replacement.replace(
        &connection.folder_id,
        vec![target.clone(), shortcut.clone()],
        "repacked".into(),
    );
    let mut req = request(DocumentsFilter::All, 0);
    req.snapshot = Some(external.snapshot.clone());
    req.manifest_since = Some(external.manifest_revision.clone());
    req.since = Some(revision);
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Unchanged { .. }
    ));
    assert_eq!(
        cache::shown(&key, "external-target").unwrap().given,
        target.seen
    );
    match describe(
        &state,
        c.clone(),
        Dsp("documents.use"),
        request(DocumentsFilter::All, 0).scope,
        external.snapshot.clone(),
        vec!["alias".into()],
    )
    .await
    .unwrap()
    {
        DocumentsDescriptionRead::Full { items, .. } => {
            assert_eq!(
                items[0].id, "alias",
                "shortcut actions keep the row identity"
            );
            assert_eq!(items[0].name, "External.pdf");
        }
        other => panic!("observation/layout must preserve describe continuity: {other:?}"),
    }
    // Thumbnail/version are absent from the compact manifest but still fence continuation.
    target.thumbnail_version = Some("2".into());
    replacement.put(&connection.folder_id, target.clone());
    let mut req = request(DocumentsFilter::All, 0);
    req.snapshot = Some(external.snapshot.clone());
    req.manifest_since = Some(external.manifest_revision);
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Reset {
            reason: DocumentsResetReason::Changed
        }
    ));
    assert!(matches!(
        describe(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0).scope,
            external.snapshot,
            vec!["alias".into()]
        )
        .await
        .unwrap(),
        DocumentsDescriptionRead::Reset {
            reason: DocumentsResetReason::Changed
        }
    ));
    let (pictured, _, _) = full(
        query(
            &state,
            c.clone(),
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        pictured.items[0].thumbnail.as_deref(),
        Some("/api/dsp/documents/items/external-target/thumbnail?v=2")
    );
    replacement.follow(
        &connection.folder_id,
        drive::Changes {
            changed: vec![drive::Changed::Gone("external-target".into())],
            next: "lost-target".into(),
        },
    );
    let mut req = request(DocumentsFilter::All, 0);
    req.snapshot = Some(pictured.snapshot);
    assert!(matches!(
        query(&state, c.clone(), Dsp("documents.use"), req)
            .await
            .unwrap(),
        DocumentsQueryRead::Reset {
            reason: DocumentsResetReason::Changed
        }
    ));
    let (unavailable, _, _) = full(
        query(
            &state,
            c,
            Dsp("documents.use"),
            request(DocumentsFilter::All, 0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(unavailable.items[0].id, "alias");
    assert!(unavailable.items[0].unavailable);
    assert_eq!(unavailable.items[0].name, "Alias");
}
