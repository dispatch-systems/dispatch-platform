//! Conditional reads exercise the shipped projection and authorization path.
use super::*;
use crate::{
    api::types::ItemKind,
    backend::{
        cache, google, mirror,
        query::{QueryRequest, describe, query},
        support::{connect, context, platform},
        tree::tests::item,
    },
};
use dispatch_core::{db::Store, testing};
use dispatch_documents_contract::Author;

fn response() -> DocumentsFolder {
    let file = DocumentsItem {
        id: "file".into(),
        name: "Plan.pdf".into(),
        kind: ItemKind::Pdf,
        size: Some(10),
        modified_at: "2026-11-01".into(),
        modified_by: Some("Morgan".into()),
        added_by: Some("Alex".into()),
        items: None,
        url: "https://example.test/file".into(),
        location: Some("Safety".into()),
        thumbnail: Some("/thumbnail?v=1".into()),
        extension: Some(".pdf".into()),
        shortcut: false,
        unavailable: false,
    };
    DocumentsFolder {
        grant_generation: "a".repeat(64),
        id: Some("safety".into()),
        name: "Safety".into(),
        url: "https://example.test/safety".into(),
        path: vec![FolderStep {
            id: "safety".into(),
            name: "Safety".into(),
        }],
        items: vec![
            file.clone(),
            DocumentsItem {
                id: "second".into(),
                ..file
            },
        ],
    }
}

