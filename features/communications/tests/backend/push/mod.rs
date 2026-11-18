//! Who is pushed what: each message new in what waits, to each browser of each member who may be
//! told of it, once, opening where it was subscribed; each line's alone to the browsers told of
//! it; the browsers let go of once they can't be told, even as a push waits to be tried again;
//! pushes going a few at once, however many batches go; and what a push says of a message, as the
//! list does.
use super::*;
use crate::api::types::{FileStatus, MessageFile};
use crate::backend::{
    phone::storage::Line,
    reply,
    support::{self, USE},
};
use dispatch_core::db::now;
use futures_util::FutureExt;
use serde_json::json;

const TEAM: &str = "T0FIXTURE2";
/// A line of the DSP's phones beside Dispatch Phone's own, as its feature pairs them.
const LINE: Line = Line {
    id: "front_desk",
    name: "Front Desk",
};
/// That line's pushes, as its feature tells of them.
const DESK: Pushed = Pushed {
    line: Some(LINE.id),
    permission: "communications.use",
    page: "front-desk",
    label: Some(LINE.name),
    display: &DISPLAY,
};

/// Has `user` turn Notifications on or off, their Message display at `revision`.
async fn notifications(state: &Arc<State>, dsp: &str, user: &str, on: bool, revision: i64) {
    let (dsp, user) = (dsp.to_owned(), user.to_owned());
    state
        .run(move |db| {
            let values = json!({"notifications": on});
            settings::save(db, &dsp, &user, &DISPLAY, &values, revision, false).map(drop)
        })
        .await
        .unwrap();
}
/// The browser at `endpoint`, `user`'s, subscribed at `address`, under keys fixture mode's push
/// services don't read.
fn browser(user: &str, endpoint: &str, address: Address) -> Subscription {
    Subscription {
        endpoint: endpoint.to_owned(),
        user: user.to_owned(),
        p256dh: "browser-key".to_owned(),
        auth: "browser-secret".to_owned(),
        address,
    }
}
/// Keeps the browser at `endpoint` as `user`'s, subscribed at the admin's address.
async fn keep(state: &Arc<State>, dsp: &str, user: &str, endpoint: &str) {
    keep_at(state, dsp, user, endpoint, Address::Admin).await;
}
async fn keep_at(state: &Arc<State>, dsp: &str, user: &str, endpoint: &str, address: Address) {
    let (dsp, subscription) = (dsp.to_owned(), browser(user, endpoint, address));
    state
        .run(move |db| db.keep_communications_push(&dsp, None, &subscription))
        .await
        .unwrap();
}
/// The browsers kept, by their endpoints.
async fn kept(state: &Arc<State>, dsp: &str) -> Vec<String> {
    let dsp = dsp.to_owned();
    let kept = state
        .read(move |db| db.communications_pushes(&dsp, None))
        .await;
    kept.unwrap()
        .into_iter()
        .map(|each| each.endpoint)
        .collect()
}

