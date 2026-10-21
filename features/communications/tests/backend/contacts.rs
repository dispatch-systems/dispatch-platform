//! Whom someone at the DSP can message: the active members of its workspaces, or of one, found
//! by a name typed, misspelt or not.
use super::*;
use crate::backend::support;
use std::sync::Arc;

/// The DSP's contacts, of the workspace `team` or every one, or those like `typed`.
async fn listed(
    state: &Arc<State>,
    dsp: &str,
    team: Option<&'static str>,
    typed: Option<&'static str>,
) -> Vec<Contact> {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state
        .read(move |db| contacts(db, &shared, &dsp, team, typed))
        .await
        .unwrap()
        .contacts
}
/// Their names, in order.
fn names(contacts: &[Contact]) -> Vec<&str> {
    contacts
        .iter()
        .map(|contact| contact.person.name.as_str())
        .collect()
}

#[tokio::test]
async fn the_active_members_of_the_workspaces_are_contacts_found_by_a_misspelt_name() {
    let (_root, state, bot) = support::connected("T0FIXTURE2").await;
    support::workspace(&state, &bot.dsp, "T0FIXTURE1").await;
    // Bots, Slackbot and someone deactivated aren't; a guest is; someone in both workspaces is
    // a contact in each.
    assert_eq!(
        names(&listed(&state, &bot.dsp, None, None).await),
        [
            "Cameron Hayes",
            "Casey Rivera",
            "Drew Sullivan",
            "Jordan Ellis",
            "Jordan Ellis",
            "Morgan Lane"
        ]
    );
    let found = listed(&state, &bot.dsp, None, Some("camron hays")).await;
    assert_eq!(found[0].person.name, "Cameron Hayes");
    assert_eq!(found[0].person.user_id, "U0FIXCAM");
    assert_eq!(found[0].matched, [(0, 3), (4, 7), (8, 11), (12, 13)]);
    // A workspace's are its own members alone.
    assert_eq!(
        names(&listed(&state, &bot.dsp, Some("T0FIXTURE1"), None).await),
        ["Jordan Ellis", "Morgan Lane"]
    );
    assert_eq!(
        names(&listed(&state, &bot.dsp, Some("T0FIXTURE2"), None).await),
        [
            "Cameron Hayes",
            "Casey Rivera",
            "Drew Sullivan",
            "Jordan Ellis"
        ]
    );
}
