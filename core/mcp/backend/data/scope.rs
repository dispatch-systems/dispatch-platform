//! What a request names, read the way a person writes it: the DSP, the days and the
//! driver. Anything unclear is refused with what it could have meant.
use super::{
    Refusal,
    access::{self, Access, Read},
};
use crate::{
    Result, State,
    accounts::api::types::Dsp,
    db::Store,
    foundation::{names::name_key, weeks},
    manifest::registry,
    mcp::{
        Caller,
        api::types::{AgentArea, AgentSource, DriverSource, DriverStatus},
    },
};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// The longest period one request may cover. Routes, packages and DVIC are read in one
/// range. Timecards and meal breaks assess individual days, so a question that needs
/// them stops at [`DAILY_LONGEST`].
pub const LONGEST_DAYS: i64 = 366;
/// The longest period for a question that assesses timecards or meal breaks.
pub const DAILY_LONGEST: i64 = 92;
/// The period a question that names no days is about.
pub const DEFAULT_PERIOD: &str = "last 30 days";

/// A query parameter as text, trimmed; empty when absent.
pub fn param<'a>(query: &'a Value, name: &str) -> &'a str {
    query.get(name).and_then(Value::as_str).unwrap_or("").trim()
}

/// The DSP a request is about: named by id or name, or the key's only DSP.
pub fn pick_dsp<'a>(caller: &'a Caller, wanted: &str) -> std::result::Result<&'a Dsp, Refusal> {
    let names = || {
        caller
            .dsps
            .iter()
            .map(|d| d.name.clone())
            .collect::<Vec<_>>()
    };
    if wanted.is_empty() {
        return match caller.dsps.as_slice() {
            [one] => Ok(one),
            [] => Err(Refusal::new(
                403,
                "no_dsps",
                "This key reaches no active DSP.",
            )),
            _ => Err(Refusal::new(
                400,
                "dsp_required",
                "This key reaches several DSPs; name one with `dsp`.",
            )
            .choices(names())),
        };
    }
    let lower = wanted.to_lowercase();
    if let Some(dsp) = caller
        .dsps
        .iter()
        .find(|d| d.id == wanted || d.name.to_lowercase() == lower)
    {
        return Ok(dsp);
    }
    let close: Vec<&Dsp> = caller
        .dsps
        .iter()
        .filter(|d| d.name.to_lowercase().contains(&lower))
        .collect();
    match close.as_slice() {
        [one] => Ok(one),
        [] => Err(Refusal::new(
            404,
            "dsp_not_found",
            format!("No DSP this key reaches is called “{wanted}”."),
        )
        .choices(names())),
        many => Err(Refusal::new(
            400,
            "dsp_ambiguous",
            format!("Several DSPs match “{wanted}”; name one exactly."),
        )
        .choices(many.iter().map(|d| d.name.clone()).collect())),
    }
}

/// Today where the DSP is.
pub fn today(dsp: &Dsp) -> NaiveDate {
    let zone: chrono_tz::Tz = dsp.timezone.parse().unwrap_or(chrono_tz::UTC);
    chrono::Utc::now().with_timezone(&zone).date_naive()
}

/// The days a request covers, both included, and how it was read.
#[derive(Clone, Debug)]
pub struct Period {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub label: String,
}
impl Period {
    pub fn days(&self) -> Vec<String> {
        let mut days = vec![];
        let mut day = self.from;
        while day <= self.to {
            days.push(day.to_string());
            day += Duration::days(1);
        }
        days
    }
    pub fn first(&self) -> String {
        self.from.to_string()
    }
    pub fn last(&self) -> String {
        self.to.to_string()
    }
}

const FORMS: &str = "Write a period as today, yesterday (or last night), this week, last \
    week, this month, last month, last N days, last N weeks, a date (2026-09-28), two dates \
    (2026-09-01..2026-09-30) or an Amazon week (2026-W39). Weeks run Sunday to Saturday.";

