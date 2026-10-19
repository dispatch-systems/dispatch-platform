//! What Slack exposes, as a feature using it finds it: only while it's on for the DSP, each
//! account the IDs asked for name, as fixture mode's Slack lists the members of two workspaces.
use crate::backend::{
    app::SCOPES,
    members::read,
    storage::SlackStore,
    support::{connect, platform},
};
use dispatch_core::testing;
use dispatch_slack_contract::{self as chat, ChatAccount};

const OPERATIONS: &str = "T0FIXTURE1";
const DRIVERS: &str = "T0FIXTURE2";

#[tokio::test]
async fn chat_accounts_are_the_ones_asked_for_only_while_slack_is_on() {
    let (_root, state, dsp) = platform();
    for team in [OPERATIONS, DRIVERS] {
        read(&connect(&state, &dsp, team, SCOPES).await)
            .await
            .unwrap();
    }
    let of = dsp.clone();
    let (accounts, refused) = state
        .run(move |db| {
            db.label_slack_workspace(&of, DRIVERS, Some("Drivers"))?;
            db.break_slack_workspace(&of, OPERATIONS)?;
            let offered = chat::for_dsp(db, &of)?.expect("Slack is on");
            let asked = [
                format!("{DRIVERS}:U0FIXJORDAN"),
                format!("{OPERATIONS}:U0FIXJORDAN"),
                format!("{DRIVERS}:U0FIXCAM"),
                format!("{DRIVERS}:U0FIXDREW"),
                format!("{DRIVERS}:U0FIXGONE"),
                // Nobody's account.
                format!("{DRIVERS}:U0NOBODY"),
            ];
            let many: Vec<String> = (0..33).map(|n| format!("{DRIVERS}:U{n}")).collect();
            offered.accounts(db, &of, &many[..32])?;
            let refused = offered.accounts(db, &of, &many).unwrap_err();
            Ok((offered.accounts(db, &of, &asked)?, refused.code))
        })
        .await
        .unwrap();
    // Jordan, in both, by each account; the workspace by its name in Dispatch, else in Slack.
    let account = |team: &str, user: &str| ChatAccount {
        id: format!("{team}:{user}"),
        workspace: if team == DRIVERS { "Drivers" } else { team }.into(),
        user: user.into(),
        display_name: None,
        active: true,
        guest: false,
        broken: team == OPERATIONS,
    };
    assert_eq!(
        accounts,
        [
            account(DRIVERS, "U0FIXJORDAN"),
            account(OPERATIONS, "U0FIXJORDAN"),
            ChatAccount {
                display_name: Some("Cam".into()),
                ..account(DRIVERS, "U0FIXCAM")
            },
            ChatAccount {
                guest: true,
                ..account(DRIVERS, "U0FIXDREW")
            },
            ChatAccount {
                active: false,
                ..account(DRIVERS, "U0FIXGONE")
            },
        ]
    );
    // 32 IDs are read at once, and one more is refused.
    assert_eq!(refused, "invalid_input");
    let offered = state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.set_feature(&dsp, "slack", false, &owner)?;
            Ok(chat::for_dsp(db, &dsp)?.is_some())
        })
        .await
        .unwrap();
    assert!(!offered);
}
