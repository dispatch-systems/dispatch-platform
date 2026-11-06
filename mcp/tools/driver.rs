//! Driver lookup and the contract readers that coordinate its answer.
mod answer;
mod budget;
mod summary;

pub use answer::Found;
use answer::{
    Chat, Collected, Daily, Identity, Inspections, Roster, Route, Routes, Scorecards, Timecards,
    mirrored,
};
use budget::fit;
use summary::summary;

use crate::toolbox::{Answer, Cx, Refusal, Reply, Tool};
use chrono::{Datelike, Duration, NaiveDate};
use chrono_tz::Tz;
use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::Store,
    foundation::{clock, validate, weeks},
    manifest::people::{Everyone, Person, Source},
};
use dispatch_daily_performance_contract::{self as performance, DailyMetrics};
use dispatch_driver_profiles_contract::{self as profiles, AmazonRoster};
use dispatch_dvic_contract::{self as dvic, Asked, ShortInspections};
use dispatch_routes_contract::{self as route, DriverRoutes};
use dispatch_slack_contract::{self as slack, ChatAccounts};
use dispatch_timecard_contract as timecard;
use dispatch_weekly_scorecard_contract::{self as scorecard, DriverScorecards};
use schemars::JsonSchema;
use serde::Deserialize;

/// The days an answer covers when none are given, and at most.
const DEFAULT_DAYS: i64 = 7;
const LONGEST: i64 = 31;
/// The IDs of one source a feature is asked about at most, the fewest any takes, and how long
/// each may be.
const IDS: usize = 16;
const LONGEST_ID: usize = 128;
/// The people a name that could be several offers to choose from at most.
const CHOICES: usize = 10;
/// The short inspections read at most.
const READ: usize = 5000;

/// Whom to look up, over which days, and what of them.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lookup {
    /// The driver: a name or part of one, an ID, email address, phone number, or their code.
    pub driver: String,
    /// The first day, YYYY-MM-DD, in the DSP's own time; the range ends today unless `to` says otherwise.
    pub from: Option<String>,
    /// The last day, YYYY-MM-DD; only with `from`.
    pub to: Option<String>,
    /// The last this many days, today included, 1 to 31; the last 7 when no days are given.
    pub last_days: Option<i64>,
    /// The sections to answer; every one when left out.
    pub sections: Option<Vec<Section>>,
}
/// A section of the answer, from one feature's data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Scorecards,
    Daily,
    Routes,
    Timecards,
    Roster,
    Chat,
    Inspections,
}
impl Section {
    const ALL: [Section; 7] = [
        Section::Scorecards,
        Section::Daily,
        Section::Routes,
        Section::Timecards,
        Section::Roster,
        Section::Chat,
        Section::Inspections,
    ];
}

/// Looks up one driver, with what each feature on at the DSP knows of them.
pub struct Driver;
impl Tool for Driver {
    const NAME: &'static str = "driver";
    const TITLE: &'static str = "Driver";
    const DESCRIPTION: &'static str = "Look up one driver: who they are, their IDs and contacts, \
        and their scorecards, routes, timecards and more.";
    const FEATURES: &'static [&'static str] = &["driver_match"];
    type Input = Lookup;
    type Output = Found;
    async fn call(cx: Cx, input: Lookup) -> Answer<Reply<Found>> {
        let dsp = cx.dsp().clone();
        let span = range(&input, clock::today(zone(&dsp)))?;
        let people = cx.everyone(&dsp).await?;
        let person = find(&people, &dsp.name, &input.driver)?.clone();
        let wanted = input.sections.unwrap_or_else(|| Section::ALL.to_vec());
        let asked = wanted.clone();
        let found = cx
            .read(move |db| answer(&Readers::at(db, &dsp.id, &asked)?, db, &dsp, &person, span))
            .await?;
        let text = summary(&found, &wanted);
        Ok(Reply::new(found).text(text))
    }
}

/// The DSP's time zone.
fn zone(dsp: &Dsp) -> Tz {
    dsp.timezone.parse().unwrap_or(chrono_tz::UTC)
}

