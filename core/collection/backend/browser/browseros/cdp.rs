use crate::{Error, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    os::unix::net::UnixStream,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
pub(super) const MAX_FRAME: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
/// How long one event poll may hold the transport, which every tab shares: a longer
/// wait starves the other tabs' commands. Measured on a day of routes read by three
/// tabs: 250 ms took 33 s, 50 ms 25 s, 20 ms no faster than 50.
const EVENT_WAIT: Duration = Duration::from_millis(50);
fn require(ok: bool, code: &str) -> Result<()> {
    ensure(ok, code, 503)
}

/// Serialized, bounded CDP transport. The descriptor is private to one browser.
/// Retains bounded Fetch requests and the latest main-frame commit per tab;
/// commands never overlap.
pub(super) struct Cdp {
    socket: BufReader<tokio::net::UnixStream>,
    next: u64,
    partial: Vec<u8>,
    events: VecDeque<Value>,
    frames: HashMap<String, Value>,
    loading: super::loading::Loading,
}
impl Cdp {
    pub(super) fn new(socket: UnixStream) -> Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket: BufReader::new(tokio::net::UnixStream::from_std(socket)?),
            next: 0,
            partial: Vec::new(),
            events: VecDeque::new(),
            frames: HashMap::new(),
            loading: super::loading::Loading::default(),
        })
    }
    // read_until appends to persistent storage, so a poll timeout cannot lose a
    // partial event frame. Only the exact Fetch subscription is retained.
    async fn read(&mut self) -> Result<Value> {
        let remaining = MAX_FRAME.saturating_sub(self.partial.len() as u64);
        let count = (&mut self.socket)
            .take(remaining)
            .read_until(0, &mut self.partial)
            .await?;
        require(count > 0, "browser_lost")?;
        require(self.partial.last() == Some(&0), "browser_protocol_failed")?;
        let bytes = std::mem::take(&mut self.partial);
        Ok(serde_json::from_slice(&bytes[..bytes.len() - 1])?)
    }
    fn retain(&mut self, value: Value) -> Result<()> {
        require(
            self.events.len() < 16 && serde_json::to_vec(&value)?.len() <= 256 * 1024,
            "browser_event_overflow",
        )?;
        self.events.push_back(value);
        Ok(())
    }
    fn observe(&mut self, value: &Value) -> Result<()> {
        self.loading.observe(value)?;
        match value["method"].as_str() {
            Some("Fetch.requestPaused") => self.retain(value.clone())?,
            Some("Page.frameNavigated") if value["params"]["frame"]["parentId"].is_null() => {
                if let Some(session) = value["sessionId"].as_str() {
                    require(
                        self.frames.contains_key(session) || self.frames.len() < 8,
                        "browser_event_overflow",
                    )?;
                    require(
                        serde_json::to_vec(&value["params"]["frame"])?.len() <= 64 * 1024,
                        "browser_event_overflow",
                    )?;
                    self.frames
                        .insert(session.into(), value["params"]["frame"].clone());
                }
            }
            Some("Target.detachedFromTarget") => {
                if let Some(session) = value["params"]["sessionId"].as_str() {
                    self.frames.remove(session);
                    self.loading.remove(session);
                }
            }
            _ => (),
        }
        Ok(())
    }
    pub(super) fn loading(&self, session: &str, loader: &str) -> Value {
        self.loading.status(session, loader)
    }
    // Do not send renderer commands while a navigation awaits response headers:
    // even Page.getFrameTree can block there and starve the other tab's commands.
    pub(super) async fn navigation(&mut self, session: &str, previous: &str) -> Result<Value> {
        let result = tokio::time::timeout(Duration::from_millis(20), async {
            loop {
                if let Some(frame) = self.frames.get(session)
                    && frame["loaderId"]
                        .as_str()
                        .is_some_and(|loader| loader != previous)
                {
                    return Ok(frame.clone());
                }
                let value = self.read().await?;
                self.observe(&value)?;
            }
        })
        .await;
        result.unwrap_or(Ok(Value::Null))
    }
    pub(super) async fn event(&mut self, session: &str) -> Result<Value> {
        let result = tokio::time::timeout(EVENT_WAIT, async {
            loop {
                if let Some(index) = self.events.iter().position(|v| v["sessionId"] == session) {
                    return Ok(self.events.remove(index).unwrap()["params"].clone());
                }
                let value = self.read().await?;
                self.observe(&value)?;
            }
        })
        .await;
        result.unwrap_or(Ok(Value::Null))
    }
    pub(super) async fn command(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut message = json!({"id":id,"method":method,"params":params});
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let mut bytes = serde_json::to_vec(&message)?;
        require(
            bytes.len() < MAX_FRAME as usize,
            "browser_command_too_large",
        )?;
        bytes.push(0);
        tokio::time::timeout(TIMEOUT, async {
            self.socket.get_mut().write_all(&bytes).await?;
            loop {
                let value = self.read().await?;
                self.observe(&value)?;
                if value["id"] != id {
                    continue;
                }
                if ["Runtime.evaluate", "Page.createIsolatedWorld"].contains(&method)
                    && value["error"]["code"] == -32000
                    && value["error"]["message"].as_str().is_some_and(|s| {
                        [
                            "Execution context was destroyed",
                            "Cannot find context",
                            "No frame for given id",
                        ]
                        .iter()
                        .any(|message| s.contains(message))
                    })
                {
                    return Ok(json!({"navigationPending":true}));
                }
                require(value.get("error").is_none(), "browser_command_failed")?;
                return Ok(value["result"].clone());
            }
        })
        .await
        .map_err(|_| Error::new("browser_command_timeout", 504))?
    }
}
#[cfg(test)]
#[path = "../../../tests/backend/browser/browseros/cdp.rs"]
mod tests;
