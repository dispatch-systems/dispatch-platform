//! Who is pushed what: each message new in what waits, to each browser of each member who may be
//! told of it, once; and the browsers let go of once they can't be told.
use super::*;
use crate::backend::{
    reply,
    support::{self, USE},
};
use serde_json::json;

const TEAM: &str = "T0FIXTURE2";

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
/// Keeps the browser at `endpoint` as `user`'s, under keys fixture mode's push services don't read.
async fn keep(state: &Arc<State>, dsp: &str, user: &str, endpoint: &str) {
    let (dsp, subscription) = (
        dsp.to_owned(),
        Subscription {
            endpoint: endpoint.to_owned(),
            user: user.to_owned(),
            p256dh: "browser-key".to_owned(),
            auth: "browser-secret".to_owned(),
        },
    );
    state
        .run(move |db| db.keep_communications_push(&dsp, &subscription))
        .await
        .unwrap();
}
/// The browsers kept, by their endpoints.
async fn kept(state: &Arc<State>, dsp: &str) -> Vec<String> {
    let dsp = dsp.to_owned();
    let kept = state.read(move |db| db.communications_pushes(&dsp)).await;
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
        look(&state, &dsp, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );

    // Another message in the conversation waiting is new: it goes to each of their browsers.
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    let new = look(&state, &dsp, &mut seen, &mut revision).await.unwrap();
    tell(&state, &dsp, new).await.unwrap();
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
        let url = pushed[0]["url"].as_str().unwrap();
        assert!(
            url.ends_with(&format!("communications?open={}", conversation.id)),
            "{url}"
        );
    }

    // Read again, nothing is new; nor is the DSP's answer.
    assert!(
        look(&state, &dsp, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );
    reply::send(&state, &c, USE, &conversation.id, "By the door", &[])
        .await
        .unwrap();
    assert!(
        look(&state, &dsp, &mut seen, &mut revision)
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
    // A browser whose push service is busy at first, one it let go of, and one of someone who
    // isn't the DSP's member.
    let busy = "https://fcm.googleapis.com/fcm/send/busy-desk";
    let gone = "https://fcm.googleapis.com/fcm/send/gone-desk";
    let stranger = "https://fcm.googleapis.com/fcm/send/stranger-desk";
    keep(&state, &dsp, &owner, busy).await;
    keep(&state, &dsp, &owner, gone).await;
    keep(
        &state,
        &dsp,
        "user_00000000000000000000000000000000",
        stranger,
    )
    .await;
    let (mut seen, mut revision) = (None, None);
    assert!(
        look(&state, &dsp, &mut seen, &mut revision)
            .await
            .unwrap()
            .is_empty()
    );

    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Where do I park?").await;
    let new = look(&state, &dsp, &mut seen, &mut revision).await.unwrap();
    tell(&state, &dsp, new).await.unwrap();
    // The busy one's, tried again, went; the others went nowhere, and are let go of.
    assert_eq!(fixture::pushed(busy).len(), 1);
    assert!(fixture::pushed(gone).is_empty());
    assert!(fixture::pushed(stranger).is_empty());
    assert_eq!(kept(&state, &dsp).await, [busy]);

    // Notifications turned off, their browsers are let go of as the next message would go.
    notifications(&state, &dsp, &owner, false, 1).await;
    support::writes(&state, &dsp, TEAM, "U0FIXCASEY", "Hello?").await;
    let new = look(&state, &dsp, &mut seen, &mut revision).await.unwrap();
    assert_eq!(new.len(), 1);
    tell(&state, &dsp, new).await.unwrap();
    assert_eq!(fixture::pushed(busy).len(), 1);
    assert!(kept(&state, &dsp).await.is_empty());
}
