//! Answering from Dispatch: each answer goes to the member's direct message in Slack under the
//! full name of whoever wrote it and their Slack profile photo, or the image of their initials,
//! as written, with any photos they added, a photo refused for its kind taking nothing from the
//! same photo added meanwhile; one Slack took that wasn't kept is kept once and never posted
//! twice; and someone at the DSP can message a member first.
use super::*;
use crate::backend::{
    files::{self, FileStore},
    inbox, initials,
    model::{ConversationState, FileStatus},
    storage::{conversations::ConversationStore, replies::ReplyStore},
    support::{self, USE},
};
use crate::integrations::messages::incoming;
use axum::body::Bytes;
use dispatch_core::accounts::Context;
use dispatch_core::db::now;
use dispatch_slack::{kit::Bot, testing as slack};
use futures_util::{TryStreamExt, stream};
use serde_json::json;
use std::task::Poll;

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

/// How long ago a try that ended, holding its note no more, started.
const ENDED: i64 = SENDING_MS + 60_000;

/// What a try of `c`'s answer `text` in the conversation `id` leaves when nothing of it was kept,
/// as when the server stopped once Slack had it: its note, from `ago` milliseconds back, held as
/// long as a try holds one, and its post in the direct message `dm` if Slack took it.
async fn unkept(
    state: &Arc<State>,
    bot: &Bot,
    c: &Context,
    (id, dm): (&str, &str),
    text: &str,
    ago: i64,
    took: bool,
) -> Noted {
    let noted = Noted {
        id: crypto::id("rpl").unwrap(),
        conversation: id.into(),
        text: text.into(),
        by: c.auth.user.id.clone(),
        name: "Test Owner".into(),
        photos: Vec::new(),
        posting_since: now() - ago,
        claimed_until: now() - ago + SENDING_MS,
    };
    let (at, note) = (bot.dsp.clone(), noted.clone());
    state
        .run(move |db| db.note_communications_reply(&at, &note))
        .await
        .unwrap();
    if took {
        let args = json!({"channel": dm, "text": text, "metadata": tag(EVENT, &noted.id)});
        bot.call("chat.postMessage", &args).await.unwrap();
    }
    noted
}
/// The texts of the conversation's messages, as kept, and of the answers noted there.
async fn kept_and_noted(state: &Arc<State>, dsp: &str, id: &str) -> (Vec<String>, Vec<String>) {
    let kept = support::messages(state, dsp, id)
        .await
        .into_iter()
        .map(|message| message.text)
        .collect();
    let (at, of) = (dsp.to_owned(), id.to_owned());
    let noted = state
        .read(move |db| db.noted_communications_replies(&at, Some(&of), i64::MAX))
        .await
        .unwrap()
        .into_iter()
        .map(|noted| noted.text)
        .collect();
    (kept, noted)
}

/// `bytes`, as an upload of a photo brings them.
async fn photo(bytes: Vec<u8>) -> Upload {
    let length = bytes.len() as u64;
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from(bytes))]);
    Upload::streamed(length, SLACK.largest, chunks)
        .await
        .unwrap()
}

/// How many of the DSP's conversations its page lists.
async fn listed(state: &Arc<State>, dsp: &str) -> usize {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    let listed = state
        .read(move |db| inbox::overview(db, &shared, &dsp, "nobody", None))
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
        async move { upload(&state, &owner, USE, name, mime, photo(bytes).await).await }
    };
    let png = initials::image("PH.png").unwrap().to_vec();
    let photo = add("Dashboard.png", "image/png", png).await.unwrap();
    // One that isn't the photo it says it is is refused.
    let refused = add("Notes.png", "image/png", b"not a picture".to_vec())
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_photo_kind");
    // Named twice, it isn't taken as two.
    let twice = [photo.id.clone(), photo.id.clone()];
    let named = send(&state, &owner, USE, &id, "", &twice)
        .await
        .unwrap_err();
    assert_eq!(named.code, "invalid_input");
    // Going with one message, it can't go with another at once.
    let (at, by, ids) = (
        dsp.clone(),
        owner.auth.user.id.clone(),
        vec![photo.id.clone()],
    );
    let claim = move |db: &dispatch_core::db::Store| {
        db.claim_communications_uploads(&at, &by, &ids, now() + 60_000, None)
            .map(drop)
    };
    state.run(claim.clone()).await.unwrap();
    let taken = state.run(claim).await.unwrap_err();
    assert_eq!(taken.code, "communications_upload_not_found");
    let (at, ids) = (dsp.clone(), vec![photo.id.clone()]);
    state
        .run(move |db| db.release_communications_uploads(&at, &ids))
        .await
        .unwrap();
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
async fn a_photo_refused_for_its_kind_takes_nothing_from_the_same_photo_added_meanwhile() {
    let (_root, state, dsp) = support::started();
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let at = dsp.clone();
    let folder = state
        .read(move |db| Ok(support::files_kept(db, &at)))
        .await
        .unwrap();
    let placed = || {
        std::fs::read_dir(&folder).unwrap().any(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        })
    };
    let png = initials::image("PH.png").unwrap().to_vec();
    // A PNG being added is let through until its bytes are among the DSP's, this test polling it,
    // waking itself, as nothing else would.
    let adding = upload(
        &state,
        &owner,
        USE,
        "Dashboard.png",
        "image/png",
        photo(png.clone()).await,
    );
    tokio::pin!(adding);
    let added = std::future::poll_fn(|cx| {
        if placed() {
            return Poll::Ready(None);
        }
        match adding.as_mut().poll(cx) {
            Poll::Ready(added) => Poll::Ready(Some(added)),
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    })
    .await;
    // Meanwhile the same bytes, said to be a JPEG, are refused.
    let jpeg = photo(png.clone()).await;
    let refused = upload(&state, &owner, USE, "Dashboard.jpg", "image/jpeg", jpeg).await;
    assert_eq!(refused.unwrap_err().code, "communications_photo_kind");
    // The PNG is added all the same, its bytes there to send.
    let added = match added {
        Some(added) => added,
        None => adding.await,
    };
    let (at, id) = (dsp.clone(), added.expect("the PNG is added").id);
    let sha256 = state
        .read(move |db| Ok(db.communications_file(&at, &id)?.unwrap().sha256.unwrap()))
        .await
        .unwrap();
    let kept = files::read(&state, &dsp, &sha256).await;
    let bytes: Vec<u8> = kept
        .expect("its bytes are kept")
        .map_ok(|chunk| chunk.to_vec())
        .try_concat()
        .await
        .unwrap();
    assert_eq!(bytes, png);
}

