//! Sync Canvases, against fixture mode's Slack and a stand-in for where the DSP keeps its files:
//! a linked channel's canvas, or one chosen on its own, is written as a document with its
//! picture in it, then written over in place once it's edited, or anew once its document is
//! gone; a change Slack tells of waits for the changes to stop; and one gone from Slack is left.
use super::super::{
    ID, check,
    storage::{Canvas, Outcome, Status},
};
use super::*;
use crate::backend::{
    app,
    channel_links::storage::{Link, LinkStore},
    fixture::{self, CANVAS},
    slack::Tokens,
    storage::{Member, SlackStore, Workspace},
    support,
};
use dispatch_core::testing;
use dispatch_documents_contract::Author;

const TEAM: &str = "T0FIXTURE1";
/// The workspace whose canvas a test edits, which no other test reads.
const EDITING: &str = "T0FIXTURE2";
const PHOTOS: &str = "C0FIXPHOTOS";
const DISPATCH: &str = "C0FIXDISPATCH";

/// The development seed's Northline Logistics, its workspace `team` connected with Sync
/// Canvases on, and the workspace's member who made the canvas known by the address of
/// Northline's member. Answers the state, the DSP and that member's user ID.
async fn syncing(team: &str) -> (impl Drop, Arc<State>, String, String) {
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
        team: team.into(),
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
        access: format!("fixture-access:{team}:test"),
        refresh: None,
        expires_at: Some(now() + 3_600_000),
    };
    let maker = Member {
        user: "U0FIXOWNER".into(),
        name: "Morgan Lane".into(),
        display_name: None,
        email: Some("Member@Dispatch.test".into()),
        phone: None,
        active: true,
        guest: false,
    };
    let (at, by, of) = (dsp.clone(), owner.clone(), team.to_owned());
    state
        .run(move |db| {
            db.save_slack_workspace(&at, &workspace, &tokens)?;
            db.keep_slack_members(&at, &of, &[maker])?;
            db.turn_on_slack_integration(&at, &of, ID, &by)
        })
        .await
        .unwrap();
    (root, state, dsp, owner)
}
/// Links the workspace `team`'s `channel` to the folder `folder-<folder>`, for its canvases.
async fn link(state: &Arc<State>, dsp: &str, team: &str, channel: &str, folder: &str) {
    let link = Link {
        team: team.into(),
        channel: channel.into(),
        channel_name: channel.into(),
        private: false,
        folder: format!("folder-{folder}"),
        folder_name: folder.into(),
        linked_by: "user_1".into(),
        linked_at: now(),
    };
    let at = dsp.to_owned();
    state
        .run(move |db| db.save_slack_channel_link(&at, &super::super::LINKING, &link))
        .await
        .unwrap();
}
/// The canvas as Sync Canvases keeps it in the workspace `team`.
async fn kept(state: &Arc<State>, dsp: &str, team: &str, canvas: &str) -> Option<Canvas> {
    let (at, of, canvas) = (dsp.to_owned(), team.to_owned(), canvas.to_owned());
    state
        .read(move |db| db.slack_canvases(&at, &of))
        .await
        .unwrap()
        .into_iter()
        .find(|kept| kept.canvas == canvas)
}
/// Makes the workspace `team`'s canvas due now, as it is once the changes to it stop.
async fn due_now(state: &Arc<State>, dsp: &str, team: &str, canvas: &str) {
    let (at, of, canvas) = (dsp.to_owned(), team.to_owned(), canvas.to_owned());
    state
        .run(move |db| {
            db.note_slack_canvas(&at, &of, &canvas, PHOTOS, "Test", now(), now(), now())
                .map(drop)
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_linked_channels_canvas_is_written_with_its_picture_then_over_itself_once_edited() {
    let (_root, state, dsp, owner) = syncing(EDITING).await;
    link(&state, &dsp, EDITING, PHOTOS, "write-ups").await;

    // The check finds the canvas and writes it into the channel's folder, as the member who
    // made it, its photo in it and the member it mentions by the name the DSP knows them by.
    check::check(&state, &dsp).await.unwrap();
    let pages = support::pages(&dsp);
    assert_eq!(pages.len(), 1);
    let written = &pages[0];
    assert_eq!(
        (written.folder.as_str(), written.name.as_str()),
        ("folder-write-ups", "Write-ups")
    );
    assert_eq!(written.by, Author::Member(owner));
    assert!(
        written
            .html
            .contains("<img src=\"data:image/png;base64,iVBORw0KGgo")
    );
    let at = dsp.clone();
    let named = state
        .read(move |db| db.members(&at))
        .await
        .unwrap()
        .into_iter()
        .find(|member| Author::Member(member.user_id.clone()) == written.by)
        .expect("the member who made it")
        .name;
    assert!(written.html.contains(&format!("signed by @{named}")));
    let synced = kept(&state, &dsp, EDITING, CANVAS).await.unwrap();
    assert_eq!(synced.status, Status::Synced);
    assert_eq!(
        synced.doc.as_ref().map(|(id, _)| id.as_str()),
        Some(written.id.as_str())
    );
    // Checked again, unedited, it isn't written again.
    check::check(&state, &dsp).await.unwrap();
    assert_eq!(support::pages(&dsp)[0].writes, 1);

    // Edited, it's written over in place, by Slack, as Slack names no editor.
    fixture::edit_canvas(EDITING, "<h1>Write-ups</h1><p>Signed and filed</p>");
    due_now(&state, &dsp, EDITING, CANVAS).await;
    sync_due(&state, &dsp).await;
    let pages = support::pages(&dsp);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].writes, 2);
    assert!(pages[0].html.contains("<p>Signed and filed</p>"));
    assert_eq!(pages[0].by, Author::Named("Slack".into()));

    // Its document deleted, the next edit writes it anew in the channel's folder.
    support::delete_page(&dsp, &written.id);
    due_now(&state, &dsp, EDITING, CANVAS).await;
    sync_due(&state, &dsp).await;
    let pages = support::pages(&dsp);
    assert_eq!(pages.len(), 1);
    assert_ne!(pages[0].id, written.id);
    assert_eq!(pages[0].folder, "folder-write-ups");
}

#[tokio::test]
async fn a_canvas_is_kept_only_while_something_covers_it_and_left_once_gone_from_slack() {
    let (_root, state, dsp, _) = syncing(TEAM).await;

    // A canvas in a channel nobody linked isn't kept, nor one in a workspace with Sync
    // Canvases off.
    check::check(&state, &dsp).await.unwrap();
    assert!(support::pages(&dsp).is_empty());
    let at = dsp.clone();
    state
        .run(move |db| {
            db.turn_off_slack_integration(&at, TEAM, ID, "user_1")
                .map(drop)
        })
        .await
        .unwrap();
    link(&state, &dsp, TEAM, PHOTOS, "write-ups").await;
    check::check(&state, &dsp).await.unwrap();
    sync_due(&state, &dsp).await;
    assert!(support::pages(&dsp).is_empty());

    // On, a canvas Slack no longer has is left as gone; its channel's others are kept.
    let at = dsp.clone();
    state
        .run(move |db| {
            db.turn_on_slack_integration(&at, TEAM, ID, "user_1")
                .map(drop)
        })
        .await
        .unwrap();
    let at = dsp.clone();
    state
        .run(move |db| {
            db.note_slack_canvas(
                &at,
                TEAM,
                "F0FIXDELETED",
                PHOTOS,
                "Test",
                now(),
                now(),
                now(),
            )
            .map(drop)
        })
        .await
        .unwrap();
    check::check(&state, &dsp).await.unwrap();
    assert_eq!(
        kept(&state, &dsp, TEAM, "F0FIXDELETED")
            .await
            .unwrap()
            .status,
        Status::Gone
    );
    assert_eq!(
        kept(&state, &dsp, TEAM, CANVAS).await.unwrap().status,
        Status::Synced
    );
    assert_eq!(support::pages(&dsp).len(), 1);
    // A canvas shared in a channel nobody linked isn't noticed.
    let told = Heard {
        dsp: dsp.clone(),
        account: TEAM.into(),
        kind: "file_shared".into(),
        event: json!({"type": "file_shared", "file_id": "F0FIXROUTE", "channel_id": DISPATCH}),
    };
    heard(state.clone(), told).await;
    assert!(kept(&state, &dsp, TEAM, "F0FIXROUTE").await.is_none());
}

#[tokio::test]
async fn a_change_slack_tells_of_waits_for_the_changes_to_stop_and_one_mid_sync_is_kept() {
    let (_root, state, dsp, _) = syncing(TEAM).await;
    link(&state, &dsp, TEAM, PHOTOS, "write-ups").await;
    let at = dsp.clone();
    let checked = state
        .run(move |db| {
            // Synced at 1,000, from its edit at 500.
            db.note_slack_canvas(&at, TEAM, "F0FIXTEST", PHOTOS, "Test", 500, 1_000, 1_000)?;
            db.claim_slack_canvas(&at, TEAM, "F0FIXTEST", 1_000, 9_000)?;
            let synced = Outcome::Synced {
                doc: "doc-9".into(),
                link: "https://example.test/doc-9".into(),
                title: "Test".into(),
                edit: 500,
            };
            db.finish_slack_canvas(&at, TEAM, "F0FIXTEST", &synced, 1_100)?;
            // Changes heard from 2,000 move it on, each two minutes after the last...
            assert!(db.heard_slack_canvas(
                &at,
                TEAM,
                "F0FIXTEST",
                2_000,
                2_000 + QUIET_MS,
                600_000
            )?);
            assert!(db.heard_slack_canvas(
                &at,
                TEAM,
                "F0FIXTEST",
                3_000,
                3_000 + QUIET_MS,
                600_000
            )?);
            // ...but never past ten minutes from the first.
            db.heard_slack_canvas(&at, TEAM, "F0FIXTEST", 601_000, 601_000 + QUIET_MS, 600_000)?;
            Ok(())
        })
        .await;
    checked.unwrap();
    let after = kept(&state, &dsp, TEAM, "F0FIXTEST").await.unwrap();
    assert_eq!(after.status, Status::Waiting);
    assert_eq!(after.due_at, 602_000);
    // A change to a canvas it doesn't keep is no news.
    let at = dsp.clone();
    let unknown = state
        .run(move |db| db.heard_slack_canvas(&at, TEAM, "F0FIXOTHER", 1, 2, 3))
        .await
        .unwrap();
    assert!(!unknown);

    // A change heard while it's being synced leaves it waiting, for the change.
    let at = dsp.clone();
    state
        .run(move |db| {
            db.claim_slack_canvas(&at, TEAM, "F0FIXTEST", 700_000, 800_000)?;
            db.heard_slack_canvas(&at, TEAM, "F0FIXTEST", 750_000, 750_000 + QUIET_MS, 600_000)?;
            let synced = Outcome::Synced {
                doc: "doc-9".into(),
                link: "https://example.test/doc-9".into(),
                title: "Test".into(),
                edit: 700_000,
            };
            db.finish_slack_canvas(&at, TEAM, "F0FIXTEST", &synced, 760_000)
        })
        .await
        .unwrap();
    // It waits for that change to stop, from when it was heard.
    let after = kept(&state, &dsp, TEAM, "F0FIXTEST").await.unwrap();
    assert_eq!(after.status, Status::Waiting);
    assert_eq!(after.written_edit, Some(700_000));
    assert_eq!(after.due_at, 750_000 + QUIET_MS);
}

#[tokio::test]
async fn a_canvas_left_comes_back_when_slack_shows_it_or_its_edit_and_follows_its_channel() {
    let (_root, state, dsp, _) = syncing(TEAM).await;
    link(&state, &dsp, TEAM, PHOTOS, "write-ups").await;
    let at = dsp.clone();
    let after = state
        .run(move |db| {
            let left = |canvas: &str, status: Status| -> dispatch_core::Result<()> {
                db.claim_slack_canvas(&at, TEAM, canvas, 1_000, 9_000)?;
                let outcome = Outcome::Left {
                    status,
                    reason: "test".into(),
                };
                db.finish_slack_canvas(&at, TEAM, canvas, &outcome, 1_100)
            };
            let status = |canvas: &str| -> dispatch_core::Result<String> {
                let found: Option<(String,)> = db.dsp(&at)?.one_as(
                    "SELECT status FROM slack_canvases WHERE team_id=? AND canvas_id=?",
                    [TEAM, canvas],
                )?;
                Ok(found.map(|(status,)| status).unwrap_or_default())
            };
            // Left as gone, as Slack answers for a canvas the bot can't see, it's back as soon
            // as Slack lists it again, or tells of a change to it.
            db.note_slack_canvas(&at, TEAM, "F0FIXSEEN", PHOTOS, "Seen", 500, 1_000, 1_000)?;
            left("F0FIXSEEN", Status::Gone)?;
            db.note_slack_canvas(&at, TEAM, "F0FIXSEEN", PHOTOS, "Seen", 500, 2_000, 2_000)?;
            let relisted = status("F0FIXSEEN")?;
            db.note_slack_canvas(&at, TEAM, "F0FIXTOLD", PHOTOS, "Told", 500, 1_000, 1_000)?;
            left("F0FIXTOLD", Status::Gone)?;
            db.heard_slack_canvas(&at, TEAM, "F0FIXTOLD", 2_000, 3_000, 600_000)?;
            let told = status("F0FIXTOLD")?;
            // Given up before it had a document, it isn't tried again until it's edited.
            db.note_slack_canvas(&at, TEAM, "F0FIXFAIL", PHOTOS, "Fail", 500, 1_000, 1_000)?;
            left("F0FIXFAIL", Status::Failed)?;
            db.note_slack_canvas(&at, TEAM, "F0FIXFAIL", PHOTOS, "Fail", 500, 2_000, 2_000)?;
            let unedited = status("F0FIXFAIL")?;
            db.note_slack_canvas(&at, TEAM, "F0FIXFAIL", PHOTOS, "Fail", 600, 3_000, 3_000)?;
            let edited = status("F0FIXFAIL")?;
            // Found first in a channel nobody linked, then in a linked one, it's kept there.
            db.note_slack_canvas(
                &at,
                TEAM,
                "F0FIXMOVED",
                DISPATCH,
                "Moved",
                500,
                1_000,
                1_000,
            )?;
            db.note_slack_canvas(&at, TEAM, "F0FIXMOVED", PHOTOS, "Moved", 500, 2_000, 2_000)?;
            Ok([relisted, told, unedited, edited])
        })
        .await
        .unwrap();
    assert_eq!(after, ["waiting", "waiting", "failed", "waiting"]);
    let moved = kept(&state, &dsp, TEAM, "F0FIXMOVED").await;
    assert_eq!(moved.map(|canvas| canvas.channel), Some(PHOTOS.to_owned()));
}

#[tokio::test]
async fn a_channel_slack_refuses_to_list_leaves_the_check_to_the_others() {
    let (_root, state, dsp, _) = syncing(TEAM).await;
    // A linked channel Slack has no more, as one deleted since, listed before #photos.
    link(&state, &dsp, TEAM, "C0FIXARCHIVED", "archived").await;
    link(&state, &dsp, TEAM, PHOTOS, "write-ups").await;
    check::check(&state, &dsp).await.unwrap();
    let synced = kept(&state, &dsp, TEAM, CANVAS).await;
    assert_eq!(synced.map(|canvas| canvas.status), Some(Status::Synced));
}
