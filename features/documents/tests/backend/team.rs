//! Whether the folder needs sharing again, from the team and who it's shared with.
use super::*;

fn member(user: &str, email: &str) -> Member {
    Member {
        user: user.into(),
        name: user.into(),
        email: email.into(),
        owner: false,
    }
}
fn shared(user: &str, email: &str) -> Person {
    Person {
        user: user.into(),
        linked: None,
        shared: Some((email.into(), format!("share-{user}"))),
        sharing: Sharing::Shared,
        emailed_at: None,
    }
}

// The minute's check asks nothing of Google: it compares the team with who the folder was
// shared with, and only a difference sends Dispatch to Google.
#[test]
fn the_folder_needs_sharing_again_only_when_the_team_changed() {
    let team = [member("dana", "dana@example.com")];
    assert!(changed(&team, &[]), "Dana joined");
    assert!(!changed(&team, &[shared("dana", "Dana@example.com")]));
    assert!(
        changed(&[], &[shared("dana", "dana@example.com")]),
        "Dana left"
    );
    assert!(
        changed(&team, &[shared("dana", "old@example.com")]),
        "Dana's Dispatch email changed"
    );
    let linked = Person {
        linked: Some("dana.r@example.com".into()),
        ..shared("dana", "dana.r@example.com")
    };
    assert!(
        !changed(&team, &[linked]),
        "shared at the account Dana linked"
    );
    let swapped = [shared("riley", "riley@example.com")];
    assert!(changed(&team, &swapped), "someone else in Dana's place");
    // Waiting on a Google account is no change: the hourly check tries again.
    let waiting = Person {
        shared: None,
        sharing: Sharing::NeedsAccount,
        ..shared("dana", "")
    };
    assert!(!changed(&team, &[waiting]));
}
