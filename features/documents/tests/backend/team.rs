//! Whether the folder needs sharing again, from the team and who it's shared with; and a DSP
//! disabled while it's shared, which Google is asked nothing more for.
use super::*;
use crate::backend::{
    drive,
    support::{connect, platform},
};
use dispatch_core::{Code, testing};

/// The account fixture mode's Google signs in as, holding the DSP's Documents.
const ACCOUNT: &str = "fixture:documents@example.test";

fn member(user: &str, email: &str) -> Member {
    Member {
        user: user.into(),
        email: email.into(),
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

/// Adds `first` at `email` to the DSP as one of its owners, who use Documents.
async fn join(state: &Arc<State>, dsp: &str, first: &str, email: &str) {
    let (at, first, email) = (dsp.to_owned(), first.to_owned(), email.to_owned());
    state
        .run(move |db| testing::owner_of(db, &at, &email, &first))
        .await
        .unwrap();
}
/// The DSP's main folder.
async fn folder(state: &Arc<State>, dsp: &str) -> String {
    let at = dsp.to_owned();
    let connection = state.read(move |db| db.documents_connection(&at));
    connection.await.unwrap().unwrap().folder_id
}
/// Who Google says the file or folder `id` is shared with, beside the account.
async fn shared_with(state: &Arc<State>, id: &str) -> Vec<String> {
    let google = Google::of(&state.config).unwrap();
    let mut emails: Vec<String> = google
        .shares(ACCOUNT, id)
        .await
        .unwrap()
        .into_iter()
        .filter(|share| share.role != "owner")
        .filter_map(|share| share.email_address)
        .collect();
    emails.sort();
    emails
}
/// Disables the DSP, as the platform owner does, when it's called.
fn disabling(state: &Arc<State>, dsp: &str) -> impl FnOnce() + Send + 'static {
    let (config, key, dsp) = (state.config.clone(), state.key.clone(), dsp.to_owned());
    move || {
        let db = Store::open(config, key).unwrap();
        testing::set_dsp_status(&db, &dsp, "suspended").unwrap();
    }
}
async fn enable(state: &Arc<State>, dsp: &str) {
    let at = dsp.to_owned();
    state
        .run(move |db| testing::set_dsp_status(db, &at, "active"))
        .await
        .unwrap();
}

// Disabled while the folder is shared with the team, the DSP's folder is shared with no one
// more, and who Google already shared it with is kept: once enabled, the next sync goes on.
#[tokio::test]
async fn a_dsp_disabled_between_two_members_shares_shares_the_folder_with_no_one_more() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    join(&state, &dsp, "Avery", "avery@example.test").await;
    join(&state, &dsp, "Blake", "blake@example.test").await;
    let folder = folder(&state, &dsp).await;
    // Disabled once the folder is shared with Avery, the first of the team.
    drive::fixture_on_share(&folder, disabling(&state, &dsp));
    let refused = sync(&state, &dsp).await.unwrap_err();
    assert!(refused.is(Code::DspUnavailable));
    assert_eq!(shared_with(&state, &folder).await, ["avery@example.test"]);
    let at = dsp.clone();
    let people = state
        .read(move |db| db.documents_people(&at))
        .await
        .unwrap();
    let kept: Vec<_> = people
        .iter()
        .filter_map(|person| person.shared.clone())
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].0, "avery@example.test");
    enable(&state, &dsp).await;
    sync(&state, &dsp).await.unwrap();
    assert_eq!(
        shared_with(&state, &folder).await,
        ["avery@example.test", "blake@example.test"]
    );
}

// Disabled while a file someone else owns is shared with the team, it's shared with no one
// more, and the share Google already made is kept, so Dispatch can take it back.
#[tokio::test]
async fn a_dsp_disabled_between_two_file_shares_shares_the_file_with_no_one_more() {
    let (_root, state, dsp) = platform();
    connect(&state, &dsp).await;
    join(&state, &dsp, "Avery", "avery@example.test").await;
    join(&state, &dsp, "Blake", "blake@example.test").await;
    let folder = folder(&state, &dsp).await;
    // A Sheet someone else owns, which Documents holds a shortcut to.
    let sheet = drive::fixture_hidden(ACCOUNT)
        .unwrap()
        .into_iter()
        .find(|item| item.name == drive::SHARED_SHEET)
        .unwrap();
    let google = Google::of(&state.config).unwrap();
    google.shortcut(ACCOUNT, &sheet, &folder).await.unwrap();
    // Disabled once the Sheet is shared with Avery, the first of the team.
    drive::fixture_on_share(&sheet.id, disabling(&state, &dsp));
    let refused = sync(&state, &dsp).await.unwrap_err();
    assert!(refused.is(Code::DspUnavailable));
    assert_eq!(shared_with(&state, &sheet.id).await, ["avery@example.test"]);
    let at = dsp.clone();
    let made = state
        .read(move |db| db.documents_shared_files(&at))
        .await
        .unwrap();
    let made: Vec<_> = made
        .iter()
        .map(|shared| (&*shared.file, &*shared.email))
        .collect();
    assert_eq!(made, [(&*sheet.id, "avery@example.test")]);
    enable(&state, &dsp).await;
    sync(&state, &dsp).await.unwrap();
    assert_eq!(
        shared_with(&state, &sheet.id).await,
        ["avery@example.test", "blake@example.test"]
    );
}
