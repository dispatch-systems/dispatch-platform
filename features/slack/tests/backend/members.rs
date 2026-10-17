//! Reading a workspace's members, as fixture mode's Slack lists them: only once its admin lets
//! the bot, without bots, and as Driver Match reads them, each by their own ID with their
//! accounts in the DSP's other workspaces.
use super::*;
use crate::backend::{
    app::SCOPES as APP,
    slack::Tokens,
    storage::{SlackStore, Workspace},
};
use dispatch_core::{db::now, manifest::people::Named, testing};

const OPERATIONS: &str = "T0FIXTURE1";
const DRIVERS: &str = "T0FIXTURE2";

/// A platform with one DSP, and its server's state.
fn platform() -> (impl Drop, Arc<State>, String) {
    crate::backend::support::install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    (root, State::new(config).unwrap(), dsp)
}
/// Connects the DSP's workspace `team`, its admin having let the bot do what `scopes` say.
async fn connect(state: &Arc<State>, dsp: &str, team: &str, scopes: &[&str]) -> Bot {
    let workspace = Workspace {
        team: team.into(),
        name: team.into(),
        broken: false,
        scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        bot_user: "U0FIXTUREBOT".into(),
        app: "A0FIXTURE".into(),
        connected_by: None,
        connected_at: "2026-01-01T00:00:00Z".into(),
        label: None,
    };
    let tokens = Tokens {
        access: format!("fixture-access:{team}:test"),
        refresh: None,
        expires_at: Some(now() + 3_600_000),
    };
    let of = dsp.to_owned();
    state
        .run(move |db| db.save_slack_workspace(&of, &workspace, &tokens))
        .await
        .unwrap();
    Bot::new(state, dsp, team)
}
async fn kept(state: &Arc<State>, dsp: &str) -> Vec<(String, Member)> {
    let of = dsp.to_owned();
    state.read(move |db| db.slack_members(&of)).await.unwrap()
}

#[tokio::test]
async fn members_are_read_once_the_admin_lets_the_bot_and_never_bots() {
    let (_root, state, dsp) = platform();
    let without: Vec<&str> = APP
        .iter()
        .copied()
        .filter(|s| !SCOPES.contains(s))
        .collect();
    let bot = connect(&state, &dsp, DRIVERS, &without).await;
    read(&bot).await.unwrap();
    assert!(kept(&state, &dsp).await.is_empty());
    let bot = connect(&state, &dsp, DRIVERS, APP).await;
    read(&bot).await.unwrap();
    let members = kept(&state, &dsp).await;
    let names: Vec<&str> = members.iter().map(|(_, m)| m.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Cameron Hayes",
            "Casey Rivera",
            "Drew Sullivan",
            "Jesse Kline",
            "Jordan Ellis"
        ]
    );
    let member = |name: &str| &members.iter().find(|(_, m)| m.name == name).unwrap().1;
    assert_eq!(member("Cameron Hayes").display_name.as_deref(), Some("Cam"));
    assert!(member("Drew Sullivan").guest);
    assert!(!member("Jesse Kline").active);
    // Someone deactivated in Slack is told of as changed, and kept inactive.
    let event = json!({"user": {"id": "U0FIXCASEY", "deleted": true,
        "profile": {"real_name": "Casey Rivera", "phone": "(612) 555-0104"}}});
    told(&bot, &event).await.unwrap();
    assert!(!member_of(&kept(&state, &dsp).await, "Casey Rivera").active);
    // Disconnected, the workspace's members go with it, and a read under way as it went keeps
    // none of them.
    let of = dsp.clone();
    state
        .run(move |db| db.remove_slack_workspace(&of, DRIVERS))
        .await
        .unwrap();
    assert!(kept(&state, &dsp).await.is_empty());
    let (of, read) = (
        dsp.clone(),
        members.into_iter().map(|(_, m)| m).collect::<Vec<_>>(),
    );
    state
        .run(move |db| db.keep_slack_members(&of, DRIVERS, &read))
        .await
        .unwrap();
    assert!(kept(&state, &dsp).await.is_empty());
}
#[test]
fn a_member_whose_profile_has_no_name_goes_by_the_one_they_chose() {
    let user = |profile: Value| json!({"id": "U0FIXSAM", "name": "sam.k", "profile": profile});
    let name = |profile: Value| member(&user(profile)).unwrap().name;
    assert_eq!(
        name(json!({"real_name": "", "display_name": "Sam K"})),
        "Sam K"
    );
    assert_eq!(name(json!({})), "sam.k");
}
fn member_of<'a>(members: &'a [(String, Member)], name: &str) -> &'a Member {
    &members.iter().find(|(_, m)| m.name == name).unwrap().1
}

#[tokio::test]
async fn driver_match_reads_each_member_with_their_accounts_in_other_workspaces() {
    let (_root, state, dsp) = platform();
    let drivers = connect(&state, &dsp, DRIVERS, APP).await;
    let operations = connect(&state, &dsp, OPERATIONS, APP).await;
    read(&drivers).await.unwrap();
    read(&operations).await.unwrap();
    let named = people(&state, &dsp).await;
    // Jordan, in both, is named by each workspace's member ID, as the same person as the other.
    let [in_operations, in_drivers] =
        [OPERATIONS, DRIVERS].map(|team| format!("{team}:U0FIXJORDAN"));
    for (id, other) in [(&in_operations, &in_drivers), (&in_drivers, &in_operations)] {
        let jordan = named.iter().find(|n| n.id == *id).unwrap();
        assert_eq!(jordan.name, "Jordan Ellis");
        assert!(jordan.listed);
        assert_eq!(jordan.emails, ["jordan.ellis@example.com"]);
        assert_eq!(jordan.same_as, [other.as_str()]);
    }
    // Only the name on their profile is read: the one they go by could be another's.
    let morgan: Vec<&str> = named
        .iter()
        .filter(|n| n.id == format!("{OPERATIONS}:U0FIXOWNER"))
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(morgan, ["Morgan Lane"]);
    let jesse = named.iter().find(|n| n.name == "Jesse Kline").unwrap();
    assert!(!jesse.listed);
    // A workspace whose access broke lists nobody until it's reconnected.
    let of = dsp.clone();
    state
        .run(move |db| db.break_slack_workspace(&of, OPERATIONS))
        .await
        .unwrap();
    let named = people(&state, &dsp).await;
    let listed = |id: &str| named.iter().find(|n| n.id == id).unwrap().listed;
    assert_eq!([listed(&in_operations), listed(&in_drivers)], [false, true]);
}
/// The DSP's members, as Driver Match reads them.
async fn people(state: &Arc<State>, dsp: &str) -> Vec<Named> {
    let of = dsp.to_owned();
    state
        .read(move |db| (crate::backend::people::PEOPLE.read)(db, &of))
        .await
        .unwrap()
}
