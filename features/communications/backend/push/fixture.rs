//! Fixture mode's push services, so previews and tests never reach one: each push is kept for the
//! server's run, by the endpoint it went to, as its browser reads it, and a test reads them with
//! `pushed`. An endpoint whose last part begins `gone` answers as one its browser let go of (410),
//! `missing` as one never known (404), and `busy` refuses the first push to it (503).
use super::sending::Answer;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
};

static PUSHED: LazyLock<Mutex<Vec<(String, Value)>>> = LazyLock::new(Default::default);
/// The `busy` endpoints that refused their first push.
static REFUSED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// What its push service answers `message` sent to `endpoint`.
pub fn deliver(endpoint: &str, message: &[u8]) -> Answer {
    let last = endpoint.rsplit('/').next().unwrap_or_default();
    if last.starts_with("gone") || last.starts_with("missing") {
        return Answer::Gone;
    }
    if last.starts_with("busy")
        && REFUSED
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(endpoint.to_owned())
    {
        return Answer::Failed(Some(503));
    }
    let Ok(read) = serde_json::from_slice(message) else {
        return Answer::Failed(Some(400));
    };
    PUSHED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push((endpoint.to_owned(), read));
    Answer::Sent
}

/// What was pushed to `endpoint` this run, in the order it went.
#[cfg(test)]
pub fn pushed(endpoint: &str) -> Vec<Value> {
    PUSHED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .filter(|(to, _)| to == endpoint)
        .map(|(_, push)| push.clone())
        .collect()
}
