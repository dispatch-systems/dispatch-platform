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

/// The Drive fixture runs the real traversal/continuation loops. Hold the second physical
/// page before admission; a final publication check cannot make this pass.
#[tokio::test]
async fn every_listing_and_change_page_rechecks_the_captured_member_and_connection_after_waiting() {
    use super::super::fixture::fixture;
    use crate::backend::{
        access, grant,
        storage::DocumentsStore,
        support::{connect_as, context, platform},
        tree::tests::item,
    };
    use dispatch_core::{foundation::crypto, server::http::route::Dsp, testing};

    for changes in [false, true] {
        for loss in [
            "none",
            "reconnect",
            "disconnect",
            "permission",
            "member",
            "feature",
            "suspended",
        ] {
            let (_root, state, dsp) = platform();
            let account = format!(
                "fixture:pages-{}@example.test",
                crypto::token().unwrap().to_ascii_lowercase()
            );
            connect_as(&state, &dsp, &account).await;
            let actor = context(&state, &dsp).await;
            let (at, owner) = (dsp.clone(), actor.clone());
            state
                .run(move |db| {
                    testing::owner_of(db, &at, "remaining@example.test", "Remaining")?;
                    let user = testing::owner_of(db, &at, "reader@example.test", "Reader")?;
                    let role = db.create_role(&owner, "Reader", &["documents.use".into()])?;
                    let membership = db
                        .members(&at)?
                        .into_iter()
                        .find(|m| m.user_id == user)
                        .unwrap();
                    db.set_role(&owner, &membership.id, Some(&role.id))?;
                    db.set_code(&at, "adm")?;
                    Ok(())
                })
                .await
                .unwrap();
            let site = dispatch_core::foundation::config::Site::Dsp("adm".into());
            let raw = state
                .login(
                    "reader@example.test".into(),
                    "test-password-long".into(),
                    "127.0.0.1".into(),
                    "test".into(),
                    dispatch_core::accounts::SessionLifetime::Standard,
                    site.clone(),
                )
                .await
                .unwrap();
            let at = dsp.clone();
            let c = state
                .read(move |db| {
                    let auth = db.authenticate(&raw, &site)?;
                    db.context(&auth, &at, "documents.use")
                })
                .await
                .unwrap();
            let role = c.role.clone();
            let captured = grant::capture_member(&state, c.clone(), Dsp("documents.use"), None)
                .await
                .unwrap();
            let drive = access::open_for(&state, &captured, false).await.unwrap();
            let folder = captured.connection.folder_id.clone();
            let from = drive.google.start_changes(&account).await.unwrap();
            fixture(&account, |remote| {
                for n in 0..1001 {
                    let id = format!("page-file-{n:04}");
                    remote.files.insert(
                        id.clone(),
                        item(&id, &id, Some(&folder), false, "2026-11-01"),
                    );
                    remote.touch(&id);
                }
                Ok(())
            })
            .unwrap();
            let (entered, waiting) = tokio::sync::oneshot::channel();
            let (release, released) = tokio::sync::oneshot::channel::<()>();
            let before = fixture(&account, |remote| {
                let before = remote.requests;
                remote.holds.insert(
                    before + 2,
                    Box::pin(async move {
                        entered.send(()).unwrap();
                        released.await.unwrap();
                    }),
                );
                Ok(before)
            })
            .unwrap();
            let (token, root) = (account.clone(), folder.clone());
            let work = tokio::spawn(async move {
                if changes {
                    drive
                        .google
                        .changes(&token, &from)
                        .await
                        .map(|answer| answer.changed.len())
                } else {
                    drive
                        .google
                        .inside(&token, &root)
                        .await
                        .map(|answer| answer.len())
                }
            });
            waiting.await.unwrap();
            assert_eq!(
                fixture(&account, |remote| Ok(remote.requests)).unwrap(),
                before + 1
            );
            let (at, original, member) = (
                dsp.clone(),
                captured.connection.clone(),
                c.auth.user.id.clone(),
            );
            state
                .run(move |db| {
                    match loss {
                        "none" => {}
                        "reconnect" => {
                            let refresh = db.documents_refresh_token(&at)?.unwrap();
                            let next = db.save_documents_connection(&at, &original, &refresh)?;
                            assert_ne!(next.generation, original.generation);
                        }
                        "disconnect" => db.remove_documents_connection(&at)?,
                        "permission" => {
                            db.update_role(&actor, &role, "Reader", &[])?;
                        }
                        "member" => {
                            let membership = db
                                .members(&at)?
                                .into_iter()
                                .find(|m| m.user_id == member)
                                .unwrap();
                            db.set_role(&actor, &membership.id, None)?;
                        }
                        "feature" => {
                            db.set_feature(&at, "documents", false, &testing::platform_owner(db))?;
                        }
                        "suspended" => testing::set_dsp_status(db, &at, "suspended")?,
                        _ => unreachable!(),
                    }
                    Ok(())
                })
                .await
                .unwrap();
            release.send(()).unwrap();
            let answer = work.await.unwrap();
            if loss == "none" {
                assert_eq!(answer.unwrap(), 1001);
                assert_eq!(
                    fixture(&account, |remote| Ok(remote.requests)).unwrap(),
                    before + 2
                );
            } else {
                assert!(
                    answer.is_err(),
                    "{loss}: an obsolete owner obtained a continuation page"
                );
                assert_eq!(
                    fixture(&account, |remote| Ok(remote.requests)).unwrap(),
                    before + 1,
                    "{loss}: the refused page must not reach fixture storage"
                );
            }
            if loss == "reconnect" {
                let current = grant::capture_member(&state, c, Dsp("documents.use"), None)
                    .await
                    .unwrap();
                let current = access::open_for(&state, &current, false).await.unwrap();
                assert_eq!(
                    current
                        .google
                        .inside(&account, &folder)
                        .await
                        .unwrap()
                        .len(),
                    1001
                );
            }
        }
    }
}
