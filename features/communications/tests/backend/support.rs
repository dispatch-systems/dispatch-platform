//! What every Communications backend test installs, and how one starts. A test binary holds one
//! registry, so each installs the same: Slack and Communications. A test starts with a platform
//! with one DSP, every feature on for it, and one of its fixture workspaces connected, its
//! members read: the workspace's admin has approved all the app asks for.
use crate::backend::{conversations, storage::CommunicationsStore};
use crate::api::types::Message;
use dispatch_core::{
    State,
    accounts::{Context, SessionLifetime},
    foundation::config::Site,
    server::http::route::Dsp,
    testing,
};
use dispatch_slack::{kit::Bot, testing as slack};
use std::sync::Arc;

/// Reading the DSP's conversations and answering them.
pub const USE: Dsp = Dsp("communications.use");
/// What the workspace's admin approved: what Slack and Communications ask for.
const APPROVED: &[&str] = &[
    "users:read",
    "users:read.email",
    "chat:write",
    "chat:write.customize",
    "im:history",
    "im:write",
    "files:read",
    "files:write",
    "im:read",
];

/// Installs Slack and Communications.
pub fn install() {
    testing::install(&[], &[&dispatch_slack::FEATURE, &crate::FEATURE]);
}
/// A platform with one DSP, and its server's state, with the DSP's workspace `team` connected
/// and its members read.
pub async fn connected(team: &str) -> (impl Drop, Arc<State>, Bot) {
    install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let bot = slack::connect(&state, &dsp, team, APPROVED).await;
    slack::read_members(&bot).await.unwrap();
    (root, state, bot)
}
/// The DSP's owner, Test Owner, signed in and acting with Communications, under the name
/// `first` `last` and the email address `email` in place of theirs.
pub async fn member(state: &Arc<State>, dsp: &str, first: &str, last: &str, email: &str) -> Context {
    let raw = state
        .login(
            "owner@example.test".into(),
            "test-password-long".into(),
            "127.0.0.1".into(),
            "test".into(),
            SessionLifetime::Standard,
            Site::Admin,
        )
        .await
        .unwrap();
    let (dsp, first, last, email) = (dsp.to_owned(), first.to_owned(), last.to_owned(), email.to_owned());
    state
        .read(move |db| {
            let mut auth = db.authenticate(&raw, &Site::Admin)?;
            (auth.user.first_name, auth.user.last_name, auth.user.email) = (first, last, email);
            db.context(&auth, &dsp, USE.0)
        })
        .await
        .unwrap()
}
/// The texts of the conversation's messages, in the order written.
pub async fn texts(state: &Arc<State>, dsp: &str, conversation: &str) -> Vec<String> {
    messages(state, dsp, conversation)
        .await
        .into_iter()
        .map(|message| message.text)
        .collect()
}
/// The conversation's messages, as the page reads them.
pub async fn messages(state: &Arc<State>, dsp: &str, conversation: &str) -> Vec<Message> {
    let (shared, dsp, conversation) = (state.clone(), dsp.to_owned(), conversation.to_owned());
    state
        .read(move |db| Ok(conversations::opened(db, &shared, &dsp, &conversation, None)?.messages))
        .await
        .unwrap()
}
/// The conversation with the workspace `team`'s member `user`.
pub async fn conversation_with(
    state: &Arc<State>,
    dsp: &str,
    team: &str,
    user: &str,
) -> Option<crate::backend::storage::Conversation> {
    let (dsp, team, user) = (dsp.to_owned(), team.to_owned(), user.to_owned());
    state
        .read(move |db| db.communications_conversation_with(&dsp, &team, &user))
        .await
        .unwrap()
}