/// The days asked about, the first and the last.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    from: NaiveDate,
    to: NaiveDate,
}
impl Span {
    /// Each day, the first first, YYYY-MM-DD.
    fn each(self) -> impl Iterator<Item = String> {
        self.from
            .iter_days()
            .take_while(move |day| *day <= self.to)
            .map(|day| day.to_string())
    }
    /// The days `known` doesn't know.
    fn missing(self, known: impl Fn(&str) -> bool) -> Vec<String> {
        self.each().filter(|day| !known(day)).collect()
    }
    /// The scorecard weeks overlapping the days, Sunday to Saturday, the oldest first, as core
    /// names Amazon's weeks.
    fn weeks(self) -> Vec<String> {
        let sunday =
            |day: NaiveDate| day - Duration::days(day.weekday().num_days_from_sunday().into());
        let (first, last) = (sunday(self.from), sunday(self.to));
        // The week holding `to` is the last to end before the Sunday after it.
        let newest = weeks::last_completed_week(last + Duration::weeks(1));
        let count = usize::try_from((last - first).num_weeks()).unwrap_or_default();
        let mut named = weeks::weeks_before(&newest, count).unwrap_or_default();
        named.reverse();
        named
    }
}

/// The days `input` names: from `from` to `to` or today, or the last `last_days` or 7, each day a
/// real one, none after today, and 31 at most.
fn range(input: &Lookup, today: NaiveDate) -> std::result::Result<Span, Refusal> {
    let invalid = |message: &str| Refusal::new("invalid_input", message);
    if input.last_days.is_some() && (input.from.is_some() || input.to.is_some()) {
        return Err(invalid("Give `from` and `to`, or `last_days`, not both."));
    }
    let day = |name: &str, value: &str| {
        parse(value).ok_or_else(|| invalid(&format!("`{name}` is a day, as YYYY-MM-DD.")))
    };
    let (from, to) = if let Some(from) = &input.from {
        let from = day("from", from)?;
        let to = input.to.as_deref().map(|to| day("to", to)).transpose()?;
        (from, to.unwrap_or(today))
    } else if input.to.is_some() {
        return Err(invalid("`to` ends a range that `from` starts."));
    } else {
        let days = input.last_days.unwrap_or(DEFAULT_DAYS);
        if !(1..=LONGEST).contains(&days) {
            return Err(invalid("`last_days` is 1 to 31."));
        }
        (today - Duration::days(days - 1), today)
    };
    if from > to {
        return Err(invalid("`from` comes after `to`."));
    }
    if to > today {
        return Err(invalid(&format!("The range ends after today, {today}.")));
    }
    if (to - from).num_days() >= LONGEST {
        return Err(invalid("Ask for 31 days at most."));
    }
    Ok(Span { from, to })
}

fn parse(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

/// Whom `asked` names at the DSP `dsp`: by their code first, alone or as a choice names it, else
/// by any name, ID, email address or phone number the directory has for them. No one is refused,
/// and so is a name that could be several people, with each as a choice: "Avery Morgan (R2VX8N)".
fn find<'a>(
    people: &'a Everyone,
    dsp: &str,
    asked: &str,
) -> std::result::Result<&'a Person, Refusal> {
    let asked = asked.trim();
    if asked.is_empty() || asked.len() > 100 {
        let message = "Name the driver, in 100 characters at most.";
        return Err(Refusal::new("invalid_input", message));
    }
    let code = (asked.strip_suffix(')'))
        .and_then(|named| named.rsplit_once('('))
        .map_or(asked, |(_, code)| code.trim());
    if let Some(person) = people.person(&code.to_ascii_uppercase()) {
        return Ok(person);
    }
    let found = people.find(asked);
    match found.len() {
        0 => {
            let message = format!(
                "No one at {dsp} goes by `{asked}`. Check the spelling, or give an ID, email \
                 address or phone number."
            );
            Err(Refusal::new("driver_not_found", message))
        }
        1 => Ok(found[0]),
        many => {
            let named = |person: &&Person| format!("{} ({})", person.name, person.code);
            let choices = found.iter().take(CHOICES).map(named).collect();
            let message = if many > CHOICES {
                format!(
                    "`{asked}` could be any of {many} people, the first {CHOICES} listed. Ask \
                     which one, or for more of their name; a choice is a `driver` as it is."
                )
            } else {
                format!(
                    "`{asked}` could be any of these people. Ask which one; a choice is a \
                     `driver` as it is."
                )
            };
            Err(Refusal::new("driver_ambiguous", message).choices(choices))
        }
    }
}