#[tokio::test]
async fn sent_again_an_answer_slack_took_unkept_is_kept_and_not_posted_twice() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0AGAIN").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let at = (id.as_str(), "D0AGAIN");
    unkept(&state, &bot, &owner, at, "Keys are in the van", ENDED, true).await;
    let message = send(&state, &owner, USE, &id, "Keys are in the van", &[])
        .await
        .unwrap();
    let posted = kit::fixture_history(TEAM, "D0AGAIN");
    assert_eq!(posted.len(), 1);
    assert_eq!(message.ts, posted[0]["ts"].as_str().unwrap());
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await,
        (
            vec!["Van won't start".into(), "Keys are in the van".into()],
            vec![]
        )
    );
    // One Slack didn't take is posted, under the same note, so should Slack take this try and
    // nothing keep it, sending it again finds this try's post.
    let failed = unkept(&state, &bot, &owner, at, "On my way", ENDED, false).await;
    send(&state, &owner, USE, &id, "On my way", &[])
        .await
        .unwrap();
    let posted = slack::posted(TEAM, "D0AGAIN");
    assert_eq!(posted.len(), 2);
    assert_eq!(posted[1].metadata["event_payload"]["message"], failed.id);
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await.1,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn an_answer_slack_took_unkept_is_found_however_much_was_written_after_it() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0BUSY").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    unkept(
        &state,
        &bot,
        &owner,
        (&id, "D0BUSY"),
        "On my way",
        ENDED,
        true,
    )
    .await;
    // More than a page of Slack's history after it.
    for n in 0..250 {
        let args = json!({"channel": "D0BUSY", "text": format!("Reminder {n}")});
        bot.call("chat.postMessage", &args).await.unwrap();
    }
    send(&state, &owner, USE, &id, "On my way", &[])
        .await
        .unwrap();
    assert_eq!(slack::posted(TEAM, "D0BUSY").len(), 251);
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await,
        (vec!["Van won't start".into(), "On my way".into()], vec![])
    );
}

#[tokio::test]
async fn sent_again_while_another_try_of_it_is_under_way_an_answer_isnt_posted_twice() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0OVERLAP").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    // A try still under way, as while Slack asks it to slow down, and not in Slack yet.
    let held = unkept(
        &state,
        &bot,
        &owner,
        (&id, "D0OVERLAP"),
        "On my way",
        0,
        false,
    )
    .await;
    let refused = send(&state, &owner, USE, &id, "On my way", &[])
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_reply_sending");
    assert!(slack::posted(TEAM, "D0OVERLAP").is_empty());
    // Once it's ended, upkeep holds it while it looks; a resend claiming it first keeps it from
    // upkeep, which then neither looks for it nor clears it.
    let (at, note) = (dsp.clone(), held.id.clone());
    let claimed = state
        .run(move |db| db.claim_communications_reply(&at, &note, now() + SENDING_MS, i64::MAX))
        .await
        .unwrap();
    assert!(claimed);
    let (at, note, stale) = (dsp.clone(), held.id.clone(), held.claimed_until);
    state
        .run(move |db| db.clear_communications_reply(&at, &note, stale))
        .await
        .unwrap();
    settle(&state, &dsp).await.unwrap();
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await.1,
        vec!["On my way".to_owned()]
    );
}

