//! Fixture mode's push services, so previews and tests never reach one: each push is kept for the
//! server's run, by the endpoint it went to, as its browser reads it, and a test reads them with
//! `pushed`. An endpoint whose last part begins `gone` answers as one its browser let go of (410),
//! `missing` as one never known (404), `busy` refuses the first push to it (503), and `slow` takes
//! a second to answer.
use super::sending::Answer;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{LazyLock, Mutex},
};

static PUSHED: LazyLock<Mutex<Vec<(String, Value)>>> = LazyLock::new(Default::default);
/// The `busy` endpoints that refused their first push.
static REFUSED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// How many pushes its push services answer at once now, and the most they have.
#[cfg(test)]
static ANSWERING: Mutex<(usize, usize)> = Mutex::new((0, 0));

/// What its push service answers `message` sent to `endpoint`.
pub async fn deliver(endpoint: &str, message: &[u8]) -> Answer {
    let last = endpoint.rsplit('/').next().unwrap_or_default();
    if last.starts_with("slow") {
        #[cfg(test)]
        answering(|(now, most)| {
            *now += 1;
            *most = (*most).max(*now);
        });
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        #[cfg(test)]
        answering(|(now, _)| *now -= 1);
    }
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

#[cfg(test)]
fn answering(change: impl FnOnce(&mut (usize, usize))) {
    change(
        &mut ANSWERING
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()),
    );
}
/// The most pushes its push services answered at once, slow ones, this run.
#[cfg(test)]
pub fn most_answering() -> usize {
    ANSWERING
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .1
}

/// Whether the `busy` endpoint `endpoint` has refused its first push this run.
#[cfg(test)]
pub fn refused(endpoint: &str) -> bool {
    REFUSED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .contains(endpoint)
}
