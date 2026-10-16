//! What Slack sends Dispatch: its events, and what people do with the app's buttons, forms and
//! shortcuts. Slack signs each request with the app's signing secret, which only it and the
//! server hold, and names the workspace it came from, which is how a request finds its DSP:
//! nothing anyone writes in Slack chooses one. Slack waits three seconds for an answer, so
//! Dispatch answers at once and does the work after, but for the app's removal from a
//! workspace or its tokens' revocation there: those are kept first, and a failure answered as
//! one, so Slack sends them again.
//!
//! Each kind of event and interaction has its line in a routing table below, `on_event` or
//! `on_interaction`, which hands it to the module that handles it, with the bot in its
//! workspace.
use super::{
    app::{EVENTS_PATH, INTERACTIONS_PATH},
    bot::Bot,
    connection, home, members,
    slack::Slack,
    storage::SlackStore,
};
use axum::{
    body::Bytes,
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Error, Result, State,
    db::now,
    foundation::{crypto, observability},
    server::http::{input::Reply, middleware},
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

/// What each event the app is told of does, by its type, for a workspace Dispatch acts in. An
/// event added to `app::EVENTS` is handled here.
async fn on_event(bot: &Bot, kind: &str, event: &Value) -> Result<()> {
    match kind {
        "app_home_opened" => home::opened(bot, event).await,
        "team_join" | "user_change" => members::told(bot, event).await,
        _ => Ok(()),
    }
}
/// What each interaction does, by its type and the button, form or shortcut it names, for a
/// workspace Dispatch acts in, with all Slack sent of it in `payload`. The app has none yet:
/// each will be handled from a `match (kind, action)` here, as events are in `on_event`, with
/// a line such as `("block_actions", feature::ACTION) => feature::clicked(bot, payload).await`.
async fn on_interaction(_bot: &Bot, _kind: &str, _action: &str, _payload: &Value) -> Result<()> {
    Ok(())
}

/// Slack's events, at `/slack/events`.
pub async fn events(state: Arc<State>, request: Request) -> Response {
    let sent = match received(&state, request, EVENTS_PATH).await {
        Ok(body) => serde_json::from_slice::<Value>(&body),
        Err(refused) => return refused,
    };
    let Ok(sent) = sent else {
        return middleware::failure(Error::new("invalid_input", 400));
    };
    match sent["type"].as_str() {
        // Slack checks the address answers it before it sends events there.
        Some("url_verification") => match sent["challenge"].as_str() {
            Some(challenge) if challenge.len() <= 200 => {
                Reply::json(json!({"challenge": challenge})).into_response()
            }
            _ => middleware::failure(Error::new("invalid_input", 400)),
        },
        Some("event_callback") => {
            let id = sent["event_id"].as_str().unwrap_or("").to_owned();
            if !first_time(&id) {
                return StatusCode::OK.into_response();
            }
            let team = sent["team_id"].as_str().unwrap_or("").to_owned();
            let event = sent["event"].clone();
            // The app removed from a workspace, or its tokens revoked there, is kept before
            // Slack has its answer, so a failure has Slack send it again. Anything else is done
            // after: Slack doesn't wait for it.
            if let Some(lifecycle) = Lifecycle::of(&event) {
                return match lifecycle.keep(&state, team).await {
                    Ok(()) => StatusCode::OK.into_response(),
                    Err(error) => {
                        forget(&id);
                        middleware::failure(error)
                    }
                };
            }
            after(state, "slack_event_failed", move |state| async move {
                let kind = event["type"].as_str().unwrap_or("");
                match Bot::serving(&state, &team).await? {
                    Some(bot) => on_event(&bot, kind, &event).await,
                    None => Ok(()),
                }
            });
            StatusCode::OK.into_response()
        }
        _ => StatusCode::OK.into_response(),
    }
}

/// What Slack says of the app in a workspace that counts whether or not Dispatch acts there
/// now: it was removed from the workspace, or its tokens there were revoked.
enum Lifecycle {
    Removed,
    Revoked,
}
impl Lifecycle {
    fn of(event: &Value) -> Option<Self> {
        match event["type"].as_str()? {
            "app_uninstalled" => Some(Self::Removed),
            "tokens_revoked"
                if event["tokens"]["bot"]
                    .as_array()
                    .is_some_and(|bots| !bots.is_empty()) =>
            {
                Some(Self::Revoked)
            }
            _ => None,
        }
    }
    /// Keeps it for the DSP the workspace `team` belongs to: forgets the workspace, or breaks
    /// it until someone reconnects it.
    async fn keep(self, state: &Arc<State>, team: String) -> Result<()> {
        let of = team.clone();
        let Some(dsp) = state.read(move |db| db.slack_team_dsp(&of)).await? else {
            return Ok(());
        };
        state
            .run(move |db| match self {
                Self::Removed => connection::removed(db, &dsp, &team),
                Self::Revoked => connection::broken(db, &dsp, &team),
            })
            .await
    }
}

/// What people do with the app's buttons, forms and shortcuts, at `/slack/interactions`.
/// Slack sends it as a form whose `payload` is its JSON.
pub async fn interactions(state: Arc<State>, request: Request) -> Response {
    let payload = match received(&state, request, INTERACTIONS_PATH).await {
        Ok(body) => url::form_urlencoded::parse(&body)
            .find(|(key, _)| key == "payload")
            .and_then(|(_, payload)| serde_json::from_str::<Value>(&payload).ok()),
        Err(refused) => return refused,
    };
    let Some(payload) = payload else {
        return middleware::failure(Error::new("invalid_input", 400));
    };
    after(state, "slack_interaction_failed", move |state| async move {
        let team = payload["team"]["id"].as_str().unwrap_or("");
        let Some(bot) = Bot::serving(&state, team).await? else {
            return Ok(());
        };
        let kind = payload["type"].as_str().unwrap_or("");
        // A button's, or else a form's or a shortcut's.
        let action = payload["actions"][0]["action_id"]
            .as_str()
            .or_else(|| payload["view"]["callback_id"].as_str())
            .or_else(|| payload["callback_id"].as_str())
            .unwrap_or("");
        on_interaction(&bot, kind, action, &payload).await
    });
    StatusCode::OK.into_response()
}

/// Does `work` once Slack has its answer, logging why it failed, if it did, as `failed`.
fn after<F>(
    state: Arc<State>,
    failed: &'static str,
    work: impl FnOnce(Arc<State>) -> F + Send + 'static,
) where
    F: Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(error) = work(state).await {
            observability::event("warn", failed, json!({"error": error.code}));
        }
    });
}

