//! Import Files, against fixture mode's Slack and a stand-in for where the DSP keeps its files: a
//! file shared in a linked channel, since it was linked, is saved once, to its folder, as the
//! member who posted it; one elsewhere, from before, too large or not hosted by Slack isn't;
//! what can't be saved yet waits; and the hourly catch-up finds what Slack didn't tell of.
use crate::backend::{
    app,
    channel_links::storage::{Link, LinkStore},
    slack::Tokens,
    storage::{Member, SlackStore, Workspace},
    support,
};
use crate::integrations::import_files::{
    ID, LINKING, catch_up,
    storage::{ImportStore, Status},
};
use dispatch_core::{State, db::now, manifest::integrations::Heard, testing};
use dispatch_documents_contract::Author;
use serde_json::json;
use std::sync::Arc;

const TEAM: &str = "T0FIXTURE1";
const PHOTOS: &str = "C0FIXPHOTOS";
const DISPATCH: &str = "C0FIXDISPATCH";

/// The development seed's Northline Logistics, its workspace `TEAM` connected with Import Files
/// on, `#photos` linked to the folder `photos`, and the workspace's member who posts the van's
/// photo known by the address of Northline's member, Jordan. Answers the state, the DSP and
/// Jordan's user ID.
async fn importing() -> (impl Drop, Arc<State>, String, String) {
    support::install();
    let (root, store) = testing::seeded();
    let mut dsp = String::new();
    for id in store.kept_dsps().unwrap() {
        if store.find_dsp(&id).unwrap().name == "Northline Logistics" {
            dsp = id;
        }
    }
    let owner = store
        .members(&dsp)
        .unwrap()
        .into_iter()
        .find(|member| member.email == "member@dispatch.test")
        .expect("Northline's member")
        .user_id;
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let workspace = Workspace {
        team: TEAM.into(),
        name: "Operations".into(),
        broken: false,
        scopes: app::scopes()
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect(),
        bot_user: "U0FIXTUREBOT".into(),
        app: "A0FIXTURE".into(),
        connected_by: None,
        connected_at: "2026-10-16T12:00:00.000Z".into(),
        label: None,
    };
    let tokens = Tokens {
        access: format!("fixture-access:{TEAM}:test"),
        refresh: None,
        expires_at: Some(now() + 3_600_000),
    };
    let posting = Member {
        user: "U0FIXOWNER".into(),
        name: "Morgan Lane".into(),
        display_name: None,
        email: Some("Member@Dispatch.test".into()),
        phone: None,
        active: true,
        guest: false,
    };
    let (at, by) = (dsp.clone(), owner.clone());
    state
        .run(move |db| {
            db.save_slack_workspace(&at, &workspace, &tokens)?;
            db.keep_slack_members(&at, TEAM, &[posting])?;
            db.turn_on_slack_integration(&at, TEAM, ID, &by)?;
            db.save_slack_channel_link(&at, &LINKING, &link(PHOTOS, "photos", &by))
        })
        .await
        .unwrap();
    (root, state, dsp, owner)
}
/// `channel` linked to the folder `folder-<folder>` a minute ago.
fn link(channel: &str, folder: &str, by: &str) -> Link {
    Link {
        team: TEAM.into(),
        channel: channel.into(),
        channel_name: channel.into(),
        private: false,
        folder: format!("folder-{folder}"),
        folder_name: folder.into(),
        linked_by: by.into(),
        linked_at: now() - 60_000,
    }
}
/// Slack telling of the file `file` shared in `channel`, `ago` milliseconds ago.
async fn shared(state: &Arc<State>, dsp: &str, file: &str, channel: &str, ago: i64) {
    let seconds = (now() - ago) as f64 / 1000.0;
    let heard = Heard {
        dsp: dsp.into(),
        account: TEAM.into(),
        kind: "file_shared".into(),
        event: json!({"type": "file_shared", "file_id": file, "channel_id": channel,
            "event_ts": format!("{seconds:.6}")}),
    };
    super::heard(state.clone(), heard).await;
}
/// How each file noticed in the workspace went, by its ID.
async fn noticed(state: &Arc<State>, dsp: &str) -> Vec<(String, Status)> {
    let at = dsp.to_owned();
    let mut noticed: Vec<_> = state
        .read(move |db| db.recent_slack_imports(&at, TEAM, 50))
        .await
        .unwrap()
        .into_iter()
        .map(|import| (import.file, import.status))
        .collect();
    noticed.sort_by(|a, b| a.0.cmp(&b.0));
    noticed
}

