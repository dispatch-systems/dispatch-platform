//! A DSP disabled while Google renews its access token, whose Drive is reached no more; and a
//! deleted DSP's token and pictures, forgotten, with none kept of a renewal or a folder shown as
//! it was deleted.
use super::*;
use crate::backend::{
    connection,
    description::Described,
    drive::DOC,
    files::make,
    google,
    storage::DocumentsStore,
    support::{ACCOUNT, connect, disabling, platform},
    tree::{Tree, tests::item},
};
use dispatch_core::{Code, testing};

#[tokio::test]
async fn a_lifecycle_change_during_renewal_cannot_admit_the_old_grant() {
    for reconnect in [false, true] {
        let (_root, state, dsp) = platform();
        connect(&state, &dsp).await;
        let grant = crate::backend::grant::capture(&state, &dsp, None)
            .await
            .unwrap();
        let at = dsp.clone();
        let refresh = state
            .read(move |db| Ok(db.documents_refresh_token(&at)?.unwrap()))
            .await
            .unwrap();
        let folder = grant.connection.folder_id.clone();
        if reconnect {
            let (config, key, at, before, token) = (
                state.config.clone(),
                state.key.clone(),
                dsp.clone(),
                grant.connection.clone(),
                refresh.clone(),
            );
            google::fixture_on_renewal(&refresh, move || {
                let db = dispatch_core::db::Store::open(config, key).unwrap();
                let replacement = db.save_documents_connection(&at, &before, &token).unwrap();
                assert_ne!(replacement.generation, before.generation);
                let mut same_visible_connection = replacement;
                same_visible_connection.generation = before.generation.clone();
                assert_eq!(same_visible_connection, before);
            });
        } else {
            google::fixture_on_renewal(&refresh, disabling(&state, &dsp));
        }
        let refused = make(
            &state,
            &grant,
            &folder,
            DOC,
            "Made under obsolete authority",
        )
        .await
        .unwrap_err();
        if reconnect {
            assert_eq!(refused.code, "documents_not_connected");
        } else {
            assert!(refused.is(Code::DspUnavailable));
        }
        assert!(
            kept(&grant.key).is_none(),
            "late renewal must not retain its token"
        );
        let made = crate::backend::drive::fixture::fixture(ACCOUNT, |drive| {
            Ok(drive.values().cloned().collect::<Vec<_>>())
        })
        .unwrap();
        assert!(
            !made
                .iter()
                .any(|item| item.name == "Made under obsolete authority")
        );

        if !reconnect {
            crate::backend::support::enable(&state, &dsp).await;
        }
        let current = crate::backend::grant::capture(&state, &dsp, None)
            .await
            .unwrap();
        assert_eq!(
            current.connection.generation == grant.connection.generation,
            !reconnect
        );
        if reconnect {
            assert!(!std::sync::Arc::ptr_eq(&current.mirror, &grant.mirror));
        }
        make(
            &state,
            &current,
            &folder,
            DOC,
            "Made under current authority",
        )
        .await
        .unwrap();
        assert!(kept(&current.key).is_some());
        assert!(
            crate::backend::access::open_for(&state, &current, false)
                .await
                .unwrap()
                .google
                .inside(ACCOUNT, &folder)
                .await
                .unwrap()
                .iter()
                .any(|item| item.name == "Made under current authority")
        );
    }
}

#[test]
fn a_deleted_dsps_token_and_pictures_are_forgotten_and_none_kept_after_and_no_other_dsps() {
    let (_root, state, _) = platform();
    let deleted = crate::backend::grant::Key {
        namespace: state.config.environment_root(),
        dsp: "dsp_deleted".into(),
        generation: "a".repeat(64),
    };
    let other = crate::backend::grant::Key {
        dsp: "dsp_other".into(),
        ..deleted.clone()
    };
    let same_dsp_elsewhere = crate::backend::grant::Key {
        namespace: deleted.namespace.join("independent-environment"),
        ..deleted.clone()
    };
    // Each DSP's access token and shown-file provenance, including the same DSP ID in
    // another environment: deleting this namespace must preserve that independent owner.
    let lasts = Instant::now() + Duration::from_secs(3600);
    for key in [&deleted, &other, &same_dsp_elsewhere] {
        keep(key, &format!("token:{}", key.dsp), lasts);
        let now = Instant::now();
        let shown = Shown {
            link: format!("fixture:{}", key.dsp),
            given: now,
            shown: now,
        };
        PICTURES
            .lock()
            .unwrap()
            .insert((key.clone(), "file".to_owned()), shown);
    }
    let tree = Tree::new("main", Vec::new());
    let (records, names) = (BTreeMap::new(), BTreeMap::new());
    let listing = Described {
        owner: &deleted,
        tree: &tree,
        records: &records,
        names: &names,
        account: "documents@example.test",
    };
    let mut listed = item("listed", "Listed.pdf", Some("main"), false, "2026-11-05");
    listed.thumbnail_link = Some("fixture:dsp_deleted:listed".into());
    connection::forgotten(&state, &deleted.dsp);
    listing.item(&Row::from(listed), false);
    keep(&deleted, "token:renewed", lasts);
    assert!(kept(&deleted).is_none());
    assert!(shown(&deleted, "file").is_none());
    assert!(shown(&deleted, "listed").is_none());
    assert_eq!(kept(&other).unwrap(), "token:dsp_other");
    assert_eq!(shown(&other, "file").unwrap().link, "fixture:dsp_other");
    assert_eq!(kept(&same_dsp_elsewhere).unwrap(), "token:dsp_deleted");
    assert_eq!(
        shown(&same_dsp_elsewhere, "file").unwrap().link,
        "fixture:dsp_deleted"
    );
    forget(&other.namespace, &other.dsp);
    forget(&same_dsp_elsewhere.namespace, &same_dsp_elsewhere.dsp);
}

