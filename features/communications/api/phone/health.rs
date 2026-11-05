//! How a phone says it stands, with each wait on its outbox: JSON in a header of its own, which a
//! server older than that ignores, as it would refuse a query it doesn't know.
use super::outbox::ERROR_MOST;
use crate::backend::phone::model::{Backlog, Happened, Health, Sim};
use dispatch_core::{Error, Result, ensure, foundation::validate as v, server::http::input::Input};
use serde_json::Value;

/// The header a phone says how it stands in.
const HEADER: &str = "x-dispatch-health";
/// The most bytes it may be.
const HEALTH_MOST: usize = 4 * 1024;
/// The most Set up steps it may name, and the most characters of each's key.
const STEPS_MOST: usize = 20;
const STEP_MOST: usize = 40;
/// The most of anything it may count.
const COUNT_MOST: i64 = 1_000_000;
/// The most characters of an unexpected error's class, and of what the app was doing.
const CLASS_MOST: usize = 200;
const DOING_MOST: usize = 100;

/// How the phone says it stands, if it says: none from an app older than that. Refused as
/// `invalid_input` past its limits.
pub(super) fn reported(input: &Input) -> Result<Option<Health>> {
    let Some(header) = input.headers.get(HEADER) else {
        return Ok(None);
    };
    ensure(header.len() <= HEALTH_MOST, "invalid_input", 400)?;
    let health: Value = header
        .to_str()
        .ok()
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(invalid)?;
    v::fields(
        &health,
        &[
            "setup",
            "read",
            "send",
            "sim",
            "waiting",
            "undownloaded",
            "sendError",
            "unexpected",
        ],
    )?;
    let waiting = &health["waiting"];
    v::fields(waiting, &["texts", "files", "statuses"])?;
    Ok(Some(Health {
        setup: health["setup"]
            .as_array()
            .filter(|steps| steps.len() <= STEPS_MOST)
            .ok_or_else(invalid)?
            .iter()
            .map(step)
            .collect::<Result<_>>()?,
        read: v::boolean(&health, "read")?,
        send: v::boolean(&health, "send")?,
        sim: Sim::parse(v::text(&health, "sim", 1, HEALTH_MOST)?).ok_or_else(invalid)?,
        waiting: Backlog {
            texts: v::integer(waiting, "texts", 0, COUNT_MOST)?,
            files: v::integer(waiting, "files", 0, COUNT_MOST)?,
            statuses: v::integer(waiting, "statuses", 0, COUNT_MOST)?,
        },
        undownloaded: v::integer(&health, "undownloaded", 0, COUNT_MOST)?,
        send_error: send_error(&health["sendError"])?,
        unexpected: unexpected(&health["unexpected"])?,
    }))
}

fn invalid() -> Error {
    Error::new("invalid_input", 400)
}
/// A Set up step's key: lowercase letters and `_`.
fn step(step: &Value) -> Result<String> {
    let key = step.as_str().ok_or_else(invalid)?;
    ensure(
        (1..=STEP_MOST).contains(&key.len())
            && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
        "invalid_input",
        400,
    )?;
    Ok(key.to_owned())
}
/// Why a text last failed to send, if one did: as the phone told Dispatch, and when.
fn send_error(failed: &Value) -> Result<Option<Happened>> {
    if failed.is_null() {
        return Ok(None);
    }
    v::fields(failed, &["error", "at"])?;
    Ok(Some(Happened {
        error: v::text(failed, "error", 1, ERROR_MOST)?.to_owned(),
        doing: None,
        at: v::integer(failed, "at", 0, i64::MAX)?,
    }))
}
/// The last error the app didn't expect, if any: its class, and what the app was doing (`where`),
/// in its own words, so neither carries a text or a number.
fn unexpected(error: &Value) -> Result<Option<Happened>> {
    if error.is_null() {
        return Ok(None);
    }
    v::fields(error, &["error", "where", "at"])?;
    let class = v::text(error, "error", 1, CLASS_MOST)?;
    let doing = v::text(error, "where", 1, DOING_MOST)?;
    ensure(
        class
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'$'))
            && doing.bytes().all(|b| b.is_ascii_alphabetic() || b == b' '),
        "invalid_input",
        400,
    )?;
    Ok(Some(Happened {
        error: class.to_owned(),
        doing: Some(doing.to_owned()),
        at: v::integer(error, "at", 0, i64::MAX)?,
    }))
}
