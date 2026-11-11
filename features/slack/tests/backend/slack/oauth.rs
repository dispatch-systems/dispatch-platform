use super::super::Client;
use super::*;
use crate::backend::{app, connection, support};
use dispatch_core::testing;

#[tokio::test]
async fn custom_oauth_scopes_and_workspace_updates_belong_only_to_the_owner_dsp() {
    let (_root, state, other) = support::platform();
    let owner = state
        .run(|db| {
            let actor = testing::platform_owner(db);
            let dsp = db.new_dsp("Custom owner", "UTC", &actor, false)?;
            db.set_code(&dsp.id, "dsp42")?;
            Ok(dsp.id)
        })
        .await
        .unwrap();
    let shared: Vec<_> = app::scopes()
        .into_iter()
        .filter(|scope| *scope != "reactions:write")
        .collect();
    support::connect(&state, &owner, "T0CUSTOM1", &shared).await;
    support::connect(&state, &other, "T0CUSTOM2", &shared).await;
    // Constructing the live URL calls no provider and uses no real credentials.
    let slack = Slack::Live(Box::leak(Box::new(Client {
        id: "fixture-client".into(),
        secret: String::new(),
        signing: String::new(),
    })));
    state
        .read(move |db| {
            for (dsp, team, extra) in [(&owner, "T0CUSTOM1", true), (&other, "T0CUSTOM2", false)] {
                let install = connection::new_state(dsp, connection::Started::Own)?;
                let url = connection::install_url(db, &slack, &install, Some(team.into()))?;
                let url = url::Url::parse(&url).unwrap();
                let scopes = url
                    .query_pairs()
                    .find(|(key, _)| key == "scope")
                    .unwrap()
                    .1
                    .into_owned();
                assert_eq!(
                    scopes.split(',').any(|scope| scope == "reactions:write"),
                    extra
                );
                let account = connection::account(db, dsp)?;
                assert_eq!(account.workspaces[0].needs_update, extra);
                assert!(
                    scopes.split(',').any(|scope| scope == "files:read"),
                    "shared integrations keep their scopes"
                );
            }
            Ok(())
        })
        .await
        .unwrap();
}
