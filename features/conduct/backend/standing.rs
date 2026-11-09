//! Where someone stands in each category: the rules a writeup's tier climbs and steps down by.
//! Pure, so the rules are tested apart from storage. Worked out on every read, so a change to a
//! category's step downs applies to everyone at once.
use crate::api::types::{
    ConductCategory, ConductStanding, ConductStepDown, ConductTier, ConductUnit, ConductWriteupRow,
};
use chrono::{Days, Months, NaiveDate};
use std::collections::BTreeMap;

/// Where everyone stands, and where each writeup's person stood just before it.
pub(crate) struct Worked {
    pub standings: Vec<ConductStanding>,
    pub stood_before: BTreeMap<String, ConductTier>,
}

/// A record in a category that isn't clean: its tier, and the day its wait to step down began.
#[derive(Clone, Copy)]
struct Held {
    tier: ConductTier,
    since: NaiveDate,
}

/// Each person's record in each category as of `today`. A writeup puts them at its tier, or
/// keeps them where they stand when that's higher, and starts the wait again; each step down
/// starts the next tier's wait. `writeups` are the newest first, as the overview lists them.
pub(crate) fn work_out(
    categories: &[ConductCategory],
    writeups: &[ConductWriteupRow],
    today: NaiveDate,
) -> Worked {
    let steps_of = |category: &str| {
        categories
            .iter()
            .find(|c| c.id == category)
            .map_or(&[][..], |c| c.step_downs.as_slice())
    };
    let mut records: BTreeMap<(&str, &str), (Held, &ConductWriteupRow)> = BTreeMap::new();
    let mut stood_before = BTreeMap::new();
    // The oldest first, the order they were written in.
    for writeup in writeups.iter().rev() {
        let Ok(on) = NaiveDate::parse_from_str(&writeup.written_on, "%Y-%m-%d") else {
            continue;
        };
        let key = (writeup.person.as_str(), writeup.category_id.as_str());
        let stood = records
            .get(&key)
            .and_then(|(held, _)| stepped(*held, steps_of(key.1), on));
        let mut tier = writeup.tier;
        if let Some(stood) = stood {
            stood_before.insert(writeup.id.clone(), stood.tier);
            if stood.tier.rank() > tier.rank() {
                tier = stood.tier;
            }
        }
        records.insert(key, (Held { tier, since: on }, writeup));
    }
    let standings = records
        .into_iter()
        .filter_map(|((person, category), (held, latest))| {
            let steps = steps_of(category);
            let held = stepped(held, steps, today)?;
            let next = step(steps, held.tier).and_then(|s| Some((due(held.since, s)?, s.to)));
            Some(ConductStanding {
                person: person.to_owned(),
                category_id: category.to_owned(),
                tier: held.tier,
                latest_id: latest.id.clone(),
                latest_on: latest.written_on.clone(),
                steps_down_on: next.map(|(on, _)| on.to_string()),
                steps_down_to: next.and_then(|(_, to)| to),
            })
        })
        .collect();
    Worked {
        standings,
        stood_before,
    }
}

/// `held` once every step down due by `on` is taken: none once the record is clean.
fn stepped(held: Held, steps: &[ConductStepDown], on: NaiveDate) -> Option<Held> {
    let mut now = held;
    while let Some(step) = step(steps, now.tier) {
        match due(now.since, step) {
            Some(due) if due <= on => {
                now = Held {
                    tier: step.to?,
                    since: due,
                };
            }
            _ => break,
        }
    }
    Some(now)
}

/// The step down `tier` takes, if it ever does.
fn step(steps: &[ConductStepDown], tier: ConductTier) -> Option<&ConductStepDown> {
    steps.iter().find(|step| step.tier == tier)
}

/// The day a wait begun on `since` ends: the same day of the month for months, or the month's
/// last where it has none.
fn due(since: NaiveDate, step: &ConductStepDown) -> Option<NaiveDate> {
    let after = u32::try_from(step.after).ok()?;
    match step.unit {
        ConductUnit::Days => since.checked_add_days(Days::new(after.into())),
        ConductUnit::Weeks => since.checked_add_days(Days::new(u64::from(after) * 7)),
        ConductUnit::Months => since.checked_add_months(Months::new(after)),
    }
}

#[cfg(test)]
#[path = "../tests/backend/standing.rs"]
mod tests;
