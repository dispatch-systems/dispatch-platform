//! The texts Dispatch has for a phone to send, waited on, and how each attempt at them went.
use super::access::{Phone, checked_in};
use crate::backend::{
    phone::{
        live,
        model::{ID_MOST, Status, Text},
        storage::{MOST_TEXTS, StatusReport, TextStore},
    },
    storage::DOMAIN,
    texts::filing,
};
use dispatch_core::{
    Error, Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Grant,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

/// How long a phone waiting on its outbox waits before it's answered with nothing.
const WAIT: Duration = Duration::from_secs(20);
/// The most characters a phone may say of why a text failed.
const ERROR_MOST: usize = 500;

/// The texts Dispatch has for the phone, each to send from its number to `number`, with its
/// pictures to fetch first. A text tried again comes again with a later `attempt`.
#[derive(Serialize)]
struct Outbox {
    texts: Vec<Outgoing>,
}
#[derive(Serialize)]
struct Outgoing {
    id: String,
    number: String,
    body: String,
    files: Vec<OutgoingFile>,
    attempt: i64,
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
/// Waits, up to twenty seconds, for texts Dispatch has for the phone: checking in, as it does.
pub(super) async fn outbox(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let caller = checked_in(&state, &input, access).await?;
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
        let texts = state
            .read(move |db| {
                let caller = access.authorize(db, &asked)?;
                db.phones_outbox(&caller.dsp, &caller.device.id)
            })
            .await?;
        if !texts.is_empty() || tokio::time::Instant::now() >= until {
            return Reply::of(&Outbox {
                texts: texts.into_iter().map(Outgoing::of).collect(),
            });
        }
        let _ = tokio::time::timeout_at(until, listener.changed()).await;
    }
}

/// Notes how the texts Dispatch had for the phone went.
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
    let (asked, dsp) = (input.clone(), caller.dsp.clone());
    let moved = state
        .run_scoped(caller.dsp, DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            let mut moved = false;
            for report in &statuses {
                moved |= db.phones_status(&caller.dsp, &caller.device.id, report)?;
            }
            Ok(moved)
        })
        .await?;
    if moved {
        filing::told(&state, &dsp).await;
    }
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
