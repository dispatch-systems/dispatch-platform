//! The source tables as origins read them: each transport's SQL dialect, a source row's topology
//! and the seeks that find one row by its ID or beside another.
use super::super::model::{Direction, Reference};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    db::{Db, FromRow, Row},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

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
    pub(super) fn boundaries(self) -> &'static str {
        match self {
            Self::Slack => "communications_slack_origins",
            Self::Text => "communications_text_origins",
        }
    }
    pub(super) fn conversations(self) -> &'static str {
        match self {
            Self::Slack => "communications_conversations",
            Self::Text => "communications_text_conversations",
        }
    }
    pub(super) fn source(self) -> &'static str {
        match self {
            Self::Slack => "communications_messages",
            Self::Text => "phones_texts",
        }
    }
    pub(super) fn order(self, descending: bool) -> &'static str {
        match (self, descending) {
            (Self::Slack, false) => "m.ts",
            (Self::Slack, true) => "m.ts DESC",
            (Self::Text, false) => "m.at,m.id",
            (Self::Text, true) => "m.at DESC,m.id DESC",
        }
    }
    /// Native tuple comparisons, always following exact conversation/generation prefixes.
    pub(super) fn bound(self, alias: &str, op: &str, ts: usize, at: usize, id: usize) -> String {
        match self {
            Self::Slack => format!("{alias}.ts{op}?{ts}"),
            Self::Text => format!("({alias}.at,{alias}.id){op}(?{at},?{id})"),
        }
    }
    pub(super) fn columns(self) -> &'static str {
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
    pub(super) fn join(self) -> &'static str {
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
    pub(super) id: String,
    pub(super) conversation_id: Option<String>,
    pub(super) ts: String,
    pub(super) at: i64,
    pub(super) direction: Direction,
    pub(super) sent_by: Option<String>,
    pub(super) phone_id: Option<String>,
    pub(super) group_id: Option<String>,
    pub(super) number: Option<String>,
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
    pub(super) fn sender_present(&self) -> bool {
        self.sent_by.is_some() || self.phone_id.is_some()
    }
}
/// Use actual timestamp dates, never clock::day's fixture-today override. Invalid chrono dates
/// cannot be indexed; their conversation's index stays unready, and `read` answers its windows
/// without origins. This does not normalize stored time.
fn day(zone: Tz, at: i64) -> Result<chrono::NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|at| at.with_timezone(&zone).date_naive())
        .ok_or_else(|| Error::new("communications_origins_date", 409))
}
pub(super) fn flags(previous: Option<&Topology>, row: &Topology, zone: Tz) -> Result<(bool, bool)> {
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
pub(super) fn by_id(db: &Db, transport: Transport, id: &str) -> Result<Option<Topology>> {
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
pub(super) fn neighbor(
    db: &Db,
    transport: Transport,
    conversation: &str,
    row: &Topology,
    after: bool,
) -> Result<Option<Topology>> {
    let place = Place {
        ts: &row.ts,
        at: row.at,
        id: &row.id,
    };
    adjacent(
        db,
        transport,
        conversation,
        &place,
        after,
        false,
        transport.columns(),
    )
}
/// A place in a conversation's order to seek from: the row `id` written at `at`, at `ts`.
pub(crate) struct Place<'a> {
    pub ts: &'a str,
    pub at: i64,
    pub id: &'a str,
}
/// The nearest source row before or after `place` in its conversation, of what the DSP sent
/// alone when `outgoing`, read as `columns`: one indexed seek, whoever asks.
pub(super) fn adjacent<T: FromRow>(
    db: &Db,
    transport: Transport,
    conversation: &str,
    place: &Place<'_>,
    after: bool,
    outgoing: bool,
    columns: &str,
) -> Result<Option<T>> {
    let bound = transport.bound("m", if after { ">" } else { "<" }, 2, 3, 4);
    // These fragments are fixed product query choices, never caller-provided SQL.
    let outgoing = if outgoing {
        " AND m.direction='out'"
    } else {
        ""
    };
    // Fixed non-null bind guards preserve one numbered bind shape for both transports.
    db.one_as(
        &format!(
            "SELECT {columns} FROM {} m {} WHERE m.conversation_id=?1 AND {bound}{outgoing} \
        AND ?2 IS NOT NULL AND ?4 IS NOT NULL ORDER BY {} LIMIT 1",
            transport.source(),
            transport.join(),
            transport.order(!after)
        ),
        params![conversation, place.ts, place.at, place.id],
    )
}
