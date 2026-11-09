//! Exact, timezone-aware run starts. No body, file, directory label or per-message run ID is
//! persisted here. Unknown writers invalidate readiness through ordinary SQLite triggers.
use super::{
    model::{Direction, Reference},
    people::People,
    texts::numbers::Numbers,
};
use crate::api::types::{
    DayOrigin, Message, MessageGroup, MessageNeighbor, MessageSenderIdentity, RunOrigin,
    SelectionOrigins, Sender, WindowOrigins,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row},
    ensure,
    foundation::{crypto, observability},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(crate) mod backfill;

/// Bump the prefix when writer/day semantics change. A tzdata update changes this key too.
pub(crate) fn algorithm() -> String {
    format!("conversation-origins-1:{}", chrono_tz::IANA_TZDB_VERSION)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    Slack,
    Text,
}
impl Transport {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Text => "text",
        }
    }
    pub(crate) fn of(reference: &Reference) -> Self {
        match reference {
            Reference::Slack(_) => Self::Slack,
            Reference::Text(_) => Self::Text,
        }
    }
    fn boundaries(self) -> &'static str {
        match self {
            Self::Slack => "communications_slack_origins",
            Self::Text => "communications_text_origins",
        }
    }
    fn conversations(self) -> &'static str {
        match self {
            Self::Slack => "communications_conversations",
            Self::Text => "communications_text_conversations",
        }
    }
    fn source(self) -> &'static str {
        match self {
            Self::Slack => "communications_messages",
            Self::Text => "phones_texts",
        }
    }
    fn order(self, descending: bool) -> &'static str {
        match (self, descending) {
            (Self::Slack, false) => "m.ts",
            (Self::Slack, true) => "m.ts DESC",
            (Self::Text, false) => "m.at,m.id",
            (Self::Text, true) => "m.at DESC,m.id DESC",
        }
    }
    /// Native tuple comparisons, always following exact conversation/generation prefixes.
    fn bound(self, alias: &str, op: &str, ts: usize, at: usize, id: usize) -> String {
        match self {
            Self::Slack => format!("{alias}.ts{op}?{ts}"),
            Self::Text => format!("({alias}.at,{alias}.id){op}(?{at},?{id})"),
        }
    }
    fn columns(self) -> &'static str {
        match self {
            Self::Slack => {
                "m.id,m.conversation_id,m.ts,m.at,m.direction,m.sent_by, \
                NULL AS phone_id,CASE WHEN m.direction='out' THEN g.id END AS group_id, \
                NULL AS number"
            }
            Self::Text => {
                "m.id,m.conversation_id,printf('%013d',m.at)||':'||m.id AS ts, \
                m.at,m.direction,CASE WHEN m.direction='out' THEN m.sent_by END AS sent_by, \
                CASE WHEN m.direction='out' AND m.sent_by IS NULL THEN m.device_id END AS phone_id, \
                NULL AS group_id,m.number"
            }
        }
    }
    fn join(self) -> &'static str {
        match self {
            Self::Slack => "LEFT JOIN communications_groups g ON g.id=m.group_id",
            Self::Text => "",
        }
    }
}

/// The only metadata carried between backfill chunks. Names are always read from the origin
/// source row; changing a name cannot split a writer or restart a candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Topology {
    id: String,
    conversation_id: Option<String>,
    ts: String,
    at: i64,
    direction: Direction,
    sent_by: Option<String>,
    phone_id: Option<String>,
    group_id: Option<String>,
    number: Option<String>,
}
impl FromRow for Topology {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation_id: row.get("conversation_id")?,
            ts: row.get("ts")?,
            at: row.get("at")?,
            direction: row.get("direction")?,
            sent_by: row.get("sent_by")?,
            phone_id: row.get("phone_id")?,
            group_id: row.get("group_id")?,
            number: row.get("number")?,
        })
    }
}
impl Topology {
    fn writer(&self) -> String {
        let number = self.number.as_deref().unwrap_or_default();
        if self.direction == Direction::In {
            return format!("in:{number}");
        }
        format!(
            "out:{}:{}:{}:{number}",
            self.sent_by.as_deref().unwrap_or_default(),
            self.phone_id.as_deref().unwrap_or_default(),
            self.group_id.as_deref().unwrap_or_default()
        )
    }
    fn sender_present(&self) -> bool {
        self.sent_by.is_some() || self.phone_id.is_some()
    }
}
/// Use actual timestamp dates, never clock::day's fixture-today override. Invalid chrono dates
/// cannot be indexed; they remain on the legacy reader. This does not normalize stored time.
fn day(zone: Tz, at: i64) -> Result<chrono::NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|at| at.with_timezone(&zone).date_naive())
        .ok_or_else(|| Error::new("communications_origins_date", 409))
}
fn flags(previous: Option<&Topology>, row: &Topology, zone: Tz) -> Result<(bool, bool)> {
    let date = day(zone, row.at)?;
    let Some(previous) = previous else {
        return Ok((true, true));
    };
    let day_start = day(zone, previous.at)? != date;
    // i128 preserves the millisecond subtraction for all representable stored i64 values.
    let start = day_start
        || previous.writer() != row.writer()
        || i128::from(row.at) - i128::from(previous.at) >= 600_000;
    Ok((start, day_start))
}