#[tokio::test]
async fn a_delete_whose_request_went_still_forgets_the_dsps_token() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db) = testing::seeded();
    let dsp = testing::dsp_named(&db, "Northline Logistics");
    let owner = testing::platform_owner(&db);
    testing::set_dsp_status(&db, &dsp, "suspended").unwrap();
    let key = crate::backend::grant::Key {
        namespace: db.config.environment_root(),
        dsp: dsp.clone(),
        generation: "a".repeat(64),
    };
    keep(
        &key,
        "token:held",
        Instant::now() + Duration::from_secs(3600),
    );
    db.delete_dsp(&dsp, "Northline Logistics", &owner).unwrap();
    let config = db.config.clone();
    drop(db);
    // The request that deleted it went before finishing it; upkeep finishes it.
    let state = dispatch_core::State::new(config).unwrap();
    dispatch_core::tenancy::deletion::finish(&state).await;
    assert!(kept(&key).is_none());
}

#[tokio::test]
async fn an_unchanged_folder_renews_thumbnail_admission_without_renewing_its_provider_observation()
{
    use crate::backend::{files::folder_read, mirror, support::context};
    use dispatch_core::server::http::route::Dsp;
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    let c = context(&state, &dsp).await;
    let at = dsp.clone();
    let connection = state
        .read(move |db| Ok(db.documents_connection(&at)?.unwrap()))
        .await
        .unwrap();
    let given = Instant::now() - PICTURE_LINK_LASTS - Duration::from_secs(10);
    let mut target = item("target", "Plan.pdf", None, false, "2026-11-01");
    target.thumbnail_link = Some("fixture:target".into());
    target.thumbnail_version = Some("7".into());
    target.seen = given;
    let shortcut = crate::backend::drive::Item {
        mime_type: crate::backend::drive::SHORTCUT.into(),
        shortcut_details: Some(crate::backend::drive::Shortcut {
            target_id: "target".into(),
            target_mime_type: "application/pdf".into(),
        }),
        ..item(
            "shortcut",
            "Plan",
            Some(&connection.folder_id),
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
        &connection.folder_id,
        mirror.confirmation(),
        vec![shortcut, target],
        "0".into(),
    );
    let first = folder_read(&state, c.clone(), Dsp("documents.use"), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        first.folder.as_ref().unwrap().items[0].thumbnail.as_deref(),
        Some("/api/dsp/documents/items/target/thumbnail?v=7")
    );
    // Expire shown admission using the cache's actual std::Instant, with no sleep or timer.
    PICTURES
        .lock()
        .unwrap()
        .get_mut(&(key.clone(), "target".into()))
        .unwrap()
        .shown = Instant::now() - PICTURE_LINK_LASTS;
    assert!(shown(&key, "target").is_none());
    let again = folder_read(
        &state,
        c,
        Dsp("documents.use"),
        None,
        None,
        Some(first.revision.clone()),
    )
    .await
    .unwrap();
    assert!(again.folder.is_none());
    assert_eq!(again.revision, first.revision);
    let renewed = shown(&key, "target").expect("unchanged must renew the shown target");
    assert_eq!(renewed.given, given);
    assert!(shown(&key, "shortcut").is_none());
    use crate::api::types::{DocumentsFilter, DocumentsQueryRead, DocumentsQueryScope};
    use crate::backend::query::{QueryRequest, query};
    let query_context = context(&state, &dsp).await;
    let scope = DocumentsQueryScope {
        id: None,
        q: None,
        filter: DocumentsFilter::All,
    };
    let page = query(
        &state,
        query_context.clone(),
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
    let (reading, content_revision) = match page {
        DocumentsQueryRead::Full {
            reading,
            content_revision,
            ..
        } => (reading, content_revision),
        other => panic!("expected query opening: {other:?}"),
    };
    assert_eq!(
        reading.items[0].thumbnail.as_deref(),
        Some("/api/dsp/documents/items/target/thumbnail?v=7")
    );
    PICTURES
        .lock()
        .unwrap()
        .get_mut(&(key.clone(), "target".into()))
        .unwrap()
        .shown = Instant::now() - PICTURE_LINK_LASTS;
    assert!(shown(&key, "target").is_none());
    let page = query(
        &state,
        query_context,
        Dsp("documents.use"),
        QueryRequest {
            scope,
            page: 0,
            size: 100,
            snapshot: Some(reading.snapshot),
            manifest_since: Some(reading.manifest_revision),
            since: Some(content_revision),
        },
    )
    .await
    .unwrap();
    assert!(matches!(page, DocumentsQueryRead::Unchanged { .. }));
    assert_eq!(
        shown(&key, "target")
            .expect("unchanged page renews shown")
            .given,
        given
    );
    assert!(shown(&key, "shortcut").is_none());
}