/// Whether `body` came from Slack: signed with `secret`, as Slack signs
/// `v0:<timestamp>:<body>`, at a time within five minutes of `now`, in seconds, so a request
/// sent again later is refused.
pub fn signed(secret: &str, headers: &HeaderMap, body: &[u8], now: i64) -> bool {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    };
    let timestamp = header("x-slack-request-timestamp");
    let Some(sent) = header("x-slack-signature").strip_prefix("v0=") else {
        return false;
    };
    if timestamp
        .parse::<i64>()
        .map_or(true, |at| (now - at).abs() > 5 * 60)
    {
        return false;
    }
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .expect("HMAC supports any key length");
    mac.update(format!("v0:{timestamp}:").as_bytes());
    mac.update(body);
    crypto::equal(&crypto::hex(&mac.finalize().into_bytes()), sent)
}
/// The body of a request Slack sent to `pattern`, once its signature is checked. It may be up
/// to 64 KiB, as any request's may: a form with many fields, as a `view_submission` brings,
/// needs more.
async fn received(
    state: &State,
    request: Request,
    pattern: &'static str,
) -> std::result::Result<Bytes, Response> {
    let (parts, body) = request.into_parts();
    middleware::address(state, &parts, pattern).map_err(middleware::failure)?;
    let slack = Slack::of(&state.config).map_err(middleware::failure)?;
    let body = middleware::bytes(body).await.map_err(middleware::failure)?;
    if !signed(slack.signing_secret(), &parts.headers, &body, now() / 1000) {
        return Err(middleware::failure(Error::new("slack_unsigned", 401)));
    }
    Ok(body)
}

/// Whether this is the first time Slack sent the event `id`: it sends one again when an answer
/// was slow or lost. Each is remembered for a quarter of an hour.
fn first_time(id: &str) -> bool {
    const KEPT_MS: i64 = 15 * 60 * 1000;
    const MOST: usize = 2000;
    if id.is_empty() {
        return true;
    }
    let at = now();
    let mut seen = seen();
    while seen
        .front()
        .is_some_and(|(_, when)| at - when > KEPT_MS || seen.len() >= MOST)
    {
        seen.pop_front();
    }
    if seen.iter().any(|(sent, _)| sent == id) {
        return false;
    }
    seen.push_back((id.to_owned(), at));
    true
}
/// The events Slack sent lately, by ID, with when each came.
fn seen() -> MutexGuard<'static, VecDeque<(String, i64)>> {
    static SEEN: LazyLock<Mutex<VecDeque<(String, i64)>>> = LazyLock::new(Default::default);
    SEEN.lock().unwrap_or_else(|poison| poison.into_inner())
}
/// Forgets the event `id`, which failed, so Slack sending it again is handled.
fn forget(id: &str) {
    seen().retain(|(sent, _)| sent != id);
}

#[cfg(test)]
#[path = "../tests/backend/inbound.rs"]
mod tests;
