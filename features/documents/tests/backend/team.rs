//! Whether the folder needs sharing again, from the team and who it's shared with; what Google
//! takes back from a member who stops using Documents, and of a shortcut taken out; and a DSP
//! disabled while it's shared, which Google is asked nothing more for.
use super::*;
use crate::backend::{
    drive, files,
    support::{ACCOUNT, connect, context, disabling, enable, platform},
};
use dispatch_core::{
    Code,
    accounts::{Auth, api::types::PublicUser},
    foundation::config::Site,
    testing,
};

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

/// Adds `first` at `email` to the DSP as one of its owners, who use Documents: their user ID.
async fn join(state: &Arc<State>, dsp: &str, first: &str, email: &str) -> String {
    let (at, first, email) = (dsp.to_owned(), first.to_owned(), email.to_owned());
    state
        .run(move |db| testing::owner_of(db, &at, &email, &first))
        .await
        .unwrap()
}
/// How the folder is shared with the member `user`, at `email`, as Documents tells them.
async fn mine_of(state: &Arc<State>, dsp: &str, user: &str, email: &str) -> MySharing {
    let auth = Auth {
        user: PublicUser {
            id: user.into(),
            email: email.into(),
            first_name: "Avery".into(),
            last_name: "Owner".into(),
            platform_owner: false,
        },
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: Site::Admin,
        scope: Some(dsp.into()),
    };
    let at = dsp.to_owned();
    state
        .read(move |db| mine(db, &db.context(&auth, &at, "documents.use")?))
        .await
        .unwrap()
        .unwrap()
}
/// The DSP's main folder.
async fn folder(state: &Arc<State>, dsp: &str) -> String {
    let at = dsp.to_owned();
    let connection = state.read(move |db| db.documents_connection(&at));
    connection.await.unwrap().unwrap().folder_id
}
/// An account of fixture mode's Google that no other test signs in as, so the requests a
/// test counts are its own.
fn own_account() -> String {
    format!(
        "fixture:sharing-{}@example.test",
        dispatch_core::foundation::crypto::token()
            .unwrap()
            .to_ascii_lowercase()
    )
}
/// Who Google says `account`'s file or folder `id` is shared with, beside the account.
fn shared_with(account: &str, id: &str) -> Vec<String> {
    let mut emails: Vec<String> = drive::fixture::fixture(account, |drive| Ok(drive.shares(id)))
        .unwrap()
        .into_iter()
        .filter(|share| share.role != "owner")
        .filter_map(|share| share.email_address)
        .collect();
    emails.sort();
    emails
}
/// Has `then` happen once Google next shares `id` or takes a share of it back: before
/// Dispatch asks Google for anything more, or with `inside`, while its next request to Google
/// waits to be let through.
fn on_sharing(account: &str, id: &str, inside: bool, then: impl FnOnce() + Send + 'static) {
    if !inside {
        return drive::fixture_on_sharing(id, then);
    }
    let account = account.to_owned();
    drive::fixture_on_sharing(id, move || {
        drive::fixture::fixture(&account, |remote| {
            remote
                .holds
                .insert(remote.requests + 1, Box::pin(async move { then() }));
            Ok(())
        })
        .unwrap();
    });
}

// A member who no longer holds Use Documents loses the folder and the files Dispatch shared
// with them in Google, and a file whose shortcut is taken out of Documents is shared with no
// one Dispatch shared it with.
#[tokio::test]
async fn google_takes_back_the_shares_of_a_member_who_leaves_documents_and_of_a_removed_shortcut() {
    let (_root, state, dsp) = platform();
    let account = own_account();
    crate::backend::support::connect_as(&state, &dsp, &account).await;
    join(&state, &dsp, "Avery", "avery@example.test").await;
    let blake = join(&state, &dsp, "Blake", "blake@example.test").await;
    let folder = folder(&state, &dsp).await;
    // A Sheet someone else owns, which Documents holds a shortcut to.
    let sheet = drive::fixture_hidden(&account)
        .unwrap()
        .into_iter()
        .find(|item| item.name == drive::SHARED_SHEET)
        .unwrap();
    let owner = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap();
    let google = crate::backend::access::open_for(&state, &owner, false)
        .await
        .unwrap()
        .google;
    let shortcut = google.shortcut(&account, &sheet, &folder).await.unwrap();
    sync(&state, &dsp).await.unwrap();
    let team = ["avery@example.test", "blake@example.test"];
    assert_eq!(shared_with(&account, &folder), team);
    assert_eq!(shared_with(&account, &sheet.id), team);

    // Blake's role changes to one without Use Documents.
    let (c, at) = (context(&state, &dsp).await, dsp.clone());
    state
        .run(move |db| {
            let role = db.create_role(&c, "Driver", &[])?;
            let member = db.members(&at)?.into_iter().find(|m| m.user_id == blake);
            db.set_role(&c, &member.unwrap().id, Some(&role.id))
        })
        .await
        .unwrap();
    sync(&state, &dsp).await.unwrap();
    assert_eq!(shared_with(&account, &folder), ["avery@example.test"]);
    assert_eq!(shared_with(&account, &sheet.id), ["avery@example.test"]);

    // The shortcut taken out of Documents.
    let c = context(&state, &dsp).await;
    files::trash(&state, c, Dsp("documents.use"), shortcut.id, None)
        .await
        .unwrap();
    sync(&state, &dsp).await.unwrap();
    assert!(shared_with(&account, &sheet.id).is_empty());
}

