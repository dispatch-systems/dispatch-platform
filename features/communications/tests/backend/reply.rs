//! Answering from Dispatch: each answer goes to the member's direct message in Slack under the
//! full name of whoever wrote it and their Slack profile photo, or the image of their initials,
//! as written, with any photos they added; and someone at the DSP can message a member first.
use super::*;
use crate::backend::{
    conversations,
    files::FileStatus,
    initials,
    storage::{CommunicationsStore, ConversationState},
    support::{self, USE},
};
use crate::integrations::messages::incoming;
use axum::body::Bytes;
use dispatch_core::db::now;
use dispatch_slack::{kit::Bot, testing as slack};
use futures_util::stream;
use serde_json::json;

const TEAM: &str = "T0FIXTURE1";

/// The conversation Jordan Ellis started in the direct message `dm`.
async fn started(state: &Arc<State>, bot: &Bot, dm: &str) -> String {
    let event = json!({"type": "message", "channel_type": "im", "channel": dm,
        "user": "U0FIXJORDAN", "text": "Van won't start", "ts": incoming::ts(now() - 1)});
    incoming::heard(state, &bot.dsp, TEAM, &event)
        .await
        .unwrap();
    support::conversation_with(state, &bot.dsp, TEAM, "U0FIXJORDAN")
        .await
        .unwrap()
        .id
}

/// How many of the DSP's conversations its page lists.
async fn listed(state: &Arc<State>, dsp: &str) -> usize {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    let listed = state
        .read(move |db| conversations::overview(db, &shared, &dsp, "nobody"))
        .await
        .unwrap();
    listed.conversations.len()
}

#[tokio::test]
async fn an_answer_goes_out_as_written_under_its_writers_name_and_slack_photo_or_initials() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0REPLY").await;
    // No member of the workspaces has their email address: their initials.
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    send(&state, &owner, USE, &id, " Keys <@here> & go ", &[])
        .await
        .unwrap();
    // A member with a photo of their own: that photo. One with Slack's drawn photo: initials.
    let morgan = support::member(&state, &dsp, "Morgan", "Lane", "Morgan.Lane@example.com").await;
    send(&state, &morgan, USE, &id, "On my way", &[])
        .await
        .unwrap();
    let jordan = support::member(&state, &dsp, "Jordan", "Ellis", "jordan.ellis@example.com").await;
    send(&state, &jordan, USE, &id, "Thanks", &[])
        .await
        .unwrap();
    let initials = |letters: &str| Some(initials::address(&state.config.origin, letters));
    let posted: Vec<_> = slack::posted(TEAM, "D0REPLY")
        .into_iter()
        .map(|post| (post.text, post.username, post.icon_url))
        .collect();
    assert_eq!(
        posted,
        [
            (
                "Keys &lt;@here&gt; &amp; go".into(),
                Some("Test Owner".into()),
                initials("TO")
            ),
            (
                "On my way".into(),
                Some("Morgan Lane".into()),
                Some("https://avatars.example.com/fixture/U0FIXOWNER_512.jpg".into())
            ),
            ("Thanks".into(), Some("Jordan Ellis".into()), initials("JE")),
        ]
    );
    // Kept as written, under whoever wrote it; answered, the conversation waits no more.
    let messages = support::messages(&state, &dsp, &id).await;
    let kept: Vec<(&str, Option<&str>)> = messages
        .iter()
        .map(|message| {
            let by = message.sender.as_ref().map(|sender| sender.name.as_str());
            (message.text.as_str(), by)
        })
        .collect();
    assert_eq!(
        kept,
        [
            ("Van won't start", None),
            ("Keys <@here> & go", Some("Test Owner")),
            ("On my way", Some("Morgan Lane")),
            ("Thanks", Some("Jordan Ellis")),
        ]
    );
    let at = dsp.clone();
    let state_now = state
        .read(move |db| Ok(db.communications_conversation(&at, &id)?.unwrap().state))
        .await
        .unwrap();
    assert_eq!(state_now, ConversationState::Responded);
}

#[tokio::test]
async fn an_answer_carries_the_photos_added_for_it_shown_in_slack() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0PHOTOS").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let add = |name: &'static str, mime: &'static str, bytes: Vec<u8>| {
        let (state, owner) = (state.clone(), owner.clone());
        async move {
            let length = bytes.len() as u64;
            let photo = Upload::streamed(
                length,
                PHOTO_MOST,
                stream::iter([Ok::<_, std::io::Error>(Bytes::from(bytes))]),
            )
            .await
            .unwrap();
            upload(&state, &owner, USE, name, mime, photo).await
        }
    };
    let png = initials::image("PH.png").unwrap().to_vec();
    let photo = add("Dashboard.png", "image/png", png).await.unwrap();
    // One that isn't the photo it says it is is refused.
    let refused = add("Notes.png", "image/png", b"not a picture".to_vec())
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_photo_kind");
    let message = send(
        &state,
        &owner,
        USE,
        &id,
        "",
        std::slice::from_ref(&photo.id),
    )
    .await
    .unwrap();
    assert_eq!(
        (message.files.len(), message.files[0].status),
        (1, FileStatus::Kept)
    );
    let posted = slack::posted(TEAM, "D0PHOTOS").remove(0);
    assert_eq!(posted.text, "Dashboard.png");
    assert_eq!(posted.blocks[0]["type"], "image");
    assert!(posted.blocks[0]["slack_file"]["id"].as_str().is_some());
    // Sent, it's no longer one to add to another message.
    let again = send(&state, &owner, USE, &id, "Again", &[photo.id])
        .await
        .unwrap_err();
    assert_eq!(again.code, "communications_upload_not_found");
}

#[tokio::test]
async fn someone_at_the_dsp_messages_a_member_first() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let opened = open(&state, &owner, USE, TEAM, "U0FIXOWNER").await.unwrap();
    assert_eq!(
        (opened.with.name.as_str(), opened.last_at),
        ("Morgan Lane", None)
    );
    // Opened, it isn't listed until a message is in it.
    assert_eq!(listed(&state, &dsp).await, 0);
    send(&state, &owner, USE, &opened.id, "Welcome aboard", &[])
        .await
        .unwrap();
    assert_eq!(slack::posted(TEAM, "D0FIXOWNER")[0].text, "Welcome aboard");
    assert_eq!(listed(&state, &dsp).await, 1);
    // Only a member of a workspace, while they're active there.
    let gone = open(&state, &owner, USE, TEAM, "U0NOBODY")
        .await
        .unwrap_err();
    assert_eq!(gone.code, "communications_contact_not_found");
}