/// What reads each section at the DSP: none for a section whose feature is off there, or that
/// wasn't asked for.
#[derive(Default)]
struct Readers<'a> {
    scorecards: Option<&'a dyn DriverScorecards>,
    daily: Option<&'a dyn DailyMetrics>,
    routes: Option<&'a dyn DriverRoutes>,
    timecards: Option<&'a dyn timecard::Timecards>,
    roster: Option<&'a dyn AmazonRoster>,
    chat: Option<&'a dyn ChatAccounts>,
    inspections: Option<&'a dyn ShortInspections>,
}
impl Readers<'static> {
    /// What reads each section of `wanted` whose feature is on at `dsp`.
    fn at(db: &Store, dsp: &str, wanted: &[Section]) -> Result<Self> {
        let on = |section: Section| wanted.contains(&section);
        Ok(Self {
            scorecards: scorecard::for_dsp(db, dsp)?.filter(|_| on(Section::Scorecards)),
            daily: performance::for_dsp(db, dsp)?.filter(|_| on(Section::Daily)),
            routes: route::for_dsp(db, dsp)?.filter(|_| on(Section::Routes)),
            timecards: timecard::for_dsp(db, dsp)?.filter(|_| on(Section::Timecards)),
            roster: profiles::for_dsp(db, dsp)?.filter(|_| on(Section::Roster)),
            chat: slack::for_dsp(db, dsp)?.filter(|_| on(Section::Chat)),
            inspections: dvic::for_dsp(db, dsp)?.filter(|_| on(Section::Inspections)),
        })
    }
}

/// The answer about `person` over `span`, each section read by its reader, fit to send.
fn answer(readers: &Readers, db: &Store, dsp: &Dsp, person: &Person, span: Span) -> Answer<Found> {
    let (from, to) = (span.from.to_string(), span.to.to_string());
    let at = Reading {
        db,
        dsp: &dsp.id,
        person,
        span,
        from: from.clone(),
        to: to.clone(),
        zone: zone(dsp),
    };
    let mut found = Found {
        dsp: dsp.name.clone(),
        from,
        to,
        person: Identity::from(person),
        scorecards: readers.scorecards.map(|r| at.scorecards(r)).transpose()?,
        daily: readers.daily.map(|r| at.daily(r)).transpose()?,
        routes: readers.routes.map(|r| at.routes(r)).transpose()?,
        timecards: readers.timecards.map(|r| at.timecards(r)).transpose()?,
        roster: readers.roster.map(|r| at.roster(r)).transpose()?,
        chat: readers.chat.map(|r| at.chat(r)).transpose()?,
        inspections: readers.inspections.map(|r| at.inspections(r)).transpose()?,
        trimmed: false,
    };
    fit(&mut found);
    Ok(found)
}

