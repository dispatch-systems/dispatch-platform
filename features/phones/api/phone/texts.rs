//! The texts a phone reports: those it received, and those sent from its own messaging app, each
//! kept once by its own ID of it.
use super::access::{Phone, checked_in};
use crate::backend::{
    LIMITS, live,
    storage::{DOMAIN, Kept, MOST_TEXTS, Reported, ReportedFile, TextStore},
};
use dispatch_core::{
    Error, Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Grant,
    },
};
use dispatch_phones_contract::{Direction, NUMBER_MOST, normal};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

/// The most characters the phone's own ID of a text may be.
const CLIENT_ID_MOST: usize = 128;
/// The most characters a reported file's name may be, and its kind.
const NAME_MOST: usize = 200;
const MIME_MOST: usize = 100;

/// What reporting answers with: for each text, by the phone's ID of it, Dispatch's, and its
/// files' in the order reported, whose bytes the phone sends next.
#[derive(Serialize)]
struct Reports {
    texts: Vec<Report>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    client_id: String,
    id: String,
    files: Vec<String>,
}
/// Keeps the texts the phone reports: those it received, and those sent from its own messaging
/// app, each once.
pub(in crate::api) async fn texts(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.body, &["texts"])?;
    let reported = input.body["texts"]
        .as_array()
        .filter(|texts| !texts.is_empty() && texts.len() <= MOST_TEXTS)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(reported)
        .collect::<Result<Vec<_>>>()?;
    let caller = checked_in(&state, &input, access).await?;
    let (asked, dsp) = (input.clone(), caller.dsp.clone());
    let kept = state
        .run_scoped(caller.dsp, DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            db.phones_report(&caller.dsp, &caller.device.id, reported)
        })
        .await?;
    live::changed(&state, &dsp).await;
    Reply::of(&Reports {
        texts: kept
            .into_iter()
            .map(
                |Kept {
                     client_id,
                     id,
                     files,
                 }| Report {
                    client_id,
                    id,
                    files,
                },
            )
            .collect(),
    })
}
/// One text as the phone reports it.
fn reported(text: &Value) -> Result<Reported> {
    v::fields(
        text,
        &["clientId", "direction", "number", "body", "at", "files"],
    )?;
    let files = match &text["files"] {
        Value::Null => Vec::new(),
        Value::Array(files) if files.len() <= LIMITS.files => files
            .iter()
            .map(|file| {
                v::fields(file, &["name", "mime", "size"])?;
                Ok(ReportedFile {
                    name: v::name(file, "name", NAME_MOST)?,
                    mime: v::text(file, "mime", 1, MIME_MOST)?.to_owned(),
                    size: v::integer(file, "size", 0, LIMITS.largest as i64)?,
                })
            })
            .collect::<Result<_>>()?,
        _ => return Err(Error::new("invalid_input", 400)),
    };
    Ok(Reported {
        client_id: v::text(text, "clientId", 1, CLIENT_ID_MOST)?.to_owned(),
        direction: match v::choice(text, "direction", &["in", "out"])? {
            "in" => Direction::In,
            _ => Direction::Out,
        },
        number: normal(v::text(text, "number", 1, NUMBER_MOST)?)
            .ok_or_else(|| Error::new("invalid_input", 400))?,
        body: v::text(text, "body", 0, LIMITS.body)?.to_owned(),
        at: v::integer(text, "at", 0, i64::MAX)?,
        files,
    })
}
