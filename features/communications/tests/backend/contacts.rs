//! Whom someone at the DSP can message: the active members of its workspaces, or of one, found
//! by a name typed, misspelt or not.
use super::*;
use crate::backend::support;

#[tokio::test]
async fn the_active_members_of_the_workspaces_are_contacts_found_by_a_misspelt_name() {
    let (_root, state, bot) = support::connected("T0FIXTURE2").await;
    let (shared, dsp) = (state.clone(), bot.dsp.clone());
    let list = move |team: Option<&'static str>, typed: Option<&'static str>| {
        let (shared, dsp) = (shared.clone(), dsp.clone());
        async move {
            let state = shared.clone();
            shared
                .read(move |db| contacts(db, &state, &dsp, team, typed))
                .await
                .unwrap()
                .contacts
        }
    };
    let find = |typed| list(None, typed);
    // Bots, Slackbot and someone deactivated aren't; a guest is.
    let names: Vec<String> = find(None)
        .await
        .into_iter()
        .map(|contact| contact.person.name)
        .collect();
    assert_eq!(
        names,
        [
            "Cameron Hayes",
            "Casey Rivera",
            "Drew Sullivan",
            "Jordan Ellis"
        ]
    );
    let found = find(Some("camron hays")).await;
    assert_eq!(found[0].person.name, "Cameron Hayes");
    assert_eq!(found[0].person.user_id, "U0FIXCAM");
    assert_eq!(found[0].matched, [(0, 3), (4, 7), (8, 11), (12, 13)]);
    // A workspace's are its own members alone.
    assert_eq!(list(Some("T0FIXTURE2"), None).await.len(), names.len());
    assert!(list(Some("T0ELSEWHERE"), None).await.is_empty());
}
