//! The facts every answer is made of: each source's rows for a DSP's days, in the DSP's
//! own time, with the days each source has. Each feature reads its own and answers for
//! them here: by driver and day for the answers that join every feature's, and where
//! packages were delivered for those that place what they count. Which of them an answer
//! may read is `access`'s.
use super::scope::{People, Period, Person};
use crate::{
    Result,
    contracts::{AgentArea, DriverSource, Dsp},
    db::Store,
    manifest::registry,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// A time of day where the DSP is, as "08:42", from epoch milliseconds.
pub fn clock(ms: Option<i64>, zone: chrono_tz::Tz) -> Option<String> {
    let at = chrono::DateTime::from_timestamp_millis(ms?)?;
    Some(at.with_timezone(&zone).format("%H:%M").to_string())
}
/// A missing meal or any unfinished meal leaves its total duration unknown.
pub fn total_minutes(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut values = values.peekable();
    values.peek()?;
    values.sum()
}
pub fn zone(dsp: &Dsp) -> chrono_tz::Tz {
    dsp.timezone.parse().unwrap_or(chrono_tz::UTC)
}

/// Each day a source holds, and for routes whether the day is final or a snapshot.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub enabled: bool,
    pub days: Vec<String>,
    pub missing: Vec<String>,
    pub snapshots: Vec<String>,
}
/// Written compactly: how many of the days the source holds, and the missing and snapshot
/// days as ranges, never every day by name.
impl Serialize for Coverage {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut out = serde_json::Map::new();
        if !self.enabled {
            out.insert("enabled".into(), Value::Bool(false));
        } else {
            out.insert("collected".into(), self.days.len().into());
            out.insert("of".into(), (self.days.len() + self.missing.len()).into());
            if !self.missing.is_empty() {
                out.insert("missing".into(), super::shape::ranges(&self.missing));
            }
            if !self.snapshots.is_empty() {
                out.insert("snapshots".into(), super::shape::ranges(&self.snapshots));
            }
        }
        Value::Object(out).serialize(serializer)
    }
}
impl Coverage {
    pub fn of(enabled: bool, held: BTreeSet<String>, period: &Period) -> Self {
        if !enabled {
            return Self::default();
        }
        let (days, missing) = period
            .days()
            .into_iter()
            .partition(|day| held.contains(day));
        Self {
            enabled,
            days,
            missing,
            snapshots: vec![],
        }
    }
}

/// Where packages were delivered, as the feature that holds the routes answers for it.
pub struct Places {
    /// The kind of data addresses are read as.
    pub area: AgentArea,
    /// Each package's address, where its route was collected.
    pub of: fn(&Store, &Dsp, &[String]) -> Result<Addresses>,
}
/// One-line addresses, by tracking ID.
pub type Addresses = HashMap<String, String>;
/// Where packages were delivered, by the feature that answers for it.
pub fn places() -> &'static Places {
    registry()
        .features
        .iter()
        .find_map(|feature| feature.mcp.places.as_ref())
        .expect("a feature answers where packages were delivered")
}

/// One kind of a DSP's facts by driver and day, which `drivers/{driver}` joins into a
/// driver's days and `team` into the team's table. A feature answers for each kind it
/// holds, in its manifest's `mcp`.
pub trait Daily: Sync {
    /// The kind of data it is read as.
    fn area(&self) -> AgentArea;
    /// Its name under an answer's `coverage`.
    fn coverage(&self) -> &'static str;
    /// Its name for its records on each day of a driver's full report.
    fn records(&self) -> &'static str;
    /// Its columns of a driver's days.
    fn columns(&self) -> &'static [&'static str];
    /// Whether its days are assessed one at a time, so that a question needing it is held
    /// to `DAILY_LONGEST` days.
    fn assessed(&self) -> bool {
        false
    }
    /// Its metrics whose team total is their value over every record, not the sum of the
    /// drivers', as a total that one unknown makes unknown.
    fn pooled(&self) -> &'static [&'static str] {
        &[]
    }
    /// What it holds in a period: everyone's, or one person's.
    fn gather(
        &self,
        db: &Store,
        dsp: &Dsp,
        period: &Period,
        people: &People,
        person: Option<&Person>,
    ) -> Result<Box<dyn Facts>>;
    /// Nothing, for an answer that doesn't read it.
    fn none(&self) -> Box<dyn Facts>;
}

/// What one kind holds for a period: its records, in its own order, and the days it has.
pub trait Facts {
    fn coverage(&self) -> &Coverage;
    fn count(&self) -> usize;
    /// A record's day, and whom it names: the source, the ID it knows them by and the name
    /// it gives them.
    fn whose(&self, record: usize) -> (&str, DriverSource, &str, &str);
    /// A record in full, for a driver's full report.
    fn record(&self, record: usize) -> Value;
    /// Whether a record makes a line of a driver's days.
    fn lined(&self, _record: usize) -> bool {
        true
    }
    /// Its columns of one of a driver's days, from that day's records that make a line,
    /// which may be none.
    fn line(&self, records: &[usize]) -> Vec<Value>;
    /// Its figures in a driver's report, by name.
    fn totals(&self) -> Vec<(&'static str, Value)>;
    /// One of its kind's metrics over some of its records, never none.
    fn metric(&self, name: &str, records: &[usize]) -> Option<Value>;
}
