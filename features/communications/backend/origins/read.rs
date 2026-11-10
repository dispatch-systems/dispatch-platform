//! The read side: a window's exact origins, from sparse boundary seeks and source rows read by
//! their exact place, and the rows beside any window's selection, each as the one projection of
//! a source row names who sent it.
use super::{
    super::{model::Reference, people::People, texts::numbers::Numbers},
    algorithm, invariant,
    source::{Place, Topology, Transport, adjacent, by_id, neighbor},
    state,
};
use crate::api::types::{
    DayOrigin, Message, MessageGroup, MessageNeighbor, MessageSenderIdentity, RunOrigin,
    SelectionOrigins, Sender, WindowOrigins,
};
use chrono_tz::Tz;
use dispatch_core::{
    Result,
    db::{Db, FromRow, Row},
    ensure,
    foundation::observability,
};
use rusqlite::params;
use serde_json::json;

/// A source row's render identity, for an origin or a selection's neighbor, obtained by exact
/// source PK/place or one neighbor seek only. The group JOIN and People/Numbers calls are the
/// same projection owners as body hydration.
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
    /// What a projection reads of a source row: its topology, the name it keeps for its sender
    /// and its group's name.
    fn columns(transport: Transport) -> String {
        let group_name = if transport == Transport::Slack {
            "g.name"
        } else {
            "NULL"
        };
        format!(
            "{},m.sender_name,{group_name} AS group_name",
            transport.columns()
        )
    }
    /// Who sent the row, exactly as conversations::messages / texts::reading::messages name a
    /// message's sender, without hydrating a Message: the member, under the name the row keeps,
    /// else the phone, under its current name. `Transport::columns` decides which a row has:
    /// Slack's sender is sent_by when present; a text has a member or a phone only when outgoing.
    fn sender(&self, numbers: Option<&Numbers>) -> Option<MessageSenderIdentity> {
        let row = &self.topology;
        row.sent_by
            .as_ref()
            .map(|id| MessageSenderIdentity {
                user_id: Some(id.clone()),
                phone_id: None,
                name: self.name.clone().unwrap_or_default(),
            })
            .or_else(|| {
                row.phone_id.as_ref().map(|id| MessageSenderIdentity {
                    user_id: None,
                    phone_id: Some(id.clone()),
                    name: numbers
                        .and_then(|numbers| numbers.phone(id))
                        .unwrap_or("Phone")
                        .to_owned(),
                })
            })
    }
    /// The row as a selection's neighbor: its place and writer, nothing of its body.
    fn beside(self, numbers: Option<&Numbers>) -> MessageNeighbor {
        let sender = self.sender(numbers);
        let row = self.topology;
        MessageNeighbor {
            id: row.id,
            ts: row.ts,
            at: row.at,
            direction: row.direction,
            sender,
            group_id: row.group_id,
            number: row.number,
        }
    }
    fn shown(self, people: &People, numbers: Option<&Numbers>) -> RunOrigin {
        let sender = self.sender(numbers).map(|sender| match &sender.user_id {
            Some(user) => people.sent_by(user, &sender.name),
            None => Sender {
                user_id: None,
                phone_id: sender.phone_id,
                name: sender.name,
                code: None,
                photo: None,
            },
        });
        let row = self.topology;
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
    let row: Projection = db
        .one_as(
            &format!(
                "SELECT {} FROM {} m {} WHERE m.id=? AND m.conversation_id=?",
                Projection::columns(transport),
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
/// The source row just before or after `place` in `reference`'s conversation, of what the DSP
/// sent alone when `outgoing`, as a window's render context names a selection's neighbor. One
/// indexed seek of the source table: it reads no origin state, so it needs no ready generation
/// and answers no origin invariant.
pub(crate) fn beside(
    db: &Db,
    reference: &Reference,
    place: Place<'_>,
    after: bool,
    outgoing: bool,
    numbers: Option<&Numbers>,
) -> Result<Option<MessageNeighbor>> {
    let transport = Transport::of(reference);
    let row: Option<Projection> = adjacent(
        db,
        transport,
        reference.id(),
        &place,
        after,
        outgoing,
        &Projection::columns(transport),
    )?;
    Ok(row.map(|row| row.beside(numbers)))
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
