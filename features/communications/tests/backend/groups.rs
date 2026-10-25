//! Messaging a group: each person gets it once, in their own direct message, under the sender's
//! name; their answers are the group's until the DSP writes to them alone, and their
//! conversations are listed under it while nothing they wait on is older; sending picks up after
//! a restart without a second copy; one that can't go isn't sent; and the groups to choose from
//! hold someone each.
use super::*;
use crate::backend::{
    reply,
    storage::ConversationState,
    support::{self, DRIVERS, USE},
};
use crate::integrations::messages::ID;
use dispatch_core::{
    accounts::Context,
    db::now,
    foundation::clock,
    manifest::people::{Person, PersonRole},
};
use dispatch_routes_contract::RouteDay;
use dispatch_slack::{kit, testing as slack};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
use storage::GroupStore;

const DRIVERS_TEAM: &str = "T0FIXTURE2";
const OPERATIONS: &str = "T0FIXTURE1";

/// Driver Match's person `code`, named `name`, in the role `role`, holding the Slack accounts
/// `slack` (each a workspace and member ID) and the drivers' IDs `ids`.
fn person(
    code: &str,
    name: &str,
    role: PersonRole,
    slack: &[(&str, &str)],
    ids: &[&str],
) -> Person {
    Person {
        code: code.into(),
        name: name.into(),
        names: vec![name.into()],
        role,
        title: None,
        ids: BTreeMap::from([
            (
                kit::SOURCE,
                slack
                    .iter()
                    .map(|(team, user)| kit::member_id(team, user))
                    .collect(),
            ),
            (DRIVERS, ids.iter().map(|id| (*id).to_owned()).collect()),
        ]),
        emails: Vec::new(),
        phones: Vec::new(),
        confirmed: true,
        photo: None,
    }
}
/// The DSP's owner, sending as Test Owner.
async fn owner(state: &Arc<State>, dsp: &str) -> Context {
    support::member(state, dsp, "Test", "Owner", "owner@example.test").await
}
/// The members `people`, each a workspace and a member ID.
fn members(people: &[(&str, &str)]) -> Vec<(String, String)> {
    people
        .iter()
        .map(|(team, user)| ((*team).to_owned(), (*user).to_owned()))
        .collect()
}
/// Makes a group of `people` named `name`, with what `c` wrote, and sends it.
async fn sent(
    state: &Arc<State>,
    c: &Context,
    name: &str,
    people: &[(&str, &str)],
    text: &str,
) -> crate::api::types::Group {
    let group = send::create(state, c, USE, name, &members(people), text, &[])
        .await
        .unwrap();
    send::drain(state, &c.dsp.id).await;
    group
}
/// What the bot posted in the workspace's direct message with `user` carrying the group message
/// `message`.
fn posts(team: &str, user: &str, message: &str) -> Vec<slack::Posted> {
    slack::posted(team, &format!("D{}", &user[1..]))
        .into_iter()
        .filter(|post| post.metadata["event_payload"]["message"] == message)
        .collect()
}
/// The group's first message, as kept.
async fn first_message(state: &Arc<State>, dsp: &str, group: &str) -> storage::GroupMessage {
    let (dsp, group) = (dsp.to_owned(), group.to_owned());
    state
        .read(move |db| Ok(db.communications_group_messages(&dsp, &group)?.remove(0)))
        .await
        .unwrap()
}
/// The conversation with the workspace's member `user`, as kept.
async fn with(
    state: &Arc<State>,
    dsp: &str,
    team: &str,
    user: &str,
) -> crate::backend::storage::Conversation {
    support::conversation_with(state, dsp, team, user)
        .await
        .unwrap()
}
/// The group `id`, as the list reads it.
async fn listed(state: &Arc<State>, dsp: &str, id: &str) -> crate::api::types::Group {
    let (shared, dsp, id) = (state.clone(), dsp.to_owned(), id.to_owned());
    state
        .read(move |db| one(db, &shared, &dsp, &id))
        .await
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_group_message_goes_once_to_each_person_in_their_own_direct_message() {
    let (_root, state, bot) = support::connected(DRIVERS_TEAM).await;
    let dsp = bot.dsp.clone();
    support::workspace(&state, &dsp, OPERATIONS).await;
    // Jordan is in both workspaces, one person to Driver Match.
    let jordan = person(
        "jordan",
        "Jordan Ellis",
        PersonRole::Driver,
        &[(DRIVERS_TEAM, "U0FIXJORDAN"), (OPERATIONS, "U0FIXJORDAN")],
        &[],
    );
    support::know(&state, &dsp, vec![jordan], Vec::new(), Vec::new());
    let c = owner(&state, &dsp).await;
    let picked = [
        (DRIVERS_TEAM, "U0FIXJORDAN"),
        (OPERATIONS, "U0FIXJORDAN"),
        (DRIVERS_TEAM, "U0FIXCAM"),
    ];
    let group = sent(
        &state,
        &c,
        "9:20 AM wave",
        &picked,
        "Station is 20 minutes behind",
    )
    .await;
    assert_eq!((group.name.as_str(), group.people), ("9:20 AM wave", 2));
    let message = first_message(&state, &dsp, &group.id).await;

    // Each in their own direct message, once, under the sender's name and initials.
    let jordans = posts(DRIVERS_TEAM, "U0FIXJORDAN", &message.id);
    assert_eq!(jordans.len(), 1);
    assert_eq!(jordans[0].text, "Station is 20 minutes behind");
    assert_eq!(jordans[0].username.as_deref(), Some("Test Owner"));
    assert!(jordans[0].icon_url.as_deref().unwrap().ends_with("/TO.png"));
    assert!(posts(OPERATIONS, "U0FIXJORDAN", &message.id).is_empty());
    assert_eq!(posts(DRIVERS_TEAM, "U0FIXCAM", &message.id).len(), 1);

    // It's in each one's own conversation, marked with its group, and has gone to both.
    let cam = with(&state, &dsp, DRIVERS_TEAM, "U0FIXCAM").await;
    let messages = support::messages(&state, &dsp, &cam.id).await;
    let tagged = messages.last().unwrap().group.as_ref().unwrap();
    assert_eq!(
        (tagged.id.as_str(), tagged.name.as_str()),
        (group.id.as_str(), "9:20 AM wave")
    );
    assert_eq!(cam.group.as_deref(), Some(group.id.as_str()));
    let shown = listed(&state, &dsp, &group.id).await;
    let progress = shown.last.unwrap().progress.unwrap();
    assert_eq!((progress.people, progress.sent, progress.failed), (2, 2, 0));
}

#[tokio::test(start_paused = true)]
async fn answers_are_the_groups_until_the_dsp_writes_to_them_alone() {
    let (_root, state, bot) = support::connected(DRIVERS_TEAM).await;
    let dsp = bot.dsp.clone();
    let c = owner(&state, &dsp).await;
    // Casey asked something before the group message, still unanswered.
    support::writes(&state, &dsp, DRIVERS_TEAM, "U0FIXCASEY", "Where do I park?").await;
    let picked = [(DRIVERS_TEAM, "U0FIXJORDAN"), (DRIVERS_TEAM, "U0FIXCASEY")];
    let group = sent(&state, &c, "Drivers", &picked, "Pick up your vests").await;
    // A group message answers no one: Casey still waits, on their own.
    let casey = with(&state, &dsp, DRIVERS_TEAM, "U0FIXCASEY").await;
    assert_eq!(
        (casey.state, casey.group),
        (ConversationState::Waiting, None)
    );
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(jordan.group.as_deref(), Some(group.id.as_str()));
    assert_eq!(
        listed(&state, &dsp, &group.id).await.state,
        ConversationState::Responded
    );

    // Jordan answers: waiting, under the group, which waits; Casey's answer is the group's too.
    support::writes(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN", "ok").await;
    support::writes(&state, &dsp, DRIVERS_TEAM, "U0FIXCASEY", "Got it").await;
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(jordan.state, ConversationState::Waiting);
    assert_eq!(jordan.group.as_deref(), Some(group.id.as_str()));
    let shown = listed(&state, &dsp, &group.id).await;
    assert_eq!(shown.state, ConversationState::Waiting);
    assert_eq!(shown.waiting_since, jordan.waiting_since);
    assert_eq!(shown.last.unwrap().text, "Got it");
    let (shared, at, id) = (state.clone(), dsp.clone(), group.id.clone());
    let read = state
        .read(move |db| {
            Ok((
                opened(db, &shared, &at, &id, None)?,
                people(db, &shared, &at, &id)?,
            ))
        })
        .await
        .unwrap();
    let texts: Vec<_> = read
        .0
        .entries
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(texts, ["Pick up your vests", "ok", "Got it"]);
    let states: Vec<_> = read.1.people.iter().map(|each| each.state).collect();
    assert_eq!(
        states,
        [GroupPersonState::Replied, GroupPersonState::Replied]
    );
    // Written to again as a group, Jordan's answer still waits under it.
    send::again(&state, &c, USE, &group.id, "Vests are by the door", &[])
        .await
        .unwrap();
    send::drain(&state, &dsp).await;
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(jordan.state, ConversationState::Waiting);
    assert_eq!(jordan.group.as_deref(), Some(group.id.as_str()));

    // Answered alone, Jordan's is their own again, and the group waits on no one.
    reply::send(&state, &c, USE, &jordan.id, "Thanks", &[])
        .await
        .unwrap();
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(
        (jordan.state, jordan.group),
        (ConversationState::Responded, None)
    );
    assert_eq!(
        listed(&state, &dsp, &group.id).await.state,
        ConversationState::Responded
    );
    // What they write after that answers no group.
    support::writes(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN", "One more thing").await;
    let shown = listed(&state, &dsp, &group.id).await;
    assert_eq!(shown.last.unwrap().text, "Vests are by the door");

    // A later group message moves them under it; Done on the group marks each waiting under it
    // done.
    reply::send(&state, &c, USE, &jordan.id, "Sure", &[])
        .await
        .unwrap();
    let later = sent(
        &state,
        &c,
        "Jordan Ellis",
        &[(DRIVERS_TEAM, "U0FIXJORDAN")],
        "Van 214",
    )
    .await;
    support::writes(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN", "On it").await;
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(jordan.group.as_deref(), Some(later.id.as_str()));
    let (shared, at, id) = (state.clone(), dsp.clone(), later.id.clone());
    let marked = state
        .read(move |db| done(db, &shared, &at, &id, "someone"))
        .await
        .unwrap();
    assert_eq!(marked.state, ConversationState::Responded);
    let jordan = with(&state, &dsp, DRIVERS_TEAM, "U0FIXJORDAN").await;
    assert_eq!(jordan.state, ConversationState::Done);

    // The list: a conversation listed under a group is the group's row, the rest their own.
    let (shared, at) = (state.clone(), dsp.clone());
    let overview = state
        .read(move |db| conversations::overview(db, &shared, &at, "nobody", None))
        .await
        .unwrap();
    let rows: Vec<_> = overview
        .conversations
        .iter()
        .map(|each| (each.with.name.as_str(), each.group_id.is_some()))
        .collect();
    assert!(rows.contains(&("Casey Rivera", false)));
    assert!(!rows.iter().any(|(name, _)| *name == "Jordan Ellis"));
    let groups: Vec<_> = overview
        .groups
        .iter()
        .map(|each| each.id.as_str())
        .collect();
    assert_eq!(groups, [later.id.as_str(), group.id.as_str()]);
}

#[tokio::test(start_paused = true)]
async fn sending_picks_up_after_a_restart_without_a_second_copy() {
    let (_root, state, bot) = support::connected(DRIVERS_TEAM).await;
    let dsp = bot.dsp.clone();
    let c = owner(&state, &dsp).await;
    let picked = members(&[(DRIVERS_TEAM, "U0FIXCAM"), (DRIVERS_TEAM, "U0FIXCASEY")]);
    let group = send::create(&state, &c, USE, "Wave", &picked, "Running late", &[])
        .await
        .unwrap();
    let message = first_message(&state, &dsp, &group.id).await;
    // Before the server stopped, a try posted to Cam and kept nothing, and one to Casey was
    // claimed and posted nothing; both claims have since run out.
    let at = dsp.clone();
    let due = state
        .read(move |db| db.due_communications_deliveries(&at, now(), 10))
        .await
        .unwrap();
    for item in &due {
        let (at, item) = (dsp.clone(), item.clone());
        state
            .run(move |db| {
                db.claim_communications_delivery(&at, &item, now(), now() - 1)
                    .map(drop)
            })
            .await
            .unwrap();
    }
    let posted = bot
        .call(
            "chat.postMessage",
            &json!({"channel": "D0FIXCAM", "text": "Running late", "metadata":
                {"event_type": "dispatch_group_message", "event_payload": {"message": message.id}}}),
        )
        .await
        .unwrap();
    send::drain(&state, &dsp).await;
    // Cam has the one post, kept; Casey gets theirs.
    assert_eq!(posts(DRIVERS_TEAM, "U0FIXCAM", &message.id).len(), 1);
    assert_eq!(posts(DRIVERS_TEAM, "U0FIXCASEY", &message.id).len(), 1);
    let cam = with(&state, &dsp, DRIVERS_TEAM, "U0FIXCAM").await;
    let kept = support::messages(&state, &dsp, &cam.id).await;
    assert_eq!(kept.last().unwrap().ts, posted["ts"].as_str().unwrap());
    let progress = listed(&state, &dsp, &group.id)
        .await
        .last
        .unwrap()
        .progress
        .unwrap();
    assert_eq!((progress.sent, progress.failed), (2, 0));
}

#[tokio::test]
async fn one_that_cant_go_is_not_sent() {
    let (_root, state, bot) = support::connected(DRIVERS_TEAM).await;
    let dsp = bot.dsp.clone();
    let c = owner(&state, &dsp).await;
    // Someone no longer in the workspace can't be picked.
    let gone = members(&[(DRIVERS_TEAM, "U0FIXJESSE")]);
    let refused = send::create(&state, &c, USE, "Gone", &gone, "Hello", &[])
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_contact_not_found");
    // Switched off in the workspace before it went: not sent.
    let picked = members(&[(DRIVERS_TEAM, "U0FIXCAM")]);
    let group = send::create(&state, &c, USE, "Cam", &picked, "Hello", &[])
        .await
        .unwrap();
    slack::switch(&state, &dsp, DRIVERS_TEAM, ID, false).await;
    send::drain(&state, &dsp).await;
    let progress = listed(&state, &dsp, &group.id)
        .await
        .last
        .unwrap()
        .progress
        .unwrap();
    assert_eq!((progress.people, progress.sent, progress.failed), (1, 0, 1));
    let (shared, at, id) = (state.clone(), dsp.clone(), group.id.clone());
    let panel = state
        .read(move |db| people(db, &shared, &at, &id))
        .await
        .unwrap();
    assert_eq!(panel.people[0].state, GroupPersonState::NotSent);
    assert_eq!(panel.sent_by, "Test Owner");
}

#[tokio::test]
async fn the_groups_to_choose_from_hold_someone_each() {
    let (_root, state, bot) = support::connected(DRIVERS_TEAM).await;
    let dsp = bot.dsp.clone();
    let at = dsp.clone();
    let zone = state
        .read(move |db| Ok(db.find_dsp(&at)?.timezone))
        .await
        .unwrap();
    let today = clock::today(zone.parse().unwrap()).to_string();
    let wave = |hour: i64| 1_792_482_000_000 + hour * 3_600_000;
    let route = |driver: &str, at: i64| RouteDay {
        day: today.clone(),
        driver_id: driver.into(),
        route_code: None,
        status: None,
        progress: None,
        vin: None,
        wave_start_at: Some(at),
        planned_departure_at: None,
        departed_at: None,
        first_delivery_at: None,
        last_delivery_at: None,
        session_end_at: None,
        stops: 0,
        stops_completed: 0,
        delivered: 0,
        not_delivered: 0,
        removed: 0,
        breaks_secs: None,
        overtime_secs: None,
        undelivered: Vec::new(),
    };
    let people = vec![
        person(
            "jordan",
            "Jordan Ellis",
            PersonRole::Driver,
            &[(DRIVERS_TEAM, "U0FIXJORDAN")],
            &["A1"],
        ),
        person(
            "cam",
            "Cameron Hayes",
            PersonRole::Driver,
            &[(DRIVERS_TEAM, "U0FIXCAM")],
            &["A2"],
        ),
        person(
            "casey",
            "Casey Rivera",
            PersonRole::Office,
            &[(DRIVERS_TEAM, "U0FIXCASEY")],
            &[],
        ),
        // On a route today, with no Slack account: not one of them.
        person("sam", "Sam Ortiz", PersonRole::Driver, &[], &["A3"]),
    ];
    let routes = vec![
        route("A2", wave(1)),
        route("A1", wave(0)),
        route("A3", wave(2)),
    ];
    support::know(&state, &dsp, people, routes, Vec::new());
    let read = || {
        let (shared, at) = (state.clone(), dsp.clone());
        state.read(move |db| {
            let zone = db.find_dsp(&at)?.timezone;
            choices::choices(db, &shared, &at, &zone)
        })
    };
    let held = |choices: &[crate::api::types::GroupChoice]| -> Vec<(GroupChoiceKind, Vec<String>)> {
        choices
            .iter()
            .map(|choice| (choice.kind, choice.people.clone()))
            .collect()
    };
    let keys = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();

    // Today: everyone on a route, then each wave in order; by role, none for helpers; one
    // workspace, no workspaces.
    let choices = read().await.unwrap();
    assert_eq!(
        held(&choices.today),
        [
            (GroupChoiceKind::Route, keys(&["cam", "jordan"])),
            (GroupChoiceKind::Wave, keys(&["jordan"])),
            (GroupChoiceKind::Wave, keys(&["cam"])),
        ]
    );
    let waves: Vec<_> = choices.today.iter().map(|choice| choice.wave_at).collect();
    assert_eq!(waves, [None, Some(wave(0)), Some(wave(1))]);
    let roles: Vec<_> = choices.roles.iter().map(|choice| choice.role).collect();
    assert_eq!(roles, [Some(PersonRole::Driver), Some(PersonRole::Office)]);
    assert!(choices.workspaces.is_empty());

    // Two workspaces: everyone, and each workspace's, by name. Routes off: nothing today.
    support::workspace(&state, &dsp, OPERATIONS).await;
    support::switch(&state, &dsp, "driving", false).await;
    let choices = read().await.unwrap();
    assert!(choices.today.is_empty());
    let kinds: Vec<_> = choices
        .workspaces
        .iter()
        .map(|choice| (choice.kind, choice.team_id.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [
            (GroupChoiceKind::Everyone, None),
            (GroupChoiceKind::Workspace, Some(OPERATIONS)),
            (GroupChoiceKind::Workspace, Some(DRIVERS_TEAM)),
        ]
    );
    let morgan = kit::member_id(OPERATIONS, "U0FIXOWNER");
    assert!(choices.workspaces[1].people.contains(&morgan));
    assert!(!choices.workspaces[2].people.contains(&morgan));
}