#[tokio::test(start_paused = true)]
async fn each_message_new_in_what_waits_goes_to_each_browser_of_each_member_told_once() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let owner = c.actor().to_owned();
    notifications(&state, &dsp, &owner, true, 0).await;
    let laptop = "https://fcm.googleapis.com/fcm/send/told-laptop";
    let phone = "https://web.push.apple.com/told-phone";
    keep(&state, &dsp, &owner, laptop).await;
    keep(&state, &dsp, &owner, phone).await;
    let (mut seen, mut revision) = (None, None);

    // What waited as the watching began tells no one.
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Morning").await;
    assert!(
        look(&state, &dsp, &OWN, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );

    // Another message in the conversation waiting is new: it goes to each of their browsers.
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    let new = look(&state, &dsp, &OWN, &mut seen, &mut revision)
        .await
        .unwrap();
    tell(&state, &dsp, &OWN, new).await.unwrap();
    let conversation = support::conversation_with(&state, &dsp, TEAM, "U0FIXCASEY")
        .await
        .unwrap();
    let message = support::messages(&state, &dsp, &conversation.id).await;
    for endpoint in [laptop, phone] {
        let pushed = fixture::pushed(endpoint);
        assert_eq!(pushed.len(), 1, "{endpoint}");
        assert_eq!(pushed[0]["title"], "Casey Rivera");
        assert_eq!(pushed[0]["body"], "Where do I park?");
        assert_eq!(pushed[0]["tag"], message.last().unwrap().id);
        // Opening the conversation at the admin's address, where they subscribed, for a page in
        // front to tell as this DSP's.
        let opens = format!(
            "{}/#dsp/{dsp}/communications?open={}",
            state.config.origin, conversation.id
        );
        assert_eq!(pushed[0]["url"], opens);
        assert_eq!(pushed[0]["dsp"], dsp);
        assert_eq!(pushed[0]["conversation"], conversation.id);
    }

    // Read again, nothing is new; nor is the DSP's answer.
    assert!(
        look(&state, &dsp, &OWN, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );
    reply::send(
        &state,
        &c,
        USE,
        &conversation.id,
        "By the door",
        (&[], None),
    )
    .await
    .unwrap();
    assert!(
        look(&state, &dsp, &OWN, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(start_paused = true)]
async fn browsers_are_let_go_of_once_they_cant_be_told() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let owner = c.actor().to_owned();
    notifications(&state, &dsp, &owner, true, 0).await;
    // A browser whose push service is busy at first, one it let go of, one they subscribed at
    // the DSP's own address, where their account isn't, and one of someone who isn't the DSP's.
    let busy = "https://fcm.googleapis.com/fcm/send/busy-desk";
    let gone = "https://fcm.googleapis.com/fcm/send/gone-desk";
    let elsewhere = "https://fcm.googleapis.com/fcm/send/elsewhere-desk";
    let stranger = "https://fcm.googleapis.com/fcm/send/stranger-desk";
    keep(&state, &dsp, &owner, busy).await;
    keep(&state, &dsp, &owner, gone).await;
    keep_at(&state, &dsp, &owner, elsewhere, Address::Dsp).await;
    keep(
        &state,
        &dsp,
        "user_00000000000000000000000000000000",
        stranger,
    )
    .await;
    let (mut seen, mut revision) = (None, None);
    assert!(
        look(&state, &dsp, &OWN, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );

    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    let new = look(&state, &dsp, &OWN, &mut seen, &mut revision)
        .await
        .unwrap();
    tell(&state, &dsp, &OWN, new).await.unwrap();
    // The busy one's, tried again, went; the others went nowhere, and are let go of.
    assert_eq!(fixture::pushed(busy).len(), 1);
    for nowhere in [gone, elsewhere, stranger] {
        assert!(fixture::pushed(nowhere).is_empty(), "{nowhere}");
    }
    assert_eq!(kept(&state, &dsp).await, [busy]);

    // Notifications turned off, their browsers are let go of as the next message would go.
    notifications(&state, &dsp, &owner, false, 1).await;
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Hello?").await;
    let new = look(&state, &dsp, &OWN, &mut seen, &mut revision)
        .await
        .unwrap();
    assert_eq!(new.len(), 1);
    tell(&state, &dsp, &OWN, new.clone()).await.unwrap();
    assert_eq!(fixture::pushed(busy).len(), 1);
    assert!(kept(&state, &dsp).await.is_empty());

    // On again, but with Communications off for the DSP, which no session of theirs could use.
    notifications(&state, &dsp, &owner, true, 2).await;
    keep(&state, &dsp, &owner, busy).await;
    support::switch(&state, &dsp, crate::FEATURE.switch.id, false).await;
    tell(&state, &dsp, &OWN, new).await.unwrap();
    assert_eq!(fixture::pushed(busy).len(), 1);
    assert!(kept(&state, &dsp).await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn each_lines_texts_are_heard_and_pushed_on_its_own_alone() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let owner = c.actor().to_owned();
    notifications(&state, &dsp, &owner, true, 0).await;
    let at = dsp.clone();
    let phone = state
        .run(move |db| Ok(support::pair_to(db, &at, LINE, "+12065550111")))
        .await
        .unwrap();
    // One browser, told of each line apart.
    let browser = "https://fcm.googleapis.com/fcm/send/both-lines";
    keep(&state, &dsp, &owner, browser).await;
    let (at, theirs) = (dsp.clone(), self::browser(&owner, browser, Address::Admin));
    state
        .run(move |db| db.keep_communications_push(&at, DESK.line, &theirs))
        .await
        .unwrap();
    let (mut ours_seen, mut ours_read) = (None, None);
    let (mut desk_seen, mut desk_read) = (None, None);
    let ours = look(&state, &dsp, &OWN, &mut ours_seen, &mut ours_read).await;
    assert!(ours.unwrap().is_empty());
    let theirs = look(&state, &dsp, &DESK, &mut desk_seen, &mut desk_read).await;
    assert!(theirs.unwrap().is_empty());
    let mut our_page = live::listen(&state, &dsp, None);
    let mut their_page = live::listen(&state, &dsp, DESK.line);
    let at = dsp.clone();
    let revision = state
        .read(move |db| db.communications_revision(&at, None))
        .await
        .unwrap();

    // The Front Desk's phone is texted: its pages alone hear, and its pushes alone tell of it,
    // under its name, opening its page.
    support::texted_on(
        &state,
        &dsp,
        &phone,
        "+14255550199",
        "About my paycheck",
        now(),
    )
    .await;
    assert!(their_page.changed().now_or_never().is_some());
    assert!(our_page.changed().now_or_never().is_none());
    let at = dsp.clone();
    let after = state
        .read(move |db| db.communications_revision(&at, None))
        .await
        .unwrap();
    assert_eq!(after, revision);
    let ours = look(&state, &dsp, &OWN, &mut ours_seen, &mut ours_read).await;
    assert!(ours.unwrap().is_empty());
    let new = look(&state, &dsp, &DESK, &mut desk_seen, &mut desk_read)
        .await
        .unwrap();
    assert_eq!(new.len(), 1);
    let conversation = new[0].conversation_id.clone();
    tell(&state, &dsp, &DESK, new).await.unwrap();
    let pushed = fixture::pushed(browser);
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0]["title"], "Front Desk · (425) 555-0199");
    let opens = format!(
        "{}/#dsp/{dsp}/front-desk?open={conversation}",
        state.config.origin
    );
    assert_eq!(pushed[0]["url"], opens);

    // Dispatch Phone's own is texted by the same number: its own alone tell of it, as ever.
    support::texted(&state, &dsp, "+14255550199", "Is this dispatch?", now()).await;
    assert!(their_page.changed().now_or_never().is_none());
    let theirs = look(&state, &dsp, &DESK, &mut desk_seen, &mut desk_read).await;
    assert!(theirs.unwrap().is_empty());
    let new = look(&state, &dsp, &OWN, &mut ours_seen, &mut ours_read)
        .await
        .unwrap();
    assert_eq!(new.len(), 1);
    tell(&state, &dsp, &OWN, new).await.unwrap();
    let pushed = fixture::pushed(browser);
    assert_eq!(pushed.len(), 2);
    assert_eq!(pushed[1]["title"], "(425) 555-0199");
}

#[tokio::test(start_paused = true)]
async fn pushes_go_a_few_at_once_however_many_batches_go() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let owner = c.actor().to_owned();
    notifications(&state, &dsp, &owner, true, 0).await;
    // Two batches of browsers whose push services take a second each, going together.
    let endpoint = |n: usize| format!("https://fcm.googleapis.com/fcm/send/slow-{n}");
    let ns: Vec<usize> = (0..sending::AT_ONCE)
        .chain(100..100 + sending::AT_ONCE)
        .collect();
    for n in &ns {
        keep(&state, &dsp, &owner, &endpoint(*n)).await;
    }
    let batch = |from: usize| -> Vec<(Subscription, Vec<u8>)> {
        (from..from + sending::AT_ONCE)
            .map(|n| {
                (
                    browser(&owner, &endpoint(n), Address::Admin),
                    b"{}".to_vec(),
                )
            })
            .collect()
    };
    let (first, second) = tokio::join!(
        sending::send(&state, &dsp, &OWN, batch(0)),
        sending::send(&state, &dsp, &OWN, batch(100)),
    );
    first.unwrap();
    second.unwrap();
    assert!(fixture::most_answering() <= sending::AT_ONCE);
    for n in ns {
        assert_eq!(fixture::pushed(&endpoint(n)).len(), 1, "{}", endpoint(n));
    }
}

#[tokio::test(start_paused = true)]
async fn a_push_tried_again_goes_only_if_its_member_is_still_told() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let owner = c.actor().to_owned();
    notifications(&state, &dsp, &owner, true, 0).await;
    let busy = "https://fcm.googleapis.com/fcm/send/busy-retried";
    keep(&state, &dsp, &owner, busy).await;
    let pushes = vec![(browser(&owner, busy, Address::Admin), b"{}".to_vec())];
    let sending = {
        let (state, dsp) = (state.clone(), dsp.clone());
        tokio::spawn(async move { sending::send(&state, &dsp, &OWN, pushes).await })
    };
    // Refused the first time; while it waits to be tried again, they turn Notifications off.
    while !fixture::refused(busy) {
        tokio::task::yield_now().await;
    }
    notifications(&state, &dsp, &owner, false, 1).await;
    sending.await.unwrap().unwrap();
    assert!(fixture::pushed(busy).is_empty());
    assert!(kept(&state, &dsp).await.is_empty());
}

#[test]
fn a_push_previews_a_message_as_the_list_does() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../frontend/previews.json")).unwrap();
    for each in cases {
        let files: Vec<MessageFile> = each["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| MessageFile {
                id: "file".into(),
                name: file[1].as_str().unwrap().into(),
                mime: file[0].as_str().unwrap().into(),
                size: 1,
                status: FileStatus::Kept,
                version: None,
                preview: false,
            })
            .collect();
        let text = each["text"].as_str().unwrap();
        assert_eq!(sending::preview(text, &files), each["preview"], "{each}");
    }
}