#[derive(Clone)]
pub(crate) struct IndexState {
    epoch: i64,
    indexed: Option<i64>,
    generation: Option<String>,
    zone: Option<String>,
    algorithm: Option<String>,
}
impl FromRow for IndexState {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            epoch: row.get("source_epoch")?,
            indexed: row.get("indexed_epoch")?,
            generation: row.get("active_generation")?,
            zone: row.get("zone")?,
            algorithm: row.get("algorithm")?,
        })
    }
}
impl IndexState {
    fn ready(&self, zone: &str) -> bool {
        self.generation.is_some()
            && self.indexed == Some(self.epoch)
            && self.zone.as_deref() == Some(zone)
            && self.algorithm.as_deref() == Some(algorithm().as_str())
    }
}
pub(crate) fn state(
    db: &Db,
    transport: Transport,
    conversation: &str,
) -> Result<Option<IndexState>> {
    db.one_as(
        "SELECT source_epoch,indexed_epoch,active_generation,zone,algorithm \
        FROM communications_origin_state WHERE transport=? AND conversation_id=?",
        params![transport.name(), conversation],
    )
}
/// A real SQLite write before any adjacency/state read. If a caller already established a
/// stale deferred snapshot, SQLite refuses its upgrade and the entire enclosing operation fails.
/// No stale neighbor can be reused. The caller's savepoint/transaction rolls this write back too.
pub(crate) fn lock(db: &Db) -> Result<()> {
    db.exec(
        "UPDATE communications_origin_control SET singleton=singleton WHERE singleton=1",
        [],
    )?;
    Ok(())
}
pub(crate) fn ensure_state(db: &Db, transport: Transport, conversation: &str) -> Result<()> {
    db.exec(
        "INSERT INTO communications_origin_state(transport,conversation_id) VALUES (?,?) \
        ON CONFLICT(transport,conversation_id) DO NOTHING",
        params![transport.name(), conversation],
    )?;
    Ok(())
}
fn by_id(db: &Db, transport: Transport, id: &str) -> Result<Option<Topology>> {
    db.one_as(
        &format!(
            "SELECT {} FROM {} m {} WHERE m.id=?",
            transport.columns(),
            transport.source(),
            transport.join()
        ),
        [id],
    )
}
fn neighbor(
    db: &Db,
    transport: Transport,
    conversation: &str,
    row: &Topology,
    after: bool,
) -> Result<Option<Topology>> {
    let bound = transport.bound("m", if after { ">" } else { "<" }, 2, 3, 4);
    // Fixed non-null bind guards preserve one numbered bind shape for both transports.
    db.one_as(
        &format!(
            "SELECT {} FROM {} m {} WHERE m.conversation_id=?1 AND {bound} \
        AND ?2 IS NOT NULL AND ?4 IS NOT NULL ORDER BY {} LIMIT 1",
            transport.columns(),
            transport.source(),
            transport.join(),
            transport.order(!after)
        ),
        params![conversation, row.ts, row.at, row.id],
    )
}
fn remove_boundary(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    row: &Topology,
) -> Result<()> {
    let bound = transport.bound("m", "=", 3, 4, 5);
    db.exec(
        &format!(
            "DELETE FROM {} AS m WHERE conversation_id=?1 AND generation=?2 \
        AND {bound} AND ?3 IS NOT NULL AND ?5 IS NOT NULL",
            transport.boundaries()
        ),
        params![conversation, generation, row.ts, row.at, row.id],
    )?;
    Ok(())
}
pub(crate) fn boundary(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    zone: Tz,
    previous: Option<&Topology>,
    row: &Topology,
) -> Result<()> {
    let (start, day_start) = flags(previous, row, zone)?;
    if !start {
        return Ok(());
    }
    match transport {
        Transport::Slack => db.exec("INSERT INTO communications_slack_origins \
            (conversation_id,generation,ts,message_id,day_start,sender_present) VALUES (?,?,?,?,?,?) \
            ON CONFLICT(conversation_id,generation,ts) DO UPDATE SET message_id=excluded.message_id, \
            day_start=excluded.day_start,sender_present=excluded.sender_present",
            params![conversation,generation,row.ts,row.id,day_start,row.sender_present()])?,
        Transport::Text => db.exec("INSERT INTO communications_text_origins \
            (conversation_id,generation,at,id,day_start,sender_present) VALUES (?,?,?,?,?,?) \
            ON CONFLICT(conversation_id,generation,at,id) DO UPDATE SET \
            day_start=excluded.day_start,sender_present=excluded.sender_present",
            params![conversation,generation,row.at,row.id,day_start,row.sender_present()])?,
    };
    Ok(())
}

