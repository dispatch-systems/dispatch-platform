//! Bounded commands and newline-delimited frames shared by host and worker.
use super::cdp;
use crate::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

pub(super) const COMMAND_BYTES: u64 = 64 * 1024;
pub(super) const RESPONSE_BYTES: u64 = cdp::MAX_FRAME + 1024;

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum WireCommand {
    Loading {
        session: String,
        loader: String,
    },
    Navigation {
        session: String,
        previous: String,
    },
    Event {
        session: String,
    },
    NativeMove {
        x: i32,
        y: i32,
    },
    NativeClick {
        x: i32,
        y: i32,
    },
    NativeType {
        text: String,
    },
    Cdp {
        method: String,
        params: Value,
        session: Option<String>,
    },
    Close,
}
pub(super) fn frame(value: &impl Serialize, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure(bytes.len() < limit as usize, "browser_frame_too_large", 413)?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(super) async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    limit: u64,
) -> Result<Option<Value>> {
    let mut bytes = Vec::new();
    let count = reader.take(limit).read_until(b'\n', &mut bytes).await?;
    if count == 0 {
        return Ok(None);
    }
    ensure(bytes.last() == Some(&b'\n'), "browser_protocol_failed", 503)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
