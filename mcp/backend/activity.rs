//! The Agents page's Activity log: each call an agent made with a key or as a connected app,
//! over REST or MCP. When it started, with which key, to which endpoint or tool, about which
//! DSP, how it ended, how long it took and how much it answered; never what it asked beyond
//! the endpoint or tool, and never a token. A request notes its call as it goes and records it
//! here, in memory, once answered. The scheduler writes calls down in batches, so an agent's
//! read never waits for the platform's write lock. Calls are kept 90 days, and at most
//! `DAILY` of each key's a day, so one busy key cannot fill the disk.
use super::{piece::Kept, server};
use crate::api::types::{AgentActivity, AgentActivityKey, AgentActivityPage, AgentDsp};
use dispatch_core::{
    Result, State,
    db::{FromRow, Row, Store, now},
    foundation::observability::{self, RequestContext, RequestTrace},
    server::http::Answered,
};
use rusqlite::params;
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

/// Calls held in memory at most; past it, the oldest go.
pub const HELD: usize = 10_000;
/// Calls written down at once.
pub const BATCH: usize = 500;
/// How often the scheduler writes calls down, unless a batch fills sooner.
pub const EVERY_MS: i64 = 5_000;
/// How long calls are kept.
pub const KEPT_MS: i64 = 90 * 86_400_000;
/// Old calls removed at once, so pruning holds the platform lock only briefly.
const PRUNE_STEP: i64 = 10_000;
/// Calls kept of each key a UTC day. The next is kept as one row that marks the day capped
/// (`CAPPED_SURFACE`, outcome `CAPPED`), and the rest of the day's are only counted.
pub const DAILY: u32 = 10_000;
/// The surface and outcome of the row that marks a key's day capped.
pub const CAPPED_SURFACE: &str = "activity:capped";
pub const CAPPED: &str = "capped";
const DAY_MS: i64 = 86_400_000;

/// A call waiting to be written down.
#[derive(Clone, Debug)]
pub struct Call {
    pub at: i64,
    pub key: AgentActivityKey,
    pub surface: String,
    pub dsp: Option<AgentDsp>,
    pub outcome: String,
    pub ms: u32,
    pub bytes: u32,
}

/// What a request notes of an agent's call as it goes: the key that signed it, as the access
/// check found it; the MCP tool it calls; and the DSP and outcome its handler found.
#[derive(Clone, Debug, Default)]
pub struct Noted {
    pub key: Option<AgentActivityKey>,
    pub surface: Option<String>,
    pub dsp: Option<AgentDsp>,
    pub outcome: Option<String>,
}

/// What the request's trace notes of an agent's call, made the first time it is asked for.
pub fn noted(context: &mut RequestContext) -> &mut Noted {
    context
        .agent
        .get_or_insert_with(|| Box::new(Noted::default()))
        .downcast_mut::<Noted>()
        .expect("only the MCP notes an agent's call")
}

/// The calls not yet written down.
#[derive(Default)]
pub struct Activity(Mutex<Held>);
#[derive(Default)]
struct Held {
    calls: VecDeque<Call>,
    /// Calls dropped since the last take.
    dropped: u64,
    /// The minute each key was last recorded refused for its rate, so a client that keeps
    /// trying is recorded once a minute.
    limited: HashMap<String, i64>,
    /// The UTC day each key last called on, and how many of its calls were recorded that
    /// day, the capped day's marker included.
    days: HashMap<String, (i64, u32)>,
    /// Calls not kept since the last take, their key past its day's cap.
    capped: u64,
}
/// Calls taken to be written down, oldest first; how many were dropped before them, the
/// buffer full; and how many were not kept, their key past its day's cap.
pub struct Taken {
    pub calls: Vec<Call>,
    pub dropped: u64,
    pub capped: u64,
}