/// One known source-row mutation, inside its existing transaction/savepoint. `to` is the
/// destination of an insertion or move. No callbacks in triggers and no connection-local flag.
/// Epoch deltas must equal exactly this row's projected transition; an earlier unknown mutation
/// or an unexpected extra write cannot be acknowledged by the next ordinary send.
/// Audited single INSERT or NULL-owner filing only. The callback must perform exactly
/// that row's placement; refiles, retries and arbitrary mutations use `change`.
pub(crate) fn place<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    mutate(db, transport, id, to, true, write)
}

/// A general known mutation keeps unfinished candidates subject to normal epoch invalidation.
pub(crate) fn change<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    mutate(db, transport, id, to, false, write)
}

fn mutate<T>(
    db: &Db,
    transport: Transport,
    id: &str,
    to: Option<&str>,
    placement: bool,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    db.transaction(|| {
        lock(db)?;
        let before = by_id(db, transport, id)?;
        let mut conversations = std::collections::BTreeSet::new();
        if let Some(conversation) = before
            .as_ref()
            .and_then(|row| row.conversation_id.as_deref())
        {
            conversations.insert(conversation.to_owned());
        }
        if let Some(to) = to {
            conversations.insert(to.to_owned());
        }
        let mut entries = Vec::new();
        for conversation in conversations {
            ensure_state(db, transport, &conversation)?;
            let entry = state(db, transport, &conversation)?.ok_or_else(invariant)?;
            let successor = before
                .as_ref()
                .filter(|row| row.conversation_id.as_deref() == Some(conversation.as_str()))
                .map(|row| neighbor(db, transport, &conversation, row, true))
                .transpose()?
                .flatten();
            let candidate: Option<backfill::Build> = if placement {
                db.one_as(
                    "SELECT generation,source_epoch,zone,algorithm,previous FROM communications_origin_builds \
                    WHERE transport=? AND conversation_id=?",
                    params![transport.name(), conversation],
                )?
            } else {
                None
            };
            entries.push((conversation, entry, successor, candidate));
        }
        let result = write()?;
        let after = by_id(db, transport, id)?;
        if let Some(conversation) = after
            .as_ref()
            .and_then(|row| row.conversation_id.as_deref())
        {
            ensure(
                entries.iter().any(|(id, _, _, _)| id == conversation),
                "communications_origins_owner",
                500,
            )?;
        }
        for (conversation, entry, old_next, candidate) in entries {
            let changed = before != after;
            let from_here = before
                .as_ref()
                .is_some_and(|row| row.conversation_id.as_deref() == Some(conversation.as_str()));
            let to_here = after
                .as_ref()
                .is_some_and(|row| row.conversation_id.as_deref() == Some(conversation.as_str()));
            let expected = i64::from(changed && (from_here || to_here));
            let current = state(db, transport, &conversation)?.ok_or_else(invariant)?;
            if placement
                && before.as_ref().is_none_or(|row| row.conversation_id.is_none())
                && to_here
                && entry.epoch.checked_add(1) == Some(current.epoch)
            {
                if let (Some(candidate), Some(row)) = (candidate, after.as_ref()) {
                    advance_candidate(db, transport, &conversation, &entry, &current, &candidate, row)?;
                }
            }
            let Some(zone) = entry.zone.as_deref().filter(|zone| entry.ready(zone)) else {
                continue;
            };
            if entry.epoch.checked_add(expected) != Some(current.epoch)
                || current.generation != entry.generation
            {
                continue;
            }
            let zone: Tz = zone.parse().map_err(|_| invariant())?;
            let generation = entry.generation.as_deref().ok_or_else(invariant)?;
            let repaired = db.transaction(|| {
                if changed {
                    // Recompute against final adjacency. Each candidate is a compact key, never a
                    // materialized run. Sequential batch callers compose even for adjacent moves.
                    let mut keys = Vec::new();
                    if from_here {
                        keys.extend(before.iter().cloned());
                    }
                    keys.extend(old_next);
                    if to_here {
                        let row = after.as_ref().ok_or_else(invariant)?;
                        keys.push(row.clone());
                        keys.extend(neighbor(db, transport, &conversation, row, true)?);
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    for key in keys {
                        if !seen.insert((key.ts.clone(), key.at, key.id.clone())) {
                            continue;
                        }
                        remove_boundary(db, transport, &conversation, generation, &key)?;
                        let row = by_id(db, transport, &key.id)?;
                        let Some(row) = row.filter(|row| {
                            row.conversation_id.as_deref() == Some(conversation.as_str())
                                && row.ts == key.ts
                                && row.at == key.at
                        }) else {
                            continue;
                        };
                        let previous = neighbor(db, transport, &conversation, &row, false)?;
                        boundary(
                            db,
                            transport,
                            &conversation,
                            generation,
                            zone,
                            previous.as_ref(),
                            &row,
                        )?;
                    }
                }
                db.exec(
                    "UPDATE communications_origin_state SET indexed_epoch=source_epoch WHERE \
                transport=? AND conversation_id=? AND source_epoch=? AND indexed_epoch=? \
                AND active_generation=?",
                    params![
                        transport.name(),
                        conversation,
                        current.epoch,
                        entry.indexed,
                        generation
                    ],
                )?;
                Ok(())
            });
            match repaired {
                Ok(()) => {}
                // The source operation has no calendar prerequisite. Roll back only derived
                // repair, retaining the source epoch so this generation stays explicitly dirty.
                Err(error) if error.code == "communications_origins_date" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    })
}
fn advance_candidate(
    db: &Db,
    transport: Transport,
    conversation: &str,
    entry: &IndexState,
    current: &IndexState,
    candidate: &backfill::Build,
    row: &Topology,
) -> Result<()> {
    if candidate.epoch != entry.epoch
        || candidate.algorithm != algorithm()
        || candidate.zone.parse::<Tz>().is_err()
    {
        return Ok(());
    }
    let Some(previous) = candidate.previous.as_deref() else {
        return Ok(());
    };
    // A candidate is derived state. An unsupported cursor cannot become a source-write
    // prerequisite: keep its old epoch so the next chunk starts a fresh build.
    let Ok(previous_row) = serde_json::from_str::<Topology>(previous) else {
        return Ok(());
    };
    let bound = transport.bound("m", ">", 2, 3, 4);
    let suffix = db.count(
        &format!(
            "SELECT count(*) FROM {} m WHERE m.id=?1 AND {bound} \
            AND ?2 IS NOT NULL AND ?4 IS NOT NULL",
            transport.source()
        ),
        params![row.id, previous_row.ts, previous_row.at, previous_row.id],
    )? == 1;
    if !suffix {
        return Ok(());
    }
    let advanced = db.exec(
        "UPDATE communications_origin_builds SET source_epoch=?1 WHERE transport=?2 \
        AND conversation_id=?3 AND generation=?4 AND source_epoch=?5 AND zone=?6 \
        AND algorithm=?7 AND previous=?8",
        params![
            current.epoch,
            transport.name(),
            conversation,
            candidate.generation,
            candidate.epoch,
            candidate.zone,
            candidate.algorithm,
            previous
        ],
    )?;
    ensure(advanced == 1, "communications_origins_invariant", 500)?;
    Ok(())
}

fn invariant() -> Error {
    Error::new("communications_origins_invariant", 500)
}

/// A source projection for an origin, obtained by exact source PK/place only. The group JOIN
/// and People/Numbers calls are the same projection owners as body hydration.
struct Projection {
    topology: Topology,
    name: Option<String>,
    group_name: Option<String>,
}
impl FromRow for Projection {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            topology: Topology::from_row(row)?,
            name: row.get("sender_name")?,
            group_name: row.get("group_name")?,
        })
    }
}
impl Projection {
    fn shown(self, people: &People, numbers: Option<&Numbers>) -> RunOrigin {
        let row = self.topology;
        let sender = row
            .sent_by
            .as_ref()
            .map(|id| people.sent_by(id, self.name.as_deref().unwrap_or_default()))
            .or_else(|| {
                row.phone_id.as_ref().map(|id| Sender {
                    user_id: None,
                    phone_id: Some(id.clone()),
                    name: numbers
                        .and_then(|numbers| numbers.phone(id))
                        .unwrap_or("Phone")
                        .to_owned(),
                    code: None,
                    photo: None,
                })
            });
        RunOrigin {
            id: row.id,
            ts: row.ts,
            at: row.at,
            direction: row.direction,
            sender,
            group: row
                .group_id
                .zip(self.group_name)
                .map(|(id, name)| MessageGroup { id, name }),
            number: row.number,
        }
    }
}
fn identity(sender: &Sender) -> MessageSenderIdentity {
    MessageSenderIdentity {
        user_id: sender.user_id.clone(),
        phone_id: sender.phone_id.clone(),
        name: sender.name.clone(),
    }
}
fn compact(row: &RunOrigin) -> MessageNeighbor {
    MessageNeighbor {
        id: row.id.clone(),
        ts: row.ts.clone(),
        at: row.at,
        direction: row.direction,
        sender: row.sender.as_ref().map(identity),
        group_id: row.group.as_ref().map(|group| group.id.clone()),
        number: row.number.clone(),
    }
}
fn project(
    db: &Db,
    transport: Transport,
    conversation: &str,
    key: &Topology,
    people: &People,
    numbers: Option<&Numbers>,
) -> Result<RunOrigin> {
    let group_name = if transport == Transport::Slack {
        "g.name"
    } else {
        "NULL"
    };
    let row: Projection = db
        .one_as(
            &format!(
                "SELECT {},m.sender_name,{group_name} AS group_name \
        FROM {} m {} WHERE m.id=? AND m.conversation_id=?",
                transport.columns(),
                transport.source(),
                transport.join()
            ),
            params![key.id, conversation],
        )?
        .ok_or_else(invariant)?;
    ensure(
        row.topology.ts == key.ts && row.topology.at == key.at,
        "communications_origins_invariant",
        500,
    )?;
    Ok(row.shown(people, numbers))
}
/// Sparse boundary seeks. Joins happen AFTER the one-row seek, so no SQL filter can turn a
/// missing source/group into a backward history walk.
enum BoundaryKind {
    Run,
    Day,
    Sender,
}
struct BoundarySeek<'a> {
    kind: BoundaryKind,
    before: bool,
    lower: Option<&'a Topology>,
}
fn seek(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    key: &Topology,
    query: BoundarySeek<'_>,
) -> Result<Option<Topology>> {
    let BoundarySeek {
        kind,
        before,
        lower,
    } = query;
    // Sparse flags must seek their partial index. SQLite may prefer the wider primary
    // key for Slack's message_id projection and then filter an entire history prefix.
    let (subset, index) = match kind {
        BoundaryKind::Run => ("", String::new()),
        BoundaryKind::Day => (
            " AND m.day_start=1",
            format!(" INDEXED BY {}_days", transport.boundaries()),
        ),
        BoundaryKind::Sender => (
            " AND m.sender_present=1",
            format!(" INDEXED BY {}_senders", transport.boundaries()),
        ),
    };
    let bound = transport.bound("m", if before { "<" } else { "<=" }, 3, 4, 5);
    let lower_sql = if lower.is_some() {
        format!(" AND {}", transport.bound("m", ">=", 6, 7, 8))
    } else {
        String::new()
    };
    let columns = match transport {
        Transport::Slack => "m.message_id AS id,m.ts,0 AS at",
        Transport::Text => "m.id,printf('%013d',m.at)||':'||m.id AS ts,m.at",
    };
    let sql = format!(
        "SELECT {columns},m.conversation_id,'in' AS direction,NULL AS sent_by, \
        NULL AS phone_id,NULL AS group_id,NULL AS number FROM {} m{index} WHERE m.conversation_id=?1 \
        AND m.generation=?2 AND {bound}{subset}{lower_sql} AND ?3 IS NOT NULL AND ?5 IS NOT NULL {} \
        ORDER BY {} LIMIT 1",
        transport.boundaries(),
        if lower.is_some() {
            "AND ?6 IS NOT NULL AND ?8 IS NOT NULL"
        } else {
            ""
        },
        transport.order(true)
    );
    let found: Option<Topology> = if let Some(lower) = lower {
        db.one_as(
            &sql,
            params![
                conversation,
                generation,
                key.ts,
                key.at,
                key.id,
                lower.ts,
                lower.at,
                lower.id
            ],
        )?
    } else {
        db.one_as(
            &sql,
            params![conversation, generation, key.ts, key.at, key.id],
        )?
    };
    // Slack boundary stores exact ts/id, not at. Resolve the real persisted milliseconds by PK.
    found
        .map(|key| {
            by_id(db, transport, &key.id)?
                .filter(|row| {
                    row.conversation_id.as_deref() == Some(conversation)
                        && row.ts == key.ts
                        && (transport == Transport::Slack || row.at == key.at)
                })
                .ok_or_else(invariant)
        })
        .transpose()
}
fn selection(
    db: &Db,
    transport: Transport,
    conversation: &str,
    generation: &str,
    messages: &[Message],
    people: &People,
    numbers: Option<&Numbers>,
) -> Result<Option<SelectionOrigins>> {
    let Some(first) = messages.first() else {
        return Ok(None);
    };
    let key = Topology {
        id: first.id.clone(),
        conversation_id: Some(conversation.to_owned()),
        ts: first.ts.clone(),
        at: first.at,
        direction: first.direction,
        sent_by: None,
        phone_id: None,
        group_id: None,
        number: None,
    };
    let run = seek(
        db,
        transport,
        conversation,
        generation,
        &key,
        BoundarySeek {
            kind: BoundaryKind::Run,
            before: false,
            lower: None,
        },
    )?
    .ok_or_else(invariant)?;
    let day = seek(
        db,
        transport,
        conversation,
        generation,
        &key,
        BoundarySeek {
            kind: BoundaryKind::Day,
            before: false,
            lower: None,
        },
    )?
    .ok_or_else(invariant)?;
    let sender = seek(
        db,
        transport,
        conversation,
        generation,
        &run,
        BoundarySeek {
            kind: BoundaryKind::Sender,
            before: true,
            lower: Some(&day),
        },
    )?;
    let sent_before_origin = sender
        .map(|key| project(db, transport, conversation, &key, people, numbers))
        .transpose()?
        .and_then(|row| row.sender)
        .map(|sender| identity(&sender));
    let before_origin = neighbor(db, transport, conversation, &run, false)?
        .map(|key| {
            project(db, transport, conversation, &key, people, numbers).map(|row| compact(&row))
        })
        .transpose()?;
    let origin = project(db, transport, conversation, &run, people, numbers)?;
    Ok(Some(SelectionOrigins {
        origin,
        before_origin,
        sent_before_origin,
        first_day: DayOrigin {
            id: day.id,
            ts: day.ts,
            at: day.at,
            number: day.number,
        },
    }))
}
/// Called only by the opt-in reader inside its body/revision DSP snapshot. Unready is explicit;
/// HTTP never backfills, repairs, scans a prefix or hydrates an origin body.
pub(crate) fn read(
    db: &Db,
    reference: &Reference,
    zone: Tz,
    selected: &[Message],
    latest: &[Message],
    people: &People,
    numbers: Option<&Numbers>,
) -> Result<WindowOrigins> {
    let transport = Transport::of(reference);
    let mut answer = WindowOrigins {
        version: 1,
        algorithm: algorithm(),
        time_zone: zone.name().to_owned(),
        ready: false,
        selected: None,
        latest: None,
    };
    let Some(index) =
        state(db, transport, reference.id())?.filter(|index| index.ready(zone.name()))
    else {
        return Ok(answer);
    };
    let generation = index.generation.as_deref().ok_or_else(invariant)?;
    let facts = || -> Result<_> {
        Ok((
            selection(
                db,
                transport,
                reference.id(),
                generation,
                selected,
                people,
                numbers,
            )?,
            selection(
                db,
                transport,
                reference.id(),
                generation,
                latest,
                people,
                numbers,
            )?,
        ))
    };
    match facts() {
        Ok((selected, latest)) => {
            answer.ready = true;
            answer.selected = selected;
            answer.latest = latest;
        }
        Err(error) if error.code == "communications_origins_invariant" => {
            observability::event(
                "error",
                "communications_origins_invariant",
                json!({"conversationId":reference.id()}),
            );
        }
        Err(error) => return Err(error),
    }
    Ok(answer)
}
