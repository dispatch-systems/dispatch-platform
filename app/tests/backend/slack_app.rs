//! Slack's app as Slack knows it, its manifest: the permissions it asks for and the events it's
//! told of, Slack's own and each of its integrations', and where Slack reaches the server. A
//! change to them is one to make in the app's settings in Slack too, so it shows in review as a
//! change to app/tests/backend/snapshots/slack-app.json.
use crate::{snapshot, testing};
use dispatch_core::manifest::registry;

#[test]
fn slack_asks_for_what_it_and_its_integrations_need() {
    crate::install();
    let (_root, store) = testing::store();
    let mut config = store.config.clone();
    config.origin = "https://admin.dispatch.test".into();
    let slack = registry()
        .features
        .iter()
        .find(|feature| feature.name == "slack")
        .expect("Slack is in the build");
    let commands = slack
        .commands
        .as_ref()
        .expect("Slack has its operator command");
    let manifest = (commands.run)(&config, &["slack-manifest".to_owned()]).unwrap();
    snapshot::check("slack-app.json", &manifest);
}