// Disabled while the folder is shared with the team, the DSP's folder is shared with no one
// more, and who Google already shared it with is kept: once enabled, the minute's check finds
// the member still to share with, who was never told they need a Google account.
#[tokio::test]
async fn a_dsp_disabled_between_two_members_shares_shares_the_folder_with_no_one_more() {
    for inside in [false, true] {
        let (_root, state, dsp) = platform();
        let account = own_account();
        crate::backend::support::connect_as(&state, &dsp, &account).await;
        join(&state, &dsp, "Avery", "avery@example.test").await;
        join(&state, &dsp, "Blake", "blake@example.test").await;
        let folder = folder(&state, &dsp).await;
        // Disabled once the folder is shared with Avery, the first of the team.
        on_sharing(&account, &folder, inside, disabling(&state, &dsp));
        let refused = sync(&state, &dsp).await.unwrap_err();
        assert!(refused.is(Code::DspUnavailable));
        assert_eq!(shared_with(&account, &folder), ["avery@example.test"]);
        let at = dsp.clone();
        let people = state
            .read(move |db| db.documents_people(&at))
            .await
            .unwrap();
        let kept: Vec<_> = people
            .iter()
            .map(|person| person.shared.as_ref().map(|(email, _)| &**email))
            .collect();
        assert_eq!(kept, [Some("avery@example.test")]);
        enable(&state, &dsp).await;
        let at = dsp.clone();
        assert!(state.read(move |db| stale(db, &at)).await.unwrap());
        sync(&state, &dsp).await.unwrap();
        assert_eq!(
            shared_with(&account, &folder),
            ["avery@example.test", "blake@example.test"]
        );
    }
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
    let owner = crate::backend::grant::capture(&state, &dsp, None)
        .await
        .unwrap();
    let google = crate::backend::access::open_for(&state, &owner, false)
        .await
        .unwrap()
        .google;
    google.shortcut(ACCOUNT, &sheet, &folder).await.unwrap();
    // Disabled once the Sheet is shared with Avery, the first of the team.
    drive::fixture_on_sharing(&sheet.id, disabling(&state, &dsp));
    let refused = sync(&state, &dsp).await.unwrap_err();
    assert!(refused.is(Code::DspUnavailable));
    assert_eq!(shared_with(ACCOUNT, &sheet.id), ["avery@example.test"]);
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
        shared_with(ACCOUNT, &sheet.id),
        ["avery@example.test", "blake@example.test"]
    );
}

// Disabled once the folder is taken back from a member's old address, before it's shared at
// the account they linked: once enabled, the minute's check shares it there, and until then
// they're told it's on its way.
#[tokio::test]
async fn a_dsp_disabled_between_a_members_two_addresses_shares_at_the_new_one_once_enabled() {
    for inside in [false, true] {
        let (_root, state, dsp) = platform();
        let account = own_account();
        crate::backend::support::connect_as(&state, &dsp, &account).await;
        let avery = join(&state, &dsp, "Avery", "avery@example.test").await;
        let folder = folder(&state, &dsp).await;
        sync(&state, &dsp).await.unwrap();
        assert_eq!(shared_with(&account, &folder), ["avery@example.test"]);
        let (at, user) = (dsp.clone(), avery.clone());
        state
            .run(move |db| db.link_documents_google(&at, &user, "avery.r@example.test"))
            .await
            .unwrap();
        on_sharing(&account, &folder, inside, disabling(&state, &dsp));
        let refused = sync(&state, &dsp).await.unwrap_err();
        assert!(refused.is(Code::DspUnavailable));
        assert!(shared_with(&account, &folder).is_empty());
        enable(&state, &dsp).await;
        let at = dsp.clone();
        assert!(state.read(move |db| stale(db, &at)).await.unwrap());
        let sharing = mine_of(&state, &dsp, &avery, "avery@example.test").await;
        assert_eq!(sharing.state, SharingState::Pending);
        sync(&state, &dsp).await.unwrap();
        assert_eq!(shared_with(&account, &folder), ["avery.r@example.test"]);
        let sharing = mine_of(&state, &dsp, &avery, "avery@example.test").await;
        assert_eq!(sharing.state, SharingState::Shared);
    }
}