#[tokio::test]
async fn an_answer_noted_on_its_way_to_slack_keeps_its_photos_until_its_settled_for_it_alone() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0KEPTPHOTO").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let png = initials::image("PH.png").unwrap().to_vec();
    let added = upload(
        &state,
        &owner,
        USE,
        "Van.png",
        "image/png",
        photo(png).await,
    )
    .await
    .unwrap();
    // A try Slack took with the photo a day and more ago, its workspace unreachable since.
    let noted = Noted {
        id: crypto::id("rpl").unwrap(),
        conversation: id.clone(),
        text: "Here's the van".into(),
        by: owner.auth.user.id.clone(),
        name: "Test Owner".into(),
        photos: vec![(added.id.clone(), "F0VANPHOTO".into())],
        posting_since: now() - 25 * 60 * 60_000,
        claimed_until: now() - 25 * 60 * 60_000 + SENDING_MS,
    };
    let (at, note) = (dsp.clone(), noted.clone());
    state
        .run(move |db| db.note_communications_reply(&at, &note))
        .await
        .unwrap();
    let args = json!({"channel": "D0KEPTPHOTO", "text": "Here's the van",
        "metadata": tag(EVENT, &noted.id)});
    bot.call("chat.postMessage", &args).await.unwrap();
    // Another message can't take it, though its try ended.
    let taken = send(
        &state,
        &owner,
        USE,
        &id,
        "Van 12",
        std::slice::from_ref(&added.id),
    )
    .await
    .unwrap_err();
    assert_eq!(taken.code, "communications_upload_not_found");
    // Upkeep letting go of photos never sent leaves it.
    let at = dsp.clone();
    state
        .run(move |db| db.forget_unsent_communications(&at, now() + 1))
        .await
        .unwrap();
    settle(&state, &dsp).await.unwrap();
    let messages = support::messages(&state, &dsp, &id).await;
    let kept = messages.last().unwrap();
    assert_eq!(
        (kept.text.as_str(), kept.files.len()),
        ("Here's the van", 1)
    );
}

#[tokio::test]
async fn two_first_tries_of_an_answer_begun_together_share_one_note_and_one_is_refused() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0TOGETHER").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let by = owner.auth.user.id.clone();
    let begun = |until: i64| {
        let (at, of, by) = (dsp.clone(), id.clone(), by.clone());
        state.run(move |db| {
            begin(db, &at, &of, (&by, "Test Owner"), "On my way", &[], until).map(drop)
        })
    };
    // Neither had found a note; the first notes it as it begins, and the second finds it held.
    begun(now() + SENDING_MS).await.unwrap();
    let refused = begun(now() + SENDING_MS).await.unwrap_err();
    assert_eq!(refused.code, "communications_reply_sending");
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await.1,
        vec!["On my way".to_owned()]
    );
}

#[tokio::test]
async fn a_note_whose_conversation_an_earlier_release_forgot_is_cleared() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    // Messaged first, the try ended unkept, before any message was in the conversation.
    let opened = open(&state, &owner, USE, TEAM, "U0FIXOWNER").await.unwrap();
    let at = (opened.id.as_str(), "D0FIXOWNER");
    unkept(&state, &bot, &owner, at, "Welcome aboard", ENDED, false).await;
    // Rolled back, the release before this one forgets the workspace's conversations with no
    // message, knowing nothing of notes.
    let of = dsp.clone();
    state
        .run(move |db| {
            db.dsp(&of)?.exec(
                "DELETE FROM communications_conversations WHERE team_id=? AND last_at IS NULL",
                [TEAM],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    settle(&state, &dsp).await.unwrap();
    let (at, of) = (dsp.clone(), opened.id.clone());
    let noted = state
        .read(move |db| db.noted_communications_replies(&at, Some(&of), i64::MAX))
        .await
        .unwrap();
    assert!(noted.is_empty());
}

#[tokio::test]
async fn upkeep_keeps_an_answer_slack_took_unkept_and_lets_go_of_one_no_try_holds_it_didnt() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let id = started(&state, &bot, "D0UPKEEP").await;
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let at = (id.as_str(), "D0UPKEEP");
    unkept(&state, &bot, &owner, at, "On my way", ENDED, true).await;
    unkept(&state, &bot, &owner, at, "Running late", ENDED, false).await;
    // One noted a day and more ago is still looked for, and kept.
    unkept(
        &state,
        &bot,
        &owner,
        at,
        "Back at the station",
        25 * 60 * 60_000,
        true,
    )
    .await;
    // One a try still holds, as while Slack asks it to slow down, is left for now.
    unkept(
        &state,
        &bot,
        &owner,
        at,
        "Keys are in the van",
        2 * 60_000,
        false,
    )
    .await;
    settle(&state, &dsp).await.unwrap();
    assert_eq!(
        kept_and_noted(&state, &dsp, &id).await,
        (
            vec![
                "Van won't start".into(),
                "On my way".into(),
                "Back at the station".into()
            ],
            vec!["Keys are in the van".into()]
        )
    );
    assert_eq!(slack::posted(TEAM, "D0UPKEEP").len(), 2);
    // Answered, the conversation waits no more.
    let at = dsp.clone();
    let state_now = state
        .read(move |db| Ok(db.communications_conversation(&at, &id)?.unwrap().state))
        .await
        .unwrap();
    assert_eq!(state_now, ConversationState::Responded);
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
