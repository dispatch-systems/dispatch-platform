//! The Agents page's Activity log: each call an agent made with a key or as a connected app,
//! over REST or MCP. When it started, with which key, to which endpoint or tool, about which
//! DSP, how it ended, whether it read a switched-off feature by bypassing features, how long
//! it took and how much it answered; never what it asked beyond
//! the endpoint or tool, and never a token. A request notes its call as it goes and records it
//! here, in memory, once answered. The scheduler writes calls down in batches, so an agent's
//! read never waits for the platform's write lock. Calls are kept 90 days, and at most
//! `DAILY` of each key's a day, so one busy key cannot fill the disk.
use super::data::catalog;
use crate::{
    Result, State,
    contracts::{AgentActivity, AgentActivityKey, AgentActivityPage, AgentDsp},
    db::{FromRow, Row, Store, now},
    observability::{self, RequestTrace},
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
    pub bypassed: bool,
    pub ms: u32,
    pub bytes: u32,
}

/// What a request notes of an agent's call as it goes: the key that signed it, as the access
/// check found it; the MCP tool it calls; and the DSP, outcome and bypassing its handler found.
#[derive(Clone, Debug, Default)]
pub struct Noted {
    pub key: Option<AgentActivityKey>,
    pub surface: Option<String>,
    pub dsp: Option<AgentDsp>,
    pub outcome: Option<String>,
    pub bypassed: bool,
}

/// How a request an agent sent ended, as its route saw it.
pub struct Answered<'a> {
    /// The route's registered path.
    pub path: &'static str,
    pub at: i64,
    pub ms: u128,
    pub bytes: u64,
    pub status: u16,
    /// The code the route failed with, when it failed rather than answered.
    pub failed: Option<&'a str>,
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
                    bypassed: false,
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
            bypassed: noted.bypassed,
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

/// The endpoint a REST route answers, as the log names it.
fn surface(path: &str) -> Option<String> {
    let id = match path {
        "/api/v1/openapi.json" => "openapi",
        "/api/v1/skill" => "skill",
        _ => catalog::ENDPOINTS.iter().find(|e| e.path == path)?.id,
    };
    Some(format!("rest:{id}"))
}

/// The tool an MCP message calls, as the log names it: `mcp:<tool>` for a tool of the
/// catalog, and `mcp:unknown` for any other name, which is only the agent's own text. Any
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
    Some(match catalog::tool(&name) {
        Some(endpoint) => format!("mcp:{}", endpoint.tool),
        None => "mcp:unknown".into(),
    })
}

/// Notes what a handler found of an agent's call: the DSP it was about, how it ended, and
/// whether it read a switched-off feature by bypassing features.
pub fn note(trace: &RequestTrace, dsp: Option<AgentDsp>, outcome: &str, bypassed: bool) {
    let mut context = trace.lock().unwrap_or_else(|poison| poison.into_inner());
    context.agent.dsp = dsp;
    context.agent.outcome = Some(outcome.to_owned());
    context.agent.bypassed = bypassed;
}

/// Writes down every call held, a batch at a time, each under the platform lock only as long
/// as its rows take. Answers how many were written. A batch that could not be written is
/// held again for the next time.
pub async fn flush(state: &Arc<State>) -> Result<usize> {
    let mut written = 0;
    for _ in 0..HELD.div_ceil(BATCH) {
        let taken = state.activity.take(BATCH);
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
            state.activity.restore(Arc::unwrap_or_clone(calls));
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
                at: crate::db::at(at),
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
                bypassed: row.get::<i64>("bypassed")? == 1,
            },
        })
    }
}

