//! Documents's storage, against a real store in a temporary directory.
use super::*;
use dispatch_core::testing;

fn connection(email: &str) -> Connection {
    Connection {
        generation: String::new(),
        broken: false,
        full: true,
        account: Account {
            email: email.into(),
            workspace: false,
        },
        folder_id: "folder-1".into(),
        folder_name: "Test Documents".into(),
        connected_by: "user_1".into(),
        connected_at: iso(),
    }
}

// The state Google carries back is the only proof the browser coming back is the one that
// left: it answers once, to whoever started it.
#[test]
fn a_sign_in_comes_back_once_to_whoever_started_it() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    store
        .start_documents_sign_in(&dsp, "user_a", "state-a", "verifier-a")
        .unwrap();
    let refused = store
        .finish_documents_sign_in(&dsp, "user_b", "state-a")
        .unwrap_err();
    assert_eq!(refused.code, "documents_connect_expired");
    assert_eq!(
        store
            .finish_documents_sign_in(&dsp, "user_a", "state-a")
            .unwrap(),
        "verifier-a"
    );
    let replayed = store
        .finish_documents_sign_in(&dsp, "user_a", "state-a")
        .unwrap_err();
    assert_eq!(replayed.code, "documents_connect_expired");
}

// The refresh token opens the account's Drive: it is kept encrypted, never as written.
#[test]
fn keeps_the_refresh_token_encrypted_until_the_connection_goes() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    let kept = connection("documents@example.test");
    let kept = store
        .save_documents_connection(&dsp, &kept, "refresh-secret")
        .unwrap();
    let file = store.area(&dsp, "secrets").unwrap().join(SECRET);
    assert!(
        !std::fs::read_to_string(&file)
            .unwrap()
            .contains("refresh-secret")
    );
    assert_eq!(
        store.documents_refresh_token(&dsp).unwrap().as_deref(),
        Some("refresh-secret")
    );
    assert_eq!(
        store.documents_connection(&dsp).unwrap().as_ref(),
        Some(&kept)
    );
    // Ordinary reads and a restart-shaped reopen keep identity. Re-saving IDENTICAL visible
    // fields is a replacement; the storage boundary owns the new identity.
    assert_eq!(kept.generation.len(), 64);
    assert_eq!(
        Store::open(store.config.clone(), store.key.clone())
            .unwrap()
            .documents_connection(&dsp)
            .unwrap()
            .unwrap()
            .generation,
        kept.generation
    );
    let replacement = store
        .save_documents_connection(&dsp, &kept, "refresh-secret")
        .unwrap();
    assert_ne!(replacement.generation, kept.generation);
    assert_eq!(replacement.folder_id, kept.folder_id);
    // A previous binary omits the new column on replacement. Its trigger must still revoke
    // the old local identity, even when every visible field and the refresh token is equal.
    store
        .dsp(&dsp)
        .unwrap()
        .exec(
            "INSERT OR REPLACE INTO documents_connection \
             (id,status,account_email,account_kind,folder_id,folder_name,connected_by,\
             connected_at,broken_at,full_access) \
             SELECT id,status,account_email,account_kind,folder_id,folder_name,connected_by,\
             connected_at,broken_at,full_access FROM documents_connection WHERE id=1",
            [],
        )
        .unwrap();
    let legacy = store.documents_connection(&dsp).unwrap().unwrap();
    assert_eq!(legacy.generation.len(), 64);
    assert!(
        legacy
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_ne!(legacy.generation, replacement.generation);
    let mut expected = replacement;
    expected.generation = legacy.generation.clone();
    assert_eq!(legacy, expected);
    assert_eq!(
        store.documents_refresh_token(&dsp).unwrap().as_deref(),
        Some("refresh-secret")
    );
    // Failure to publish the encrypted secret leaves a durably broken replacement, never
    // a connected old generation. Reconnecting through the normal writer restores it.
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(
        store
            .save_documents_connection(&dsp, &legacy, "replacement-secret")
            .is_err()
    );
    let refused = store.documents_connection(&dsp).unwrap().unwrap();
    assert!(refused.broken);
    assert_ne!(refused.generation, legacy.generation);
    std::fs::remove_dir(&file).unwrap();
    let resumed = store
        .save_documents_connection(&dsp, &legacy, "refresh-secret")
        .unwrap();
    // Broken once, so the audit log says so once.
    assert!(store.break_documents_connection(&dsp).unwrap());
    assert!(!store.break_documents_connection(&dsp).unwrap());
    let broken = store.documents_connection(&dsp).unwrap().unwrap();
    assert!(broken.broken);
    assert_eq!(broken.generation, resumed.generation);
    store.remove_documents_connection(&dsp).unwrap();
    assert_eq!(store.documents_connection(&dsp).unwrap(), None);
    assert_eq!(store.documents_refresh_token(&dsp).unwrap(), None);
    assert!(!file.exists());
}

#[test]
fn provenance_is_read_only_for_requested_files_once_and_with_both_authors() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    store
        .record_documents_change(&dsp, "one", "user_a")
        .unwrap();
    store
        .record_documents_change_by(&dsp, "one", &Author::Named("Outside".into()))
        .unwrap();
    store
        .record_documents_change(&dsp, "elsewhere", "user_b")
        .unwrap();
    let records = store
        .documents_records(&dsp, &["one".into(), "missing".into(), "one".into()])
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records["one"].added_by, Author::Member("user_a".into()));
    assert_eq!(records["one"].changed_by, Author::Named("Outside".into()));
    assert!(store.documents_records(&dsp, &[]).unwrap().is_empty());
}
