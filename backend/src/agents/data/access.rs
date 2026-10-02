//! What a key or app reads at one DSP: the kinds of data it is allowed there, each from a
//! feature the DSP has switched on, or one switched off when it bypasses features. Every
//! answer is gated here, so no tool can read past what the owner allowed.
use super::{
    Failure, Refusal,
    scope::{param, pick_dsp},
};
use crate::{
    Result,
    agents::Caller,
    contracts::{AgentArea, AgentReads, AgentSource, Dsp},
    db::Store,
};
use serde_json::{Value, json};

/// The features a DSP has switched on that agents read from.
pub fn switched_on(db: &Store, dsp: &str) -> Result<Vec<AgentSource>> {
    let on = db.features(dsp)?;
    let has = |id: &str| on.iter().any(|f| f == id);
    Ok(AgentSource::ALL
        .into_iter()
        .filter(|source| match source {
            AgentSource::Routes => has("routes"),
            AgentSource::Timecards => has("timecard.daily") || has("timecard.employees"),
            AgentSource::MealBreaks => has("timecard.meal_breaks"),
            AgentSource::Dvic => has("dvic"),
            AgentSource::Scorecard => has("scorecard"),
        })
        .collect())
}

/// How a kind of data reads at a DSP: from a feature it has on; by bypassing one it has
/// switched off; or not at all, as the key or app isn't allowed it, or the feature is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    On,
    Bypassed,
    NotAllowed,
    Off,
}

/// One key's or app's reads at one DSP.
pub struct Access<'a> {
    pub dsp: &'a Dsp,
    connection: &'a str,
    reads: &'a AgentReads,
    on: Vec<AgentSource>,
}
impl<'a> Access<'a> {
    /// What it reads at the DSP a request names, or its only one.
    pub fn of(db: &Store, caller: &'a Caller, query: &Value) -> std::result::Result<Self, Failure> {
        let dsp = pick_dsp(caller, param(query, "dsp"))?;
        Ok(Self {
            dsp,
            connection: &caller.name,
            reads: caller.reads_at(&dsp.id),
            on: switched_on(db, &dsp.id)?,
        })
    }
    pub fn on(&self, source: AgentSource) -> bool {
        self.on.contains(&source)
    }
    /// How a kind of data reads here. Delivery addresses come with the routes, so they are
    /// allowed only with them.
    pub fn read(&self, area: AgentArea) -> Read {
        let allowed = self.reads.has(area)
            && (area != AgentArea::Locations || self.reads.has(AgentArea::Routes));
        if !allowed {
            Read::NotAllowed
        } else if self.on(area.source()) {
            Read::On
        } else if self.reads.bypass {
            Read::Bypassed
        } else {
            Read::Off
        }
    }
    /// Whether it reads a kind of data here, one way or the other.
    pub fn reads(&self, area: AgentArea) -> bool {
        matches!(self.read(area), Read::On | Read::Bypassed)
    }
    /// Whether it reads anything a feature holds here.
    pub fn reads_from(&self, source: AgentSource) -> bool {
        AgentArea::ALL
            .into_iter()
            .any(|area| area.source() == source && self.reads(area))
    }
    /// How a kind of data reads here, or the refusal that tells the agent why it doesn't:
    /// not allowed, or switched off. Addresses are refused as the routes are, when those are.
    pub fn check(&self, area: AgentArea) -> std::result::Result<Read, Refusal> {
        if area == AgentArea::Locations {
            self.check(AgentArea::Routes)?;
        }
        match self.read(area) {
            Read::NotAllowed => Err(Refusal::new(
                403,
                "not_allowed",
                format!(
                    "{} can't read {} at {}. Tell the user they can allow it for this \
                     connection on the Agents page in Dispatch.",
                    self.connection,
                    area.label(),
                    self.dsp.name
                ),
            )),
            Read::Off => Err(switched_off(self.dsp, area.source())),
            read => Ok(read),
        }
    }
}

/// A source the DSP has switched off: refused by name, so the agent tells the user rather
/// than answering from something else.
pub fn switched_off(dsp: &Dsp, source: AgentSource) -> Refusal {
    Refusal::new(
        403,
        "source_off",
        format!(
            "{} has {} switched off, so nothing from it can be read. Tell the user it is \
             switched off; don't work the answer out from other tools.",
            dsp.name,
            source.switch()
        ),
    )
}

/// Names a feature an answer read by bypassing it, under `bypassed`, once.
pub fn bypassed(answer: &mut Value, source: AgentSource) {
    let switch = json!(source.switch());
    match answer.get_mut("bypassed").and_then(Value::as_array_mut) {
        Some(named) if named.contains(&switch) => {}
        Some(named) => named.push(switch),
        None => answer["bypassed"] = json!([switch]),
    }
}