#[test]
fn every_visible_response_field_and_its_scope_participate_in_the_revision() {
    let original = response();
    let revision = folder_revision("dsp_one", "root", Some("plan"), &original).unwrap();
    assert_eq!(revision.len(), 64);
    assert_eq!(
        folder_revision("dsp_one", "root", Some(" plan "), &original).unwrap(),
        revision
    );
    type FolderChange = (&'static str, fn(&mut DocumentsFolder));
    let changes: &[FolderChange] = &[
        ("grant generation", |f| f.grant_generation = "b".repeat(64)),
        ("folder id", |f| f.id = None),
        ("folder name", |f| f.name.push('!')),
        ("folder url", |f| f.url.push('!')),
        ("path id", |f| f.path[0].id.push('!')),
        ("path name", |f| f.path[0].name.push('!')),
        ("path length", |f| f.path.clear()),
        ("item id", |f| f.items[0].id.push('!')),
        ("name", |f| f.items[0].name.push('!')),
        ("kind", |f| f.items[0].kind = ItemKind::Image),
        ("size", |f| f.items[0].size = Some(11)),
        ("modified at", |f| f.items[0].modified_at.push('!')),
        ("current modifier", |f| f.items[0].modified_by = None),
        ("current adder", |f| f.items[0].added_by = None),
        ("immediate child count", |f| f.items[0].items = Some(2)),
        ("item url", |f| f.items[0].url.push('!')),
        ("search location", |f| f.items[0].location = None),
        ("thumbnail capability", |f| f.items[0].thumbnail = None),
        ("extension", |f| f.items[0].extension = None),
        ("shortcut", |f| f.items[0].shortcut = true),
        ("unavailable target", |f| f.items[0].unavailable = true),
        ("membership", |f| {
            f.items.pop();
        }),
        ("order", |f| f.items.swap(0, 1)),
    ];
    for (name, change) in changes {
        let mut changed = original.clone();
        change(&mut changed);
        assert_ne!(
            folder_revision("dsp_one", "root", Some("plan"), &changed).unwrap(),
            revision,
            "{name}"
        );
    }
    for (dsp, root, query) in [
        ("dsp_two", "root", Some("plan")),
        ("dsp_one", "other", Some("plan")),
        ("dsp_one", "root", Some("pdf")),
        ("dsp_one", "root", None),
    ] {
        assert_ne!(
            folder_revision(dsp, root, query, &original).unwrap(),
            revision
        );
    }
}

#[tokio::test]
async fn unchanged_still_projects_current_member_names_counts_shortcut_targets_and_search_locations()
 {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let mut c = context(&state, &dsp).await;
    let at = dsp.clone();
    let (connection, author) = state
        .run(move |db| {
            // The author's later removal must leave another DSP owner.
            testing::owner_of(db, &at, "remaining@example.test", "Remaining")?;
            Ok((
                db.documents_connection(&at)?.unwrap(),
                testing::owner_of(db, &at, "author@example.test", "Avery")?,
            ))
        })
        .await
        .unwrap();
    let root = &connection.folder_id;
    let safety = item("safety", "Safety", Some(root), true, "2026-11-01");
    let target = item("target", "Plan.pdf", None, false, "2026-11-01");
    let shortcut = Item {
        mime_type: drive::SHORTCUT.into(),
        shortcut_details: Some(drive::Shortcut {
            target_id: "target".into(),
            target_mime_type: "application/pdf".into(),
        }),
        ..item(
            "shortcut",
            "Shortcut.pdf",
            Some("safety"),
            false,
            "2026-11-01",
        )
    };
    let key = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap()
        .key;
    let mirror = mirror::of(&key);
    mirror.replace(
        root,
        mirror.confirmation(),
        vec![safety.clone(), target.clone(), shortcut],
        "0".into(),
    );
    let at = dsp.clone();
    let by = author.clone();
    state
        .run(move |db| {
            db.record_documents_change(&at, "shortcut", &by)?;
            // Provenance belongs to the shortcut, never to its external target.
            db.record_documents_change_by(
                &at,
                "target",
                &Author::Named("Wrong target author".into()),
            )
        })
        .await
        .unwrap();
    let read = |c, since| {
        folder_read(
            &state,
            c,
            Dsp("documents.use"),
            None,
            Some("plan".into()),
            since,
        )
    };
    let first = read(c.clone(), None).await.unwrap();
    let full = first.folder.as_ref().unwrap();
    assert_eq!(full.items.len(), 1);
    assert_eq!(full.items[0].name, "Plan.pdf");
    assert_eq!(full.items[0].location.as_deref(), Some("Safety"));
    assert_eq!(full.items[0].added_by.as_deref(), Some("Avery Owner"));
    assert!(full.items[0].shortcut);
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(folder(&state, &c, None, Some("plan".into())).await.unwrap()).unwrap()
    );
    assert!(
        read(c.clone(), Some(first.revision.clone()))
            .await
            .unwrap()
            .folder
            .is_none()
    );

    let (at, by) = (dsp.clone(), author.clone());
    state
        .run(move |db| {
            testing::set_user_name(db, Some(&at), &by, "Renamed", "Owner")?;
            testing::set_user_status(db, Some(&at), &by, "disabled")
        })
        .await
        .unwrap();
    let renamed = read(c.clone(), Some(first.revision)).await.unwrap();
    assert_eq!(
        renamed.folder.as_ref().unwrap().items[0]
            .added_by
            .as_deref(),
        Some("Renamed Owner")
    );
    assert_eq!(
        renamed.folder.as_ref().unwrap().items[0]
            .modified_by
            .as_deref(),
        Some("Renamed Owner")
    );
    let (at, by, actor) = (dsp.clone(), author, c.clone());
    c = state
        .run(move |db| {
            let member = db
                .members(&at)?
                .into_iter()
                .find(|member| member.user_id == by)
                .expect("the author's membership");
            db.set_role(&actor, &member.id, None)?;
            // Removal changes the DSP revision; the next read opens its current view.
            db.context(&actor.auth, &at, "documents.use")
        })
        .await
        .unwrap();
    let removed = read(c.clone(), Some(renamed.revision)).await.unwrap();
    assert!(removed.folder.as_ref().unwrap().items[0].added_by.is_none());

    let at = dsp.clone();
    let support = state
        .run(move |db| {
            let user = db.create_user(
                None,
                "support@example.test",
                "Support",
                "Owner",
                "test-password-long",
            )?;
            db.record_documents_change(&at, "shortcut", &user.id)?;
            Ok(user.id)
        })
        .await
        .unwrap();
    let supported = read(c.clone(), Some(removed.revision)).await.unwrap();
    assert_eq!(
        supported.folder.as_ref().unwrap().items[0]
            .modified_by
            .as_deref(),
        Some("Platform support")
    );
    state
        .run(move |db| testing::revoke_platform_owner(db, &support))
        .await
        .unwrap();
    let no_support = read(c.clone(), Some(supported.revision)).await.unwrap();
    assert!(
        no_support.folder.as_ref().unwrap().items[0]
            .modified_by
            .is_none()
    );

    let mut renamed_folder = safety.clone();
    renamed_folder.name = "Compliance".into();
    mirror.put(root, renamed_folder);
    let relocated = read(c.clone(), Some(no_support.revision)).await.unwrap();
    assert_eq!(
        relocated.folder.as_ref().unwrap().items[0]
            .location
            .as_deref(),
        Some("Compliance")
    );
    let mut changed_target = target;
    changed_target.name = "New plan.pdf".into();
    mirror.put(root, changed_target);
    let changed = read(c.clone(), Some(relocated.revision)).await.unwrap();
    assert_eq!(
        changed.folder.as_ref().unwrap().items[0].name,
        "New plan.pdf"
    );

    let top = folder_read(&state, c.clone(), Dsp("documents.use"), None, None, None)
        .await
        .unwrap();
    assert_eq!(top.folder.as_ref().unwrap().items[0].items, Some(1));
    mirror.put(
        root,
        item("child", "Child.pdf", Some("safety"), false, "2026-11-01"),
    );
    let counted = folder_read(
        &state,
        c.clone(),
        Dsp("documents.use"),
        None,
        None,
        Some(top.revision),
    )
    .await
    .unwrap();
    assert_eq!(counted.folder.as_ref().unwrap().items[0].items, Some(2));
    let inside = folder_read(
        &state,
        c.clone(),
        Dsp("documents.use"),
        Some("safety".into()),
        None,
        None,
    )
    .await
    .unwrap();
    mirror.follow(
        root,
        mirror.confirmation(),
        drive::Changes {
            changed: vec![drive::Changed::Gone("target".into())],
            next: "1".into(),
        },
    );
    let missing = folder_read(
        &state,
        c,
        Dsp("documents.use"),
        Some("safety".into()),
        None,
        Some(inside.revision),
    )
    .await
    .unwrap();
    assert!(
        missing
            .folder
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == "shortcut")
            .unwrap()
            .unavailable
    );
}

