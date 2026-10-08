//! Documents's storage, against a real store in a temporary directory.
use super::*;
use dispatch_core::testing;

fn connection(email: &str) -> Connection {
    Connection {
        broken: false,
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
    store
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
    assert_eq!(store.documents_connection(&dsp).unwrap(), Some(kept));
    // Broken once, so the audit log says so once.
    assert!(store.break_documents_connection(&dsp).unwrap());
    assert!(!store.break_documents_connection(&dsp).unwrap());
    assert!(store.documents_connection(&dsp).unwrap().unwrap().broken);
    store.remove_documents_connection(&dsp).unwrap();
    assert_eq!(store.documents_connection(&dsp).unwrap(), None);
    assert_eq!(store.documents_refresh_token(&dsp).unwrap(), None);
    assert!(!file.exists());
}