impl Activity {
    fn held(&self) -> MutexGuard<'_, Held> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
    /// Counts each key's calls recorded today as the database holds them, so a restart
    /// keeps its cap. One count a key, read from the key's own index.
    pub fn seeded(db: &Store) -> Result<Self> {
        let day = now().div_euclid(DAY_MS);
        let counts: Vec<(String, i64)> = db.platform.query_as(
            "SELECT k.id,(SELECT count(*) FROM agent_activity a WHERE a.key_id=k.id \
             AND a.at>=?) FROM agent_keys k",
            [day * DAY_MS],
        )?;
        let activity = Self::default();
        activity.held().days = counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(key, count)| (key, (day, u32::try_from(count).unwrap_or(u32::MAX))))
            .collect();
        Ok(activity)
    }
    /// Holds a call until the scheduler writes it down: at most `DAILY` of a key's a day,
    /// then one that marks the day capped.
    pub fn record(&self, call: Call) {
        let mut held = self.held();
        if call.outcome == "rate_limited" {
            let minute = call.at / 60_000;
            if held.limited.get(&call.key.id) == Some(&minute) {
                return;
            }
            held.limited.retain(|_, limited| *limited == minute);
            held.limited.insert(call.key.id.clone(), minute);
        }
        let day = call.at.div_euclid(DAY_MS);
        let today = held.days.entry(call.key.id.clone()).or_insert((day, 0));
        // A call that started before midnight and ended after counts toward the new day.
        if day > today.0 {
            *today = (day, 0);
        }
        today.1 = today.1.saturating_add(1);
        let count = today.1;
        let call = match count.cmp(&(DAILY + 1)) {
            std::cmp::Ordering::Less => call,
            std::cmp::Ordering::Equal => {
                held.capped += 1;
                Call {
                    surface: CAPPED_SURFACE.into(),
                    dsp: None,
                    outcome: CAPPED.into(),
                    ms: 0,
                    bytes: 0,
                    ..call
                }
            }
            std::cmp::Ordering::Greater => {
                held.capped += 1;
                return;
            }
        };
        held.calls.push_back(call);
        held.bound();
    }
    /// Records the call a request noted, once its route answered. A request no key signed
    /// is no one's call, nor is an MCP message that calls no tool, such as the handshake.
    pub fn finish(&self, noted: Noted, answered: Answered<'_>) {
        let Some(key) = noted.key else {
            return;
        };
        let Some(surface) = noted.surface.or_else(|| surface(answered.path)) else {
            return;
        };
        let outcome = answered
            .failed
            .map(str::to_owned)
            .or(noted.outcome)
            .unwrap_or_else(|| {
                if surface.starts_with("mcp:") {
                    // The MCP server refused the message before any tool ran.
                    "invalid_request".into()
                } else if answered.status < 400 {
                    "ok".into()
                } else {
                    "error".into()
                }
            });
        self.record(Call {
            at: answered.at,
            key,
            surface,
            dsp: noted.dsp,
            outcome,
            ms: u32::try_from(answered.ms).unwrap_or(u32::MAX),
            bytes: u32::try_from(answered.bytes).unwrap_or(u32::MAX),
        });
    }
    /// How many calls wait to be written down.
    pub fn pending(&self) -> usize {
        self.held().calls.len()
    }
    /// The oldest calls, at most `most`, to be written down.
    pub fn take(&self, most: usize) -> Taken {
        let mut held = self.held();
        let count = most.min(held.calls.len());
        Taken {
            calls: held.calls.drain(..count).collect(),
            dropped: std::mem::take(&mut held.dropped),
            capped: std::mem::take(&mut held.capped),
        }
    }
    /// Holds calls again, ahead of any recorded since, after writing them down failed.
    pub fn restore(&self, calls: Vec<Call>) {
        let mut held = self.held();
        for call in calls.into_iter().rev() {
            held.calls.push_front(call);
        }
        held.bound();
    }
}
impl Held {
    fn bound(&mut self) {
        while self.calls.len() > HELD {
            self.calls.pop_front();
            self.dropped += 1;
        }
    }
}

/// The endpoint a REST route answers, as the log names it: `rest:whoami` for
/// `/api/v1/whoami`. The MCP endpoint names its calls by their tool instead.
fn surface(path: &str) -> Option<String> {
    let endpoint = path.strip_prefix("/api/v1/")?;
    (endpoint != "mcp").then(|| format!("rest:{endpoint}"))
}