fn date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}
fn sunday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_sunday()))
}
fn month_start(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

/// Reads a period written in words, as a date, two dates or an Amazon week.
pub fn read_period(text: &str, today: NaiveDate) -> Option<Period> {
    let lower = text.trim().to_lowercase();
    let span = |from: NaiveDate, to: NaiveDate| Period {
        from,
        to,
        label: format!("{lower} ({from} to {to})"),
    };
    let one = |day: NaiveDate| Period {
        from: day,
        to: day,
        label: format!("{lower} ({day})"),
    };
    Some(match lower.as_str() {
        "today" | "tonight" => one(today),
        // A route day ends in the evening, so last night's routes are yesterday's.
        "yesterday" | "last night" => one(today - Duration::days(1)),
        "this week" => span(sunday(today), today),
        "past week" => span(today - Duration::days(6), today),
        "last week" => {
            let start = sunday(today) - Duration::days(7);
            span(start, start + Duration::days(6))
        }
        "this month" => span(month_start(today), today),
        "last month" => {
            let end = month_start(today) - Duration::days(1);
            span(month_start(end), end)
        }
        _ => {
            let counted = |unit: &str| {
                lower
                    .strip_prefix("last ")
                    .or_else(|| lower.strip_prefix("past "))
                    .and_then(|rest| {
                        rest.strip_suffix(&format!(" {unit}s"))
                            .or_else(|| rest.strip_suffix(&format!(" {unit}")))
                    })
                    .and_then(|n| n.trim().parse::<i64>().ok())
            };
            let days = counted("day").or_else(|| counted("week").and_then(|n| n.checked_mul(7)));
            if let Some(n) = days.filter(|n| (1..=LONGEST_DAYS).contains(n)) {
                return Some(span(today - Duration::days(n - 1), today));
            }
            if let Some((a, b)) = lower.split_once("..") {
                let (from, to) = (date(a.trim())?, date(b.trim())?);
                return (from <= to).then(|| Period {
                    from,
                    to,
                    label: format!("{from} to {to}"),
                });
            }
            if let Some(day) = date(&lower) {
                return Some(Period {
                    from: day,
                    to: day,
                    label: day.to_string(),
                });
            }
            let week = lower.to_uppercase();
            let (from, to) = weeks::week_days(&week).ok()?;
            return Some(Period {
                from,
                to,
                label: format!("Amazon week {week} ({from} to {to})"),
            });
        }
    })
}

/// Bounds the daily assessments in a timecard or meal-break question.
pub fn daily_limit(period: &Period) -> std::result::Result<(), Refusal> {
    if (period.to - period.from).num_days() >= DAILY_LONGEST {
        return Err(Refusal::new(
            400,
            "period_too_long",
            format!(
                "Hours and meal breaks are read a day at a time; ask about {DAILY_LONGEST} days \
                 or fewer, or ask about routes and packages alone."
            ),
        ));
    }
    Ok(())
}

/// Refuses a period of more than one day, for an answer about one.
pub fn one_day(period: &Period) -> std::result::Result<(), Refusal> {
    if period.from == period.to {
        Ok(())
    } else {
        Err(Refusal::new(
            400,
            "one_day_only",
            "This answers one day at a time; give `date`.",
        ))
    }
}

/// The period a request asks about: `period`, `date`, or `from` with `to`; `default` when
/// it names none.
pub fn period(
    query: &Value,
    today: NaiveDate,
    default: &str,
) -> std::result::Result<Period, Refusal> {
    let (words, day, from, to) = (
        param(query, "period"),
        param(query, "date"),
        param(query, "from"),
        param(query, "to"),
    );
    let named = [
        !words.is_empty(),
        !day.is_empty(),
        !from.is_empty() || !to.is_empty(),
    ];
    if named.iter().filter(|given| **given).count() > 1 {
        return Err(Refusal::new(
            400,
            "period_conflict",
            "Give one of `period`, `date`, or `from` with `to`.",
        ));
    }
    let invalid = || Refusal::new(400, "invalid_period", FORMS);
    let read = if !from.is_empty() || !to.is_empty() {
        match (date(from), date(to)) {
            (Some(from), Some(to)) if from <= to => Period {
                from,
                to,
                label: format!("{from} to {to}"),
            },
            _ => {
                return Err(Refusal::new(
                    400,
                    "invalid_period",
                    "`from` and `to` are dates, as 2026-09-01, with `from` first.",
                ));
            }
        }
    } else {
        let text = [words, day]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or(default);
        read_period(text, today).ok_or_else(invalid)?
    };
    if (read.to - read.from).num_days() >= LONGEST_DAYS {
        return Err(Refusal::new(
            400,
            "period_too_long",
            format!("Ask about {LONGEST_DAYS} days or fewer at a time."),
        ));
    }
    Ok(read)
}

/// Someone a DSP's sources name, as the feature that tells people apart knows them: their
/// code, name and standing, and every ID a source knows them by.
#[derive(Clone, Serialize)]
pub struct Identity {
    pub code: String,
    pub name: String,
    pub status: DriverStatus,
    pub ids: Vec<Known>,
}
/// An ID a source knows a person by, and the name it gives them.
#[derive(Clone, Serialize)]
pub struct Known {
    pub source: DriverSource,
    pub id: String,
    pub name: String,
}
/// Who everyone a DSP's sources name is, given the DSP: Driver Match fills it with its codes.
pub type Identify = fn(&Store, &str) -> Result<Vec<Identity>>;
/// Everyone a DSP's sources name, as the registered identity knows them; no one without one.
fn identities(db: &Store, dsp: &str) -> Result<Vec<Identity>> {
    match registry().features.iter().find_map(|f| f.mcp.identity) {
        Some(identify) => identify(db, dsp),
        None => Ok(vec![]),
    }
}

/// A driver as an agent names them: their Driver Match code and the IDs each source knows
/// them by.
#[derive(Clone, Debug)]
pub struct Person {
    pub code: String,
    pub name: String,
    pub status: DriverStatus,
    pub paycom: Vec<String>,
    pub amazon: Vec<String>,
}
/// Everyone with a code in a DSP, and who holds each source's ID.
pub struct People {
    pub list: Vec<Person>,
    holders: HashMap<(DriverSource, String), usize>,
    /// The features whose IDs it holds only by bypassing them, as the DSP has them off.
    bypassed: Vec<AgentSource>,
}
/// Whether the key or app reads a source's IDs at the DSP, with any kind of data they name
/// drivers in, and the features it reads them from only by bypassing them: none when it reads
/// any of them from a feature switched on.
fn read_ids(access: &Access, source: DriverSource) -> (bool, Vec<AgentSource>) {
    let areas: Vec<AgentArea> = AgentArea::all().filter(|area| area.names(source)).collect();
    if areas.iter().any(|area| access.read(*area) == Read::On) {
        return (true, vec![]);
    }
    let bypassed: Vec<AgentSource> = areas
        .iter()
        .filter(|area| access.read(**area) == Read::Bypassed)
        .map(|area| area.source())
        .collect();
    (!bypassed.is_empty(), bypassed)
}
impl People {
    /// The DSP's people, from the identity Driver Match fills, kept until a source or driver
    /// decision changes, with the IDs of the sources the key or app reads there.
    pub fn load(db: &Store, state: &State, access: &Access) -> Result<Self> {
        // Driver Match deliberately keeps every collected identity while a source is off.
        // Project that durable cache through what the key or app reads at the DSP on every
        // read, so a switch or an allowance changed takes effect at once without invalidating
        // or rebuilding it.
        let dsp = access.dsp.id.as_str();
        let (paycom_on, paycom_bypassed) = read_ids(access, DriverSource::Paycom);
        let (amazon_on, amazon_bypassed) = read_ids(access, DriverSource::Amazon);
        let revision = state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        let matched: Vec<Identity> = state.read_cache.read(
            crate::server::cache::Scope::tenant(crate::server::cache::PEOPLE, dsp),
            format!("agent-people:{dsp}"),
            revision,
            || identities(db, dsp),
        )?;
        let mut list = vec![];
        let mut holders = HashMap::new();
        let (mut paycom_held, mut amazon_held) = (false, false);
        for driver in matched {
            let visible =
                |source: DriverSource| driver.ids.iter().filter(move |id| id.source == source);
            let paycom = if paycom_on {
                visible(DriverSource::Paycom).collect::<Vec<_>>()
            } else {
                vec![]
            };
            let amazon = if amazon_on {
                visible(DriverSource::Amazon).collect::<Vec<_>>()
            } else {
                vec![]
            };
            if paycom.is_empty() && amazon.is_empty() {
                continue;
            }
            paycom_held |= !paycom.is_empty();
            amazon_held |= !amazon.is_empty();
            let name = if paycom_on && amazon_on {
                driver.name.clone()
            } else {
                paycom
                    .iter()
                    .chain(&amazon)
                    .map(|id| id.name.trim())
                    .find(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        paycom
                            .first()
                            .or_else(|| amazon.first())
                            .map_or_else(|| driver.code.clone(), |id| id.id.clone())
                    })
            };
            let status = match (paycom_on, amazon_on, driver.status) {
                (true, true, status) => status,
                (true, false, DriverStatus::Former) => DriverStatus::Former,
                (true, false, DriverStatus::Office) => DriverStatus::Office,
                (true, false, _) => DriverStatus::PaycomOnly,
                (false, true, DriverStatus::Former) => DriverStatus::Former,
                (false, true, _) => DriverStatus::AmazonOnly,
                (false, false, _) => unreachable!("a person without visible IDs was omitted"),
            };
            let person = Person {
                code: driver.code.clone(),
                name,
                status,
                paycom: paycom.iter().map(|id| id.id.clone()).collect(),
                amazon: amazon.iter().map(|id| id.id.clone()).collect(),
            };
            for id in paycom.into_iter().chain(amazon) {
                holders.insert((id.source, id.id.clone()), list.len());
            }
            list.push(person);
        }
        // A source's IDs held only by bypassing its features are named wherever they are used.
        let mut bypassed: Vec<AgentSource> = vec![];
        for (held, sources) in [
            (paycom_held, paycom_bypassed),
            (amazon_held, amazon_bypassed),
        ] {
            if held {
                bypassed.extend(sources);
            }
        }
        let bypassed = AgentSource::all()
            .filter(|source| bypassed.contains(source))
            .collect();
        Ok(Self {
            list,
            holders,
            bypassed,
        })
    }
    /// Names under `bypassed` the features it holds IDs from only by bypassing them, as every
    /// answer that finds or names drivers with these people does.
    pub fn mark(&self, answer: &mut Value) {
        for source in &self.bypassed {
            access::bypassed(answer, *source);
        }
    }
    /// Who holds an ID, if anyone does yet.
    pub fn holder(&self, source: DriverSource, id: &str) -> Option<&Person> {
        self.holders
            .get(&(source, id.to_owned()))
            .map(|&index| &self.list[index])
    }
    /// The one person `wanted` names: a code, either source's ID, a full name, or a part of
    /// one only a single person has.
    pub fn find(&self, wanted: &str) -> std::result::Result<&Person, Refusal> {
        let upper = wanted.to_uppercase();
        if let Some(person) = self
            .list
            .iter()
            .find(|p| p.code == upper || p.paycom.iter().chain(&p.amazon).any(|id| id == wanted))
        {
            return Ok(person);
        }
        let key = name_key(wanted);
        if key.is_empty() {
            return Err(Refusal::new(
                400,
                "driver_required",
                "Name a driver by name, Driver Match code or ID.",
            ));
        }
        let exact: Vec<&Person> = self
            .list
            .iter()
            .filter(|p| name_key(&p.name) == key)
            .collect();
        let words: Vec<String> = wanted.split_whitespace().map(name_key).collect();
        let found: Vec<&Person> = if exact.is_empty() {
            self.list
                .iter()
                .filter(|p| {
                    let name: Vec<String> = p.name.split_whitespace().map(name_key).collect();
                    words
                        .iter()
                        .all(|word| name.iter().any(|part| part.starts_with(word.as_str())))
                })
                .collect()
        } else {
            exact
        };
        let named = |p: &&Person| format!("{} ({})", p.name, p.code);
        match found.as_slice() {
            [one] => Ok(one),
            [] => Err(Refusal::new(
                404,
                "driver_not_found",
                format!("No driver in this DSP is called “{wanted}”."),
            )),
            many => Err(Refusal::new(
                400,
                "driver_ambiguous",
                format!("{} drivers match “{wanted}”; name one by code.", many.len()),
            )
            .choices(many.iter().take(20).map(named).collect())),
        }
    }
}

