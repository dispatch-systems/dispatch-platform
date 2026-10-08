//! Who the folder should be shared with, against a real store in a temporary directory.
use super::*;
use dispatch_core::testing;

// The minute's check asks nothing of Google: it compares the team with who the folder was
// shared with, and only a difference sends Dispatch to Google.
#[test]
fn the_folder_needs_sharing_again_only_when_the_team_changed() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    let user = store
        .create_user(
            "dana@example.test",
            "Dana",
            "Reyes",
            "test-password-long",
            false,
        )
        .unwrap();
    store
        .platform
        .exec(
            "INSERT INTO memberships(id,user_id,dsp_id,role) VALUES ('mem_dana',?,?,'owner')",
            [&user.id, &dsp],
        )
        .unwrap();
    let owner = store.members(&dsp).unwrap().remove(0);
    let shared = |email: &str| Person {
        user: owner.user_id.clone(),
        linked: None,
        shared: Some((email.into(), "share-1".into())),
        sharing: Sharing::Shared,
        emailed_at: None,
    };
    // The owner holds Use Documents, and the folder isn't shared with them yet.
    assert!(stale(&store, &dsp).unwrap());
    store
        .save_documents_person(&dsp, &shared("Dana@example.test"))
        .unwrap();
    assert!(!stale(&store, &dsp).unwrap());

    // Shared at an address they moved on from.
    store
        .save_documents_person(&dsp, &shared("old@example.test"))
        .unwrap();
    assert!(stale(&store, &dsp).unwrap());
    let linked = Person {
        linked: Some("old@example.test".into()),
        ..shared("old@example.test")
    };
    store.save_documents_person(&dsp, &linked).unwrap();
    assert!(!stale(&store, &dsp).unwrap());

    // Someone no longer on the team.
    let gone = Person {
        user: "user_gone".into(),
        ..shared("gone@example.test")
    };
    store.save_documents_person(&dsp, &gone).unwrap();
    assert!(stale(&store, &dsp).unwrap());
    store.remove_documents_person(&dsp, "user_gone").unwrap();

    // Waiting on a Google account is no change: the hourly check tries again.
    let waiting = Person {
        shared: None,
        sharing: Sharing::NeedsAccount,
        ..shared("")
    };
    store.save_documents_person(&dsp, &waiting).unwrap();
    assert!(!stale(&store, &dsp).unwrap());
}