/// The tool an MCP message calls, as the log names it: `mcp:<tool>` for a tool the server
/// offers, and `mcp:unknown` for any other name, which is only the agent's own text. Any
/// other message, such as the handshake or a list, is no call.
pub fn tool_call(body: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Message {
        method: String,
        params: Option<Params>,
    }
    #[derive(serde::Deserialize)]
    struct Params {
        name: Option<String>,
    }
    let message: Message = serde_json::from_slice(body).ok()?;
    if message.method != "tools/call" {
        return None;
    }
    let name = message.params.and_then(|p| p.name).unwrap_or_default();
    Some(match server::known(&name) {
        Some(tool) => format!("mcp:{tool}"),
        None => "mcp:unknown".into(),
    })
}

/// Notes what a handler found of an agent's call: the DSP it was about, and how it ended.
pub fn note(trace: &RequestTrace, dsp: Option<AgentDsp>, outcome: &str) {
    let mut context = trace.lock().unwrap_or_else(|poison| poison.into_inner());
    let noted = noted(&mut context);
    noted.dsp = dsp;
    noted.outcome = Some(outcome.to_owned());
}

/// Writes down every call held, a batch at a time, each under the platform lock only as long
/// as its rows take. Answers how many were written. A batch that could not be written is
/// held again for the next time.
pub async fn flush(state: &Arc<State>) -> Result<usize> {
    let mut written = 0;
    for _ in 0..HELD.div_ceil(BATCH) {
        let taken = Kept::of(state).activity.take(BATCH);
        if taken.dropped > 0 {
            observability::event(
                "warn",
                "agent.activity_dropped",
                json!({"dropped": taken.dropped}),
            );
        }
        if taken.capped > 0 {
            observability::event(
                "warn",
                "agent.activity_capped",
                json!({"capped": taken.capped}),
            );
        }
        let count = taken.calls.len();
        if count == 0 {
            break;
        }
        let calls = Arc::new(taken.calls);
        let writing = calls.clone();
        let result = state
            .run_bookkeeping(move |db| db.record_agent_activity(&writing))
            .await;
        if let Err(error) = result {
            Kept::of(state)
                .activity
                .restore(Arc::unwrap_or_clone(calls));
            return Err(error);
        }
        written += count;
        if count < BATCH {
            break;
        }
    }
    Ok(written)
}

/// Which calls a page lists, by how they ended. A capped day's marker is listed whichever
/// is asked for, since the calls it stands for ended every way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcomes {
    #[default]
    All,
    Ok,
    Refused,
}
/// What `GET /api/platform/agents/activity` asks for: one key's calls or all of them, those
/// that ended one way, and the page after `before`.
#[derive(Clone, Debug, Default)]
pub struct ActivityQuery {
    pub key: Option<String>,
    pub outcomes: Outcomes,
    pub before: Option<(i64, i64)>,
    pub limit: usize,
}
/// Where a page ends, as its `next` names it: the last call's start and row.
pub fn cursor(text: &str) -> Option<(i64, i64)> {
    let (at, id) = text.split_once('.')?;
    Some((at.parse().ok()?, id.parse().ok()?))
}

struct Listed {
    id: i64,
    at: i64,
    call: AgentActivity,
}
impl FromRow for Listed {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let dsp: Option<String> = row.get("dsp_id")?;
        let at: i64 = row.get("at")?;
        Ok(Self {
            id: row.get("id")?,
            at,
            call: AgentActivity {
                at: dispatch_core::db::at(at),
                key: AgentActivityKey {
                    id: row.get("key_id")?,
                    name: row.get("key_name")?,
                    kind: row.get("key_kind")?,
                },
                surface: row.get("surface")?,
                dsp: match dsp {
                    Some(id) => Some(AgentDsp {
                        id,
                        name: row.get::<Option<String>>("dsp_name")?.unwrap_or_default(),
                    }),
                    None => None,
                },
                outcome: row.get("outcome")?,
                ms: row.get("ms")?,
                bytes: row.get("bytes")?,
            },
        })
    }
}

