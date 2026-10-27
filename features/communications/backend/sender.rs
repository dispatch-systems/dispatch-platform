//! Who a message from someone at the DSP goes out as in Slack: the full name on their Dispatch
//! account, and their Slack profile photo, read from the member of the DSP's workspaces with
//! their email address, the conversation's workspace first. Slack's own drawn photo, which a
//! profile has until its member gives it one, isn't theirs. With no photo, or no member with
//! their email address, it goes out under the image of their initials Dispatch publishes. Each
//! photo found, or found missing, is remembered in memory for an hour, never kept.
use super::initials;
use dispatch_core::{State, accounts::api::types::PublicUser, db::now};
use dispatch_slack::kit::{self, Bot};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

/// How long a photo found, or found missing, is remembered, in milliseconds.
const REMEMBERED_MS: i64 = 60 * 60 * 1000;
/// The most of a name Slack shows a message under.
const NAME_MOST: usize = 80;

/// The name and photo a message goes out under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    /// The address of the photo, if there's one to show.
    pub icon: Option<String>,
}

/// Who `user`, at the DSP, sends a message in its workspace `team` as.
pub async fn identity(state: &Arc<State>, dsp: &str, team: &str, user: &PublicUser) -> Identity {
    let letters = initials::of(&user.first_name, &user.last_name);
    named(
        state,
        dsp,
        team,
        &name(user),
        Some(&user.email),
        letters.as_deref(),
    )
    .await
}
/// The name a message from `user` goes out under: their full name, as Slack shows one.
pub fn name(user: &PublicUser) -> String {
    full(&user.name())
}
/// The name a message from someone at the DSP whose full name is `name` goes out under, as Slack
/// shows one.
pub fn full(name: &str) -> String {
    name.trim().chars().take(NAME_MOST).collect()
}
/// Who someone at the DSP named `name` sends a message in its workspace `team` as: their Slack
/// photo by their email address `email`, while they have one, else the image of their initials
/// `letters`, if any.
pub async fn named(
    state: &Arc<State>,
    dsp: &str,
    team: &str,
    name: &str,
    email: Option<&str>,
    letters: Option<&str>,
) -> Identity {
    let photo = match email {
        Some(email) => photo(state, dsp, team, email).await,
        None => None,
    };
    let icon =
        photo.or_else(|| letters.map(|letters| initials::address(&state.config.origin, letters)));
    Identity {
        name: name.to_owned(),
        icon,
    }
}

/// The Slack profile photo of the member of the DSP's workspaces with the email address
/// `email`, in the workspace `team` first: none for no member, a photo Slack drew, or Slack
/// not answering.
async fn photo(state: &Arc<State>, dsp: &str, team: &str, email: &str) -> Option<String> {
    let at = dsp.to_owned();
    let members = state.read(move |db| kit::members(db, &at)).await.ok()?;
    let (team, user) = members
        .into_iter()
        .filter(|(_, member)| {
            member.active
                && member
                    .email
                    .as_deref()
                    .is_some_and(|theirs| theirs.eq_ignore_ascii_case(email))
        })
        .min_by_key(|(of, _)| of != team)
        .map(|(of, member)| (of, member.user))?;
    let key = (dsp.to_owned(), team.clone(), user.clone());
    if let Some((photo, at)) = remembered().get(&key)
        && now() - at < REMEMBERED_MS
    {
        return photo.clone();
    }
    let info = Bot::new(state, dsp, &team)
        .call("users.info", &json!({"user": user}))
        .await
        .ok()?;
    let profile = &info["user"]["profile"];
    let photo = (profile["is_custom_image"] == true)
        .then(|| {
            profile["image_512"]
                .as_str()
                .or(profile["image_192"].as_str())
        })
        .flatten()
        .map(str::to_owned);
    remembered().insert(key, (photo.clone(), now()));
    photo
}

/// Each member's photo found, or found missing, by DSP, workspace and member, with when.
type Remembered = HashMap<(String, String, String), (Option<String>, i64)>;
fn remembered() -> std::sync::MutexGuard<'static, Remembered> {
    static REMEMBERED: LazyLock<Mutex<Remembered>> = LazyLock::new(Default::default);
    REMEMBERED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}
