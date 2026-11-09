//! The texts Dispatch has for a phone to send, waited on, and how each attempt at them went.
use super::{
    access::{Phone, checked_in},
    health,
    pairing::VERSION_MOST,
};
use crate::api::types::GifVersion;
use crate::backend::{
    gifs::{self, Sending},
    phone::{
        app::{self, App},
        live,
        model::{EXPIRY_MS, ID_MOST, Status, Text, WANT_MS},
        storage::{
            AppVersion, Device, DeviceStore, FileStore, MOST_TEXTS, StatusReport, TextStore,
        },
    },
    storage::DOMAIN,
    texts::filing,
};
use dispatch_core::{
    Error, Result, State,
    db::now,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, query_number},
        route::Grant,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

/// How long a phone waiting on its outbox waits before it's answered with nothing.
const WAIT: Duration = Duration::from_secs(20);
// Ordinary page and video waits can depend on phones receiving work. Keep their admission
// separate so those waits cannot consume the phones' own bounded capacity.
static WAITING: Semaphore = Semaphore::const_new(128);
/// The most characters a phone may say of why a text failed.
pub(super) const ERROR_MOST: usize = 500;
/// The highest `versionCode` Android takes.
const VERSION_CODE_MOST: usize = 2_100_000_000;

/// The texts Dispatch has for the phone, each to send from its number to `number`, with its
/// pictures to fetch first, never started once it `expires`. A text tried again comes again with a
/// later `attempt`, and a later expiry. With them, the videos the phone keeps that someone is
/// playing, by their IDs, whose bytes it sends; and the app's latest version, while the server hands
/// one out, so the app offers to update within a wait.
#[derive(Serialize)]
struct Outbox {
    texts: Vec<Outgoing>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    wanted: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<Latest>,
}
/// The app's latest version, and the `versionCode` Android compares.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Latest {
    version: String,
    version_code: u32,
}
impl Latest {
    fn of(app: App) -> Self {
        Self {
            version: app.version,
            version_code: app.version_code,
        }
    }
}
#[derive(Serialize)]
struct Outgoing {
    id: String,
    number: String,
    body: String,
    files: Vec<OutgoingFile>,
    attempt: i64,
    expires: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    gif: Option<OutgoingGif>,
}
/// A GIF of KLIPY's, which the phone fetches from KLIPY at the largest of its sizes it can send.
#[derive(Serialize)]
struct OutgoingGif {
    name: String,
    versions: Vec<GifVersion>,
}
#[derive(Serialize)]
struct OutgoingFile {
    id: String,
    name: String,
    mime: String,
    size: i64,
}
impl Outgoing {
    fn of((text, attempt): (Text, i64)) -> Self {
        // Its GIF, as a file the phone fetches, or as its link after what was written.
        let (body, gif) = match (text.gif, gifs::SENDING) {
            (Some(gif), Sending::Link) => (gifs::linked(&text.body, &gif), None),
            (gif, _) => (
                text.body,
                gif.map(|gif| OutgoingGif {
                    name: format!("{}.gif", gif.slug),
                    versions: gif.versions,
                }),
            ),
        };
        Self {
            attempt,
            // Waiting, its status last moved as it was queued, or tried again.
            expires: text.status_at.unwrap_or(text.at) + EXPIRY_MS,
            id: text.id,
            number: text.number,
            body,
            gif,
            files: text
                .files
                .into_iter()
                .map(|file| OutgoingFile {
                    id: file.id,
                    name: file.name,
                    mime: file.mime,
                    size: file.size,
                })
                .collect(),
        }
    }
}
/// Waits, up to twenty seconds, for texts Dispatch has for the phone, or videos it keeps that someone
/// is playing, each handed out once each time it's asked for: checking in, as it does, and noting
/// the app it runs and how it says it stands once either changed.
pub(super) async fn outbox(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.query, &["version", "versionCode"])?;
    let running = running(&input.query)?;
    let health = health::reported(&input)?;
    let caller = checked_in(&state, &input, access).await?;
    let app = running.filter(|app| !caller.device.runs(app));
    let health = health.filter(|health| caller.device.health.as_ref() != Some(health));
    if app.is_some() || health.is_some() {
        let asked = input.clone();
        state
            .run_scoped(caller.dsp.clone(), DOMAIN, move |db| {
                let caller = access.authorize(db, &asked)?;
                if let Some(app) = &app {
                    db.phones_app_version(&caller.dsp, &caller.device.id, app)?;
                }
                if let Some(health) = &health {
                    db.phones_health(&caller.dsp, &caller.device.id, health)?;
                }
                Ok(())
            })
            .await?;
    }
    let slot = WAITING.try_acquire().ok();
    // Subscribed before reading, so a text queued, or a video asked for, between the read and the
    // wait still wakes it.
    let mut listener = state.topics.subscribe(live::TOPIC, &caller.dsp);
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        let asked = input.clone();
        let (texts, outdated, wanted, app) = state
            .read(move |db| {
                let caller = access.authorize(db, &asked)?;
                let (texts, outdated): (Vec<_>, Vec<_>) = db
                    .phones_outbox(&caller.dsp, &caller.device.id)?
                    .into_iter()
                    .partition(|(text, _)| sendable(text, &caller.device));
                let wanted = db.phones_wanted(&caller.dsp, &caller.device.id, now() - WANT_MS)?;
                Ok((texts, outdated, wanted, app::latest(&db.config)))
            })
            .await?;
        if !outdated.is_empty() {
            fail_outdated(&state, &input, access, &caller.dsp, outdated).await?;
            continue;
        }
        if !wanted.is_empty() {
            let (asked, handed) = (input.clone(), wanted.clone());
            // Bookkeeping: no cached read depends on when a video was handed out.
            state
                .run_bookkeeping(move |db| {
                    let caller = access.authorize(db, &asked)?;
                    db.phones_handed(&caller.dsp, &handed, now())
                })
                .await?;
        }
        // Without admission, answer ready work but never an empty heartbeat or an idle wait.
        if texts.is_empty() && wanted.is_empty() && slot.is_none() {
            return Err(Error::busy("phone_outbox_idle_unavailable"));
        }
        if !texts.is_empty() || !wanted.is_empty() || tokio::time::Instant::now() >= until {
            return Reply::of(&Outbox {
                texts: texts.into_iter().map(Outgoing::of).collect(),
                wanted,
                app: app.map(Latest::of),
            });
        }
        let _ = tokio::time::timeout_at(until, listener.changed()).await;
    }
}