pub trait ActivityStore {
    /// Writes calls down, all in one transaction.
    fn record_agent_activity(&self, calls: &[Call]) -> Result<()>;

    /// Removes calls past 90 days, a step at a time. Answers how many went.
    fn prune_agent_activity(&self) -> Result<usize>;

    /// A page of the log, newest first, naming keys and DSPs as they are called now.
    fn agent_activity(&self, query: &ActivityQuery) -> Result<AgentActivityPage>;
}
impl ActivityStore for Store {
    fn record_agent_activity(&self, calls: &[Call]) -> Result<()> {
        self.platform.transaction(|| {
            for call in calls {
                let dsp = call.dsp.as_ref();
                self.platform.exec(
                    "INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,dsp_id,\
                     dsp_name,outcome,ms,bytes) VALUES (?,?,?,?,?,?,?,?,?,?)",
                    params![
                        call.at,
                        call.key.id,
                        call.key.name,
                        call.key.kind,
                        call.surface,
                        dsp.map(|d| &d.id),
                        dsp.map(|d| &d.name),
                        call.outcome,
                        call.ms,
                        call.bytes
                    ],
                )?;
            }
            Ok(())
        })
    }

    fn prune_agent_activity(&self) -> Result<usize> {
        self.platform.exec(
            "DELETE FROM agent_activity WHERE id IN \
             (SELECT id FROM agent_activity WHERE at<? LIMIT ?)",
            params![now() - KEPT_MS, PRUNE_STEP],
        )
    }

    fn agent_activity(&self, query: &ActivityQuery) -> Result<AgentActivityPage> {
        let (sql, values) = page(query);
        let mut listed: Vec<Listed> = self
            .platform
            .query_as(&sql, rusqlite::params_from_iter(values))?;
        let next = if listed.len() > query.limit {
            listed.truncate(query.limit);
            listed.last().map(|last| format!("{}.{}", last.at, last.id))
        } else {
            None
        };
        // Each DSP by its name now, or as it was named when called once it is gone.
        let reached: Vec<&str> = listed
            .iter()
            .filter_map(|listed| listed.call.dsp.as_ref().map(|dsp| dsp.id.as_str()))
            .collect();
        let names = self.dsp_names(&reached)?;
        for dsp in listed
            .iter_mut()
            .filter_map(|listed| listed.call.dsp.as_mut())
        {
            if let Some(name) = names.get(&dsp.id) {
                dsp.name.clone_from(name);
            }
        }
        Ok(AgentActivityPage {
            rows: listed.into_iter().map(|listed| listed.call).collect(),
            next,
        })
    }
}

/// The query for a page of the log, one more call than it shows, and its values. Each filter
/// adds its own condition, so every page reads one index in order.
fn page(query: &ActivityQuery) -> (String, Vec<rusqlite::types::Value>) {
    let mut sql = "SELECT a.id,a.at,a.key_id,COALESCE(k.name,a.key_name) key_name,\
        a.key_kind,a.surface,a.dsp_id,a.dsp_name,a.outcome,a.ms,a.bytes \
        FROM agent_activity a LEFT JOIN agent_keys k ON k.id=a.key_id WHERE 1"
        .to_owned();
    let mut values: Vec<rusqlite::types::Value> = vec![];
    if let Some(key) = &query.key {
        sql.push_str(" AND a.key_id=?");
        values.push(key.clone().into());
    }
    match query.outcomes {
        Outcomes::All => {}
        Outcomes::Ok => sql.push_str(" AND a.outcome IN ('ok','capped')"),
        Outcomes::Refused => sql.push_str(" AND a.outcome<>'ok'"),
    }
    if let Some((before, id)) = query.before {
        sql.push_str(" AND (a.at,a.id)<(?,?)");
        values.push(before.into());
        values.push(id.into());
    }
    sql.push_str(" ORDER BY a.at DESC,a.id DESC LIMIT ?");
    values.push((query.limit as i64 + 1).into());
    (sql, values)
}

#[cfg(test)]
#[path = "../tests/backend/activity.rs"]
mod tests;