#[tokio::test]
async fn full_and_unchanged_refuse_permission_loss_disconnect_or_broken_connection_during_await() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for reader in ["folder", "query", "describe"] {
        for loss in ["permission", "disconnect", "broken"] {
            for conditional in [false, true] {
                let (_root, state, dsp) = platform();
                connect(&state, &dsp).await;
                let owner = context(&state, &dsp).await;
                let (at, actor) = (dsp.clone(), owner.clone());
                let c = state
                    .run(move |db| {
                        let role = db.create_role(&actor, "Reader", &["documents.use".into()])?;
                        let mut auth = actor.auth.clone();
                        auth.preview = Some(role.id);
                        db.context(&auth, &at, "documents.use")
                    })
                    .await
                    .unwrap();
                let first = folder_read(&state, c.clone(), Dsp("documents.use"), None, None, None)
                    .await
                    .unwrap();
                let scope = crate::api::types::DocumentsQueryScope {
                    id: None,
                    q: None,
                    filter: crate::api::types::DocumentsFilter::All,
                };
                let baseline = query(
                    &state,
                    c.clone(),
                    Dsp("documents.use"),
                    QueryRequest {
                        scope: scope.clone(),
                        page: 0,
                        size: 100,
                        snapshot: None,
                        manifest_since: None,
                        since: None,
                    },
                )
                .await
                .unwrap();
                let (snapshot, manifest_revision, content_revision) = match baseline {
                    crate::api::types::DocumentsQueryRead::Full {
                        reading,
                        content_revision,
                        ..
                    } => (
                        reading.snapshot,
                        reading.manifest_revision,
                        content_revision,
                    ),
                    other => panic!("expected opening, got {other:?}"),
                };
                let at = dsp.clone();
                let refresh = state
                    .read(move |db| Ok(db.documents_refresh_token(&at)?.unwrap()))
                    .await
                    .unwrap();
                // Force an awaited cold refresh through the existing fixture-provider boundary.
                mirror::forget(&state.config.environment_root(), &dsp);
                cache::forget(&state.config.environment_root(), &dsp);
                let entered = Arc::new(AtomicBool::new(false));
                let observed = Arc::clone(&entered);
                let (config, key, at, role) = (
                    state.config.clone(),
                    state.key.clone(),
                    dsp.clone(),
                    c.role.clone(),
                );
                google::fixture_on_renewal(&refresh, move || {
                    let db = Store::open(config, key).unwrap();
                    match loss {
                        "permission" => {
                            db.update_role(&owner, &role, "Reader", &[]).unwrap();
                        }
                        "disconnect" => {
                            db.remove_documents_connection(&at).unwrap();
                        }
                        "broken" => {
                            db.break_documents_connection(&at).unwrap();
                        }
                        _ => unreachable!(),
                    }
                    observed.store(true, Ordering::SeqCst);
                });
                let refused = match reader {
                    "folder" => folder_read(
                        &state,
                        c,
                        Dsp("documents.use"),
                        None,
                        None,
                        conditional.then_some(first.revision),
                    )
                    .await
                    .map(|_| ()),
                    "query" => query(
                        &state,
                        c,
                        Dsp("documents.use"),
                        QueryRequest {
                            scope,
                            page: 0,
                            size: 100,
                            snapshot: conditional.then_some(snapshot),
                            manifest_since: Some(manifest_revision),
                            since: conditional.then_some(content_revision),
                        },
                    )
                    .await
                    .map(|_| ()),
                    "describe" => describe(
                        &state,
                        c,
                        Dsp("documents.use"),
                        scope,
                        snapshot,
                        vec!["valid-but-absent".into()],
                    )
                    .await
                    .map(|_| ()),
                    _ => unreachable!(),
                }
                .unwrap_err();
                assert!(
                    entered.load(Ordering::SeqCst),
                    "fixture did not reach the awaited boundary"
                );
                assert_eq!(
                    refused.code,
                    match loss {
                        "permission" => "permission_denied",
                        "disconnect" => "documents_not_connected",
                        "broken" => "google_connection_broken",
                        _ => unreachable!(),
                    }
                );
            }
        }
    }
}