/// A person as answers name them: their name, with their code when another has it too.
pub fn label(people: &People, person: &Person) -> String {
    if people.list.iter().filter(|p| p.name == person.name).count() > 1 {
        format!("{} ({})", person.name, person.code)
    } else {
        person.name.clone()
    }
}
/// The private identity and public label of a source row. Team aggregation must keep distinct
/// unmatched provider identities separate without returning those identifiers to the caller.
pub struct DriverGroup {
    pub key: String,
    pub label: String,
}
pub fn driver_group(people: &People, source: DriverSource, id: &str, name: &str) -> DriverGroup {
    match people.holder(source, id) {
        Some(person) => DriverGroup {
            key: format!("person:{}", person.code),
            label: label(people, person),
        },
        None => DriverGroup {
            key: format!("source:{}:{id}", source.as_str()),
            label: if name.trim().is_empty() {
                "Unknown driver".into()
            } else {
                format!("{} (unmatched)", name.trim())
            },
        },
    }
}
/// Who a source's row belongs to in ordinary answers. Provider IDs leave Dispatch only on an
/// explicit ID request.
pub fn who(people: &People, source: DriverSource, id: &str, name: &str) -> String {
    driver_group(people, source, id, name).label
}

#[cfg(test)]
#[path = "../../tests/backend/data/scope.rs"]
mod tests;