#[tokio::test]
async fn a_file_shared_in_a_linked_channel_is_saved_once_to_its_folder_as_who_posted_it() {
    let (_root, state, dsp, owner) = importing().await;

    // The van's photo, shared in #photos, goes to its folder, as the member Morgan is.
    shared(&state, &dsp, "F0FIXVAN", PHOTOS, 0).await;
    let van = (
        dsp.clone(),
        "folder-photos".to_owned(),
        "Van 12 damage.jpg".to_owned(),
        2_048,
        Author::Member(owner.clone()),
    );
    assert_eq!(support::kept(&dsp), std::slice::from_ref(&van));
    // Told of again, as Slack does when it shares a file in two channels, it's saved once.
    shared(&state, &dsp, "F0FIXVAN", PHOTOS, 0).await;
    assert_eq!(support::kept(&dsp), std::slice::from_ref(&van));

    // Nothing from a channel that isn't linked, or from before #photos was.
    shared(&state, &dsp, "F0FIXROUTE", DISPATCH, 0).await;
    shared(&state, &dsp, "F0FIXCANVAS", PHOTOS, 60 * 60_000).await;
    // A video too large to keep, and a canvas, which Slack doesn't host as a file, are left.
    shared(&state, &dsp, "F0FIXDASHCAM", PHOTOS, 0).await;
    shared(&state, &dsp, "F0FIXCANVAS", PHOTOS, 0).await;
    assert_eq!(support::kept(&dsp), [van]);
    assert_eq!(
        noticed(&state, &dsp).await,
        [
            ("F0FIXCANVAS".to_owned(), Status::Skipped),
            ("F0FIXDASHCAM".to_owned(), Status::TooLarge),
            ("F0FIXVAN".to_owned(), Status::Saved),
        ]
    );

    // While its files can't be kept, a file waits, saying why; someone who isn't one of the
    // DSP's members is named as Slack shows them.
    support::ready(&dsp, false);
    let (at, by) = (dsp.clone(), owner.clone());
    state
        .run(move |db| db.save_slack_channel_link(&at, &LINKING, &link(DISPATCH, "routes", &by)))
        .await
        .unwrap();
    shared(&state, &dsp, "F0FIXROUTE", DISPATCH, 0).await;
    let at = dsp.clone();
    let waiting = state
        .read(move |db| db.recent_slack_imports(&at, TEAM, 1))
        .await
        .unwrap();
    assert_eq!(waiting[0].status, Status::Waiting);
    assert_eq!(waiting[0].reason.as_deref(), Some("Not ready"));
    support::ready(&dsp, true);
}

#[tokio::test]
async fn the_hourly_catch_up_saves_what_slack_did_not_tell_of() {
    let (_root, state, dsp, owner) = importing().await;
    catch_up::catch_up(&state, &dsp).await.unwrap();
    let saved: Vec<_> = support::kept(&dsp)
        .into_iter()
        .map(|(_, folder, name, _, by)| (folder, name, by))
        .collect();
    assert_eq!(
        saved,
        [(
            "folder-photos".to_owned(),
            "Van 12 damage.jpg".to_owned(),
            Author::Member(owner),
        )]
    );
    // Run again, it saves nothing twice.
    catch_up::catch_up(&state, &dsp).await.unwrap();
    assert_eq!(support::kept(&dsp).len(), 1);
    // Switched off in the workspace, nothing it lists is noticed.
    let at = dsp.clone();
    state
        .run(move |db| {
            db.turn_off_slack_integration(&at, TEAM, ID, "user_1")
                .map(drop)
        })
        .await
        .unwrap();
    let before = noticed(&state, &dsp).await;
    catch_up::catch_up(&state, &dsp).await.unwrap();
    assert_eq!(noticed(&state, &dsp).await, before);
}

// Removing a workspace clears every table Slack declares by its team, an integration's as well
// as Slack's own, without the removal naming them.
#[tokio::test]
async fn a_removed_workspace_takes_its_linked_channels_and_files_with_it() {
    let (_root, state, dsp, _) = importing().await;
    shared(&state, &dsp, "F0FIXVAN", PHOTOS, 0).await;
    assert!(!noticed(&state, &dsp).await.is_empty());
    let at = dsp.clone();
    state
        .run(move |db| db.remove_slack_workspace(&at, TEAM))
        .await
        .unwrap();
    assert!(noticed(&state, &dsp).await.is_empty());
    let at = dsp.clone();
    let links = state
        .read(move |db| db.slack_channel_links(&at, &LINKING, TEAM))
        .await
        .unwrap();
    assert!(links.is_empty());
}