/// The app the phone runs, as its wait says it: `version` and `versionCode` together, or neither,
/// as an app older than that says.
fn running(query: &Value) -> Result<Option<AppVersion>> {
    if query.get("version").is_none() && query.get("versionCode").is_none() {
        return Ok(None);
    }
    Ok(Some(AppVersion {
        version: v::name(query, "version", VERSION_MOST)?,
        version_code: query_number(query, "versionCode", 0, 1, VERSION_CODE_MOST)? as i64,
    }))
}

/// Whether `text` can go from `device`: one with a GIF only from an app that sends one, unless
/// GIFs go as links.
fn sendable(text: &Text, device: &Device) -> bool {
    text.gif.is_none()
        || gifs::SENDING == Sending::Link
        || device
            .app_version_code
            .is_some_and(|code| code >= gifs::RELAY_SENDS_GIFS)
}

/// Fails each of `texts`, with a GIF its phone's app can't send, as the phone would report it.
async fn fail_outdated(
    state: &Arc<State>,
    input: &Input,
    access: Phone,
    dsp: &str,
    texts: Vec<(Text, i64)>,
) -> Result<()> {
    let asked = input.clone();
    filing::changing(state, dsp, move |db| {
        let caller = access.authorize(db, &asked)?;
        let mut moved = Vec::new();
        for (text, attempt) in texts {
            let report = StatusReport {
                id: text.id,
                attempt,
                status: Status::Failed,
                at: now(),
                error: Some(gifs::OUTDATED.to_owned()),
            };
            if db.phones_status(&caller.dsp, &caller.device.id, &report)? {
                moved.push(report.id);
            }
        }
        Ok(((), moved))
    })
    .await
}

/// Notes how the texts Dispatch had for the phone went, and files those that moved.
pub(super) async fn statuses(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.body, &["statuses"])?;
    let statuses = input.body["statuses"]
        .as_array()
        .filter(|statuses| !statuses.is_empty() && statuses.len() <= MOST_TEXTS)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(reported)
        .collect::<Result<Vec<_>>>()?;
    let caller = checked_in(&state, &input, access).await?;
    let asked = input.clone();
    filing::changing(&state, &caller.dsp, move |db| {
        let caller = access.authorize(db, &asked)?;
        let mut moved = Vec::new();
        for report in statuses {
            if db.phones_status(&caller.dsp, &caller.device.id, &report)? {
                moved.push(report.id);
            }
        }
        Ok(((), moved))
    })
    .await?;
    Ok(Reply::ok())
}
/// How one attempt went, as the phone reports it.
fn reported(status: &Value) -> Result<StatusReport> {
    v::fields(status, &["id", "attempt", "status", "at", "error"])?;
    let error = match &status["error"] {
        Value::Null => None,
        _ => Some(v::text(status, "error", 0, ERROR_MOST)?.to_owned()),
    };
    Ok(StatusReport {
        id: v::text(status, "id", 1, ID_MOST)?.to_owned(),
        attempt: v::integer(status, "attempt", 0, i64::MAX)?,
        status: match v::choice(status, "status", &["sent", "delivered", "failed"])? {
            "sent" => Status::Sent,
            "delivered" => Status::Delivered,
            _ => Status::Failed,
        },
        at: v::integer(status, "at", 0, i64::MAX)?,
        error,
    })
}

#[cfg(test)]
#[path = "../../tests/backend/phone/admission.rs"]
mod tests;