/// What each section is read with: the DSP, the person, and the days, as the contracts take them.
struct Reading<'a> {
    db: &'a Store,
    dsp: &'a str,
    person: &'a Person,
    span: Span,
    from: String,
    to: String,
    zone: Tz,
}
impl Reading<'_> {
    /// The person's IDs of `source` a feature is asked about, the first linked first: none it
    /// would refuse.
    fn ids(&self, source: Source) -> Vec<String> {
        let fits = |id: &&String| !id.is_empty() && id.len() <= LONGEST_ID;
        let held = self.person.ids_of(source).iter();
        held.filter(fits).take(IDS).cloned().collect()
    }

    fn scorecards(&self, reader: &dyn DriverScorecards) -> Result<Scorecards> {
        let ids = self.ids(reader.source());
        let collected = reader.collected(self.db, self.dsp)?;
        let known = collected.as_ref().map_or(&[][..], |c| &c.weeks[..]);
        let mut unknown_weeks = self.span.weeks();
        unknown_weeks.retain(|week| !known.contains(week));
        Ok(Scorecards {
            unknown_weeks,
            weeks: mirrored(reader.scorecards(self.db, self.dsp, &ids, &self.from, &self.to)?),
            packages: mirrored(reader.packages(self.db, self.dsp, &ids, &self.from, &self.to)?),
            more_packages: 0,
            collected: collected.map(|c| Collected::new(c.from, c.through)),
            ids,
        })
    }

    fn daily(&self, reader: &dyn DailyMetrics) -> Result<Daily> {
        let collected = reader.collected(self.db, self.dsp)?;
        let ids = self.ids(reader.source());
        let read = reader.days(self.db, self.dsp, &ids, &self.from, &self.to)?;
        let known = |day: &str| read.collected.iter().any(|known| known == day);
        Ok(Daily {
            collected: collected.map(|c| Collected::new(c.from, c.through)),
            unknown_days: self.span.missing(known),
            days: mirrored(read.days),
            more_days: 0,
            more_returned: 0,
            more_events: 0,
            ids,
        })
    }

    fn routes(&self, reader: &dyn DriverRoutes) -> Result<Routes> {
        let collected = reader.collected(self.db, self.dsp)?;
        let ids = self.ids(reader.source());
        let driven = reader.routes(self.db, self.dsp, &ids, &self.from, &self.to)?;
        let known = |day: &str| driven.days.iter().any(|known| known.day == day);
        let partial = driven.days.iter().filter(|day| !day.complete);
        Ok(Routes {
            collected: collected.map(|c| Collected::new(c.from, c.through)),
            unknown_days: self.span.missing(known),
            partial_days: partial.map(|day| day.day.clone()).collect(),
            routes: driven
                .routes
                .into_iter()
                .map(|it| Route::from_day(it, self.zone))
                .collect(),
            more_routes: 0,
            more_undelivered: 0,
            ids,
        })
    }

    fn timecards(&self, reader: &dyn timecard::Timecards) -> Result<Timecards> {
        let collected = reader.collected(self.db, self.dsp)?;
        let ids = self.ids(reader.source());
        let read = reader.timecards(self.db, self.dsp, &ids, &self.from, &self.to)?;
        Ok(Timecards {
            collected: collected.map(|c| Collected::new(c.from, c.through)),
            unknown_days: read.unknown,
            partial_days: read.partial,
            days: mirrored(read.days),
            more_days: 0,
            ids,
        })
    }

    fn roster(&self, reader: &dyn AmazonRoster) -> Result<Roster> {
        let ids = self.ids(reader.source());
        Ok(Roster {
            collected: reader.collected(self.db, self.dsp)?,
            associates: mirrored(reader.associates(self.db, self.dsp, &ids)?),
            ids,
        })
    }

    fn chat(&self, reader: &dyn ChatAccounts) -> Result<Chat> {
        let ids = self.ids(reader.source());
        let accounts = mirrored(reader.accounts(self.db, self.dsp, &ids)?);
        Ok(Chat { ids, accounts })
    }

    /// Their short inspections, read a page at a time as the DVIC tool reads them: more than
    /// `READ` is refused.
    fn inspections(&self, reader: &dyn ShortInspections) -> Answer<Inspections> {
        // DVIC refuses an ID of anything but letters, digits, `_`, `-` or `.`: it holds none.
        let mut ids = self.ids(reader.source());
        ids.retain(|id| validate::token(id, LONGEST_ID));
        let (from, to) = (&self.from, &self.to);
        let mut read = Vec::new();
        for id in &ids {
            let mut after = String::new();
            loop {
                let asked = Asked {
                    from,
                    to,
                    driver: Some(id),
                    after: &after,
                    limit: 500,
                };
                let page = reader.inspections(self.db, self.dsp, &asked)?;
                read.extend(page.inspections);
                if read.len() > READ {
                    let message =
                        format!("More than {READ} short inspections match. Ask for fewer days.");
                    return Err(Refusal::new("too_many", message).into());
                }
                let Some(next) = page.next else { break };
                after = next;
            }
        }
        read.sort_by(|a, b| (&b.date, &b.start_time).cmp(&(&a.date, &a.start_time)));
        let collected = reader.collected(self.db, self.dsp)?;
        let collected = collected.map(|c| Collected::new(c.from, c.through));
        let covered = |day: &str| collected.as_ref().is_some_and(|c| c.holds(day));
        Ok(Inspections {
            unknown_days: self.span.missing(covered),
            collected,
            inspections: mirrored(read),
            more_inspections: 0,
            ids,
        })
    }
}