impl Store {
    /// Writes calls down, all in one transaction.
    pub fn record_agent_activity(&self, calls: &[Call]) -> Result<()> {
        self.platform.transaction(|| {
            for call in calls {
                let dsp = call.dsp.as_ref();
                self.platform.exec(
                    "INSERT INTO agent_activity(at,key_id,key_name,key_kind,surface,dsp_id,\
                     dsp_name,outcome,bypassed,ms,bytes) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                    params![
                        call.at,
                        call.key.id,
                        call.key.name,
                        call.key.kind,
                        call.surface,
                        dsp.map(|d| &d.id),
                        dsp.map(|d| &d.name),
                        call.outcome,
                        i64::from(call.bypassed),
                        call.ms,
                        call.bytes
                    ],
                )?;
            }
            Ok(())
        })
    }

    /// Removes calls past 90 days, a step at a time. Answers how many went.
    pub fn prune_agent_activity(&self) -> Result<usize> {
        self.platform.exec(
            "DELETE FROM agent_activity WHERE id IN \
             (SELECT id FROM agent_activity WHERE at<? LIMIT ?)",
            params![now() - KEPT_MS, PRUNE_STEP],
        )
    }

    /// A page of the log, newest first, naming keys and DSPs as they are called now.
    pub fn agent_activity(&self, query: &ActivityQuery) -> Result<AgentActivityPage> {
        // Each filter adds its own condition, so every page reads one index in order.
        let mut sql = "SELECT a.id,a.at,a.key_id,COALESCE(k.name,a.key_name) key_name,\
            a.key_kind,a.surface,a.dsp_id,COALESCE(d.name,a.dsp_name) dsp_name,a.outcome,\
            a.bypassed,a.ms,a.bytes FROM agent_activity a LEFT JOIN agent_keys k ON k.id=a.key_id \
            LEFT JOIN dsps d ON d.id=a.dsp_id WHERE 1"
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
        let mut listed: Vec<Listed> = self
            .platform
            .query_as(&sql, rusqlite::params_from_iter(values))?;
        let next = if listed.len() > query.limit {
            listed.truncate(query.limit);
            listed.last().map(|last| format!("{}.{}", last.at, last.id))
        } else {
            None
        };
        Ok(AgentActivityPage {
            rows: listed.into_iter().map(|listed| listed.call).collect(),
            next,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::AgentKeyKind;

    fn call(at: i64, key: &str, outcome: &str) -> Call {
        Call {
            at,
            key: AgentActivityKey {
                id: key.into(),
                name: "Laptop".into(),
                kind: AgentKeyKind::Key,
            },
            surface: "rest:whoami".into(),
            dsp: None,
            outcome: outcome.into(),
            bypassed: false,
            ms: 3,
            bytes: 120,
        }
    }

    #[test]
    fn a_full_buffer_drops_the_oldest_and_counts_them() {
        let activity = Activity::default();
        // Two keys, so neither passes its day's cap.
        let key = |at: i64| if at % 2 == 0 { "key_a" } else { "key_b" };
        for at in 0..HELD as i64 + 5 {
            activity.record(call(at, key(at), "ok"));
        }
        assert_eq!(activity.pending(), HELD);
        let taken = activity.take(BATCH);
        assert_eq!(taken.dropped, 5);
        assert_eq!(taken.calls.len(), BATCH);
        assert_eq!(taken.calls[0].at, 5);
        // Writing them down failed: they wait again ahead of the rest, and nothing more goes.
        activity.restore(taken.calls);
        assert_eq!(activity.pending(), HELD);
        let again = activity.take(1);
        assert_eq!((again.calls[0].at, again.dropped), (5, 0));
        // Held again when full, the oldest of them go first.
        activity.record(call(HELD as i64 + 5, key(HELD as i64 + 5), "ok"));
        activity.restore(again.calls);
        let last = activity.take(HELD);
        assert_eq!((last.calls[0].at, last.dropped), (6, 1));
        assert_eq!(activity.pending(), 0);
    }

    #[test]
    fn a_key_refused_for_its_rate_is_recorded_once_a_minute() {
        let activity = Activity::default();
        let minute = 1_790_000_040_000 / 60_000 * 60_000;
        for at in [minute, minute + 10, minute + 59_999] {
            activity.record(call(at, "key_a", "rate_limited"));
            activity.record(call(at, "key_b", "rate_limited"));
            activity.record(call(at, "key_a", "ok"));
        }
        activity.record(call(minute + 60_000, "key_a", "rate_limited"));
        let outcomes: Vec<(String, String)> = activity
            .take(HELD)
            .calls
            .into_iter()
            .map(|c| (c.key.id, c.outcome))
            .collect();
        let limited = |key: &str| {
            outcomes
                .iter()
                .filter(|(k, o)| k == key && o == "rate_limited")
                .count()
        };
        assert_eq!((limited("key_a"), limited("key_b")), (2, 1));
        assert_eq!(outcomes.iter().filter(|(_, o)| o == "ok").count(), 3);
    }

    #[test]
    fn a_key_keeps_its_days_first_calls_then_one_row_that_marks_the_day_capped() {
        let activity = Activity::default();
        let day = 1_790_000_000_000 / DAY_MS * DAY_MS;
        for n in 0..i64::from(DAILY) {
            activity.record(call(day + n, "key_a", "ok"));
        }
        let first = activity.take(HELD);
        assert_eq!((first.calls.len(), first.capped), (DAILY as usize, 0));
        // A refusal repeated within its minute is not one more call.
        for n in 0..5 {
            activity.record(call(day + 50_000 + n, "key_a", "rate_limited"));
            activity.record(call(day + 50_000 + n, "key_a", "ok"));
        }
        activity.record(call(day + 60_000, "key_b", "ok"));
        activity.record(call(day + DAY_MS, "key_a", "ok"));
        let taken = activity.take(HELD);
        assert_eq!(taken.capped, 6);
        let seen: Vec<(&str, &str, &str, i64)> = taken
            .calls
            .iter()
            .map(|c| {
                (
                    c.key.id.as_str(),
                    c.surface.as_str(),
                    c.outcome.as_str(),
                    c.at,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("key_a", CAPPED_SURFACE, CAPPED, day + 50_000),
                ("key_b", "rest:whoami", "ok", day + 60_000),
                ("key_a", "rest:whoami", "ok", day + DAY_MS),
            ]
        );
        assert_eq!((taken.calls[0].ms, taken.calls[0].bytes), (0, 0));
        // A call that started the day before is still today's, and does not reopen it.
        activity.record(call(day + DAY_MS - 1, "key_a", "ok"));
        assert_eq!(activity.take(HELD).calls[0].at, day + DAY_MS - 1);
    }

    #[test]
    fn calls_are_named_by_endpoint_or_tool_and_never_by_what_they_ask() {
        assert_eq!(surface("/api/v1/whoami").unwrap(), "rest:whoami");
        assert_eq!(surface("/api/v1/drivers/{driver}").unwrap(), "rest:driver");
        assert_eq!(surface("/api/v1/skill").unwrap(), "rest:skill");
        assert_eq!(surface("/api/v1/mcp"), None);
        let message = |method: &str, params: serde_json::Value| {
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string()
        };
        let called = message(
            "tools/call",
            json!({"name":"driver_report","arguments":{"driver":"Avery Morgan"}}),
        );
        assert_eq!(tool_call(called.as_bytes()).unwrap(), "mcp:driver_report");
        let invented = message("tools/call", json!({"name":"x".repeat(25_000)}));
        assert_eq!(tool_call(invented.as_bytes()).unwrap(), "mcp:unknown");
        assert_eq!(tool_call(message("tools/list", json!({})).as_bytes()), None);
        assert_eq!(tool_call(message("initialize", json!({})).as_bytes()), None);
        assert_eq!(tool_call(b"not json"), None);
        assert_eq!(cursor("1790000000000.42"), Some((1_790_000_000_000, 42)));
        assert_eq!(cursor("42"), None);
        assert_eq!(cursor("a.b"), None);
    }

    #[test]
    fn what_a_request_noted_becomes_its_call() {
        let activity = Activity::default();
        let key = AgentActivityKey {
            id: "agentkey_a".into(),
            name: "Laptop".into(),
            kind: AgentKeyKind::App,
        };
        let answered = |path, status, failed| Answered {
            path,
            at: 1,
            ms: 12,
            bytes: 300,
            status,
            failed,
        };
        let noted = |surface: Option<&str>, outcome: Option<&str>| Noted {
            key: Some(key.clone()),
            surface: surface.map(str::to_owned),
            dsp: None,
            outcome: outcome.map(str::to_owned),
            bypassed: false,
        };
        // No key signed it, or an MCP message that calls no tool: nothing to record.
        activity.finish(Noted::default(), answered("/api/v1/whoami", 401, Some("x")));
        activity.finish(noted(None, None), answered("/api/v1/mcp", 200, None));
        assert_eq!(activity.pending(), 0);
        activity.finish(noted(None, None), answered("/api/v1/skill", 200, None));
        activity.finish(
            noted(None, Some("dsp_required")),
            answered("/api/v1/team", 400, None),
        );
        activity.finish(
            noted(Some("mcp:whoami"), None),
            answered("/api/v1/mcp", 429, Some("rate_limited")),
        );
        activity.finish(
            noted(Some("mcp:whoami"), None),
            answered("/api/v1/mcp", 200, None),
        );
        // An answer read by bypassing features is marked so.
        activity.finish(
            Noted {
                bypassed: true,
                ..noted(None, Some("ok"))
            },
            answered("/api/v1/dvic", 200, None),
        );
        let calls = activity.take(HELD).calls;
        let seen: Vec<(&str, &str, bool)> = calls
            .iter()
            .map(|c| (c.surface.as_str(), c.outcome.as_str(), c.bypassed))
            .collect();
        assert_eq!(
            seen,
            [
                ("rest:skill", "ok", false),
                ("rest:team", "dsp_required", false),
                ("mcp:whoami", "rate_limited", false),
                ("mcp:whoami", "invalid_request", false),
                ("rest:dvic", "ok", true),
            ]
        );
        assert_eq!((calls[0].ms, calls[0].bytes), (12, 300));
        assert_eq!(calls[0].key.kind, AgentKeyKind::App);
    }
}
