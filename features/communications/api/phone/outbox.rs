//! The texts Dispatch has for a phone to send, waited on, and how each attempt at them went.
use super::{
    access::{Phone, checked_in},
    health,
    pairing::VERSION_MOST,
};
use crate::backend::{
    phone::{
        app::{self, App},
        live,
        model::{EXPIRY_MS, ID_MOST, Status, Text},
        storage::{AppVersion, DeviceStore, MOST_TEXTS, StatusReport, TextStore},
    },
    storage::DOMAIN,
    texts::filing,
};
use dispatch_core::{
    Error, Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, query_number},
        route::Grant,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

/// How long a phone waiting on its outbox waits before it's answered with nothing.
const WAIT: Duration = Duration::from_secs(20);
/// The most characters a phone may say of why a text failed.
pub(super) const ERROR_MOST: usize = 500;
/// The highest `versionCode` Android takes.
const VERSION_CODE_MOST: usize = 2_100_000_000;

/// The texts Dispatch has for the phone, each to send from its number to `number`, with its
/// pictures to fetch first, never started once it `expires`. A text tried again comes again with a
/// later `attempt`, and a later expiry. With them, the app's latest version, while the server hands
/// one out, so the app offers to update within a wait.
#[derive(Serialize)]
struct Outbox {
    texts: Vec<Outgoing>,
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
        Self {
            attempt,
            // Waiting, its status last moved as it was queued, or tried again.
            expires: text.status_at.unwrap_or(text.at) + EXPIRY_MS,
            id: text.id,
            number: text.number,
            body: text.body,
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
/// Waits, up to twenty seconds, for texts Dispatch has for the phone: checking in, as it does, and
/// noting the app it runs and how it says it stands once either changed.
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
    let _slot = state
        .topics
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new("platform_busy", 503))?;
    // Subscribed before reading, so a text queued between the read and the wait still wakes it.
    let mut listener = state.topics.subscribe(live::TOPIC, &caller.dsp);
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        let asked = input.clone();
        let (texts, app) = state
            .read(move |db| {
                let caller = access.authorize(db, &asked)?;
                let texts = db.phones_outbox(&caller.dsp, &caller.device.id)?;
                Ok((texts, app::latest(&db.config)))
            })
            .await?;
        if !texts.is_empty() || tokio::time::Instant::now() >= until {
            return Reply::of(&Outbox {
                texts: texts.into_iter().map(Outgoing::of).collect(),
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
