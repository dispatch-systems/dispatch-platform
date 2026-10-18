//! One driver at one DSP: who they are, as the directory tells people apart, with every ID,
//! email address and phone number it has for them; and over some days, what each feature on at
//! the DSP knows of them, each read through what the feature exposes, under their IDs of the
//! source it names drivers by. A section whose feature is off is null, and one whose feature
//! knows none of their IDs is empty. Each section says what it hasn't collected, which is
//! unknown rather than nothing.
use crate::toolbox::{Answer, Cx, Refusal, Reply, Tool};
use chrono::{DateTime, Datelike, Duration, NaiveDate};
use chrono_tz::Tz;
use dispatch_core::{
    Result, State,
    accounts::api::types::Dsp,
    db::Store,
    foundation::{validate, weeks},
    manifest::people::{self, Everyone, Person, Source},
};
use dispatch_daily_performance_contract::{self as performance, DailyMetrics};
use dispatch_driver_profiles_contract::{self as profiles, AmazonRoster};
use dispatch_dvic_contract::{self as dvic, Asked, ShortInspections};
use dispatch_routes_contract::{self as route, DriverRoutes};
use dispatch_slack_contract::{self as slack, ChatAccounts};
use dispatch_timecard_contract as timecard;
use dispatch_weekly_scorecard_contract::{self as scorecard, DriverScorecards};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
/// The room an answer's JSON fits in, below the server's 48 KB, with its words beside it.
const ROOM: usize = 40_000;
/// The most each long list keeps, the newest, before any is cut further to fit.
const CAPS: [(List, usize); 5] = [
    (List::Packages, 40),
    (List::Returned, 30),
    (List::Events, 30),
    (List::Undelivered, 30),
    (List::Inspections, 30),
];

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
    fn name(self) -> &'static str {
        match self {
            Section::Scorecards => "scorecards",
            Section::Daily => "daily",
            Section::Routes => "routes",
            Section::Timecards => "timecards",
            Section::Roster => "roster",
            Section::Chat => "chat",
            Section::Inspections => "inspections",
        }
    }
    /// Whether `found` has it.
    fn answered(self, found: &Found) -> bool {
        match self {
            Section::Scorecards => found.scorecards.is_some(),
            Section::Daily => found.daily.is_some(),
            Section::Routes => found.routes.is_some(),
            Section::Timecards => found.timecards.is_some(),
            Section::Roster => found.roster.is_some(),
            Section::Chat => found.chat.is_some(),
            Section::Inspections => found.inspections.is_some(),
        }
    }
}

/// Who the driver is, and what each feature on at the DSP knows of them over the days. A
/// section is null where its feature is off at the DSP or it wasn't asked for, and empty where
/// the feature knows none of their IDs.
#[derive(Serialize, JsonSchema)]
pub struct Found {
    /// The DSP searched.
    pub dsp: String,
    /// The first day, YYYY-MM-DD, in the DSP's own time.
    pub from: String,
    /// The last day, YYYY-MM-DD, in the DSP's own time.
    pub to: String,
    pub person: Identity,
    /// Their weekly scorecards.
    pub scorecards: Option<Scorecards>,
    /// Amazon's Daily Quality and Day Safety of theirs, day by day.
    pub daily: Option<Daily>,
    /// The routes they drove.
    pub routes: Option<Routes>,
    /// Their timecards, as Paycom records them.
    pub timecards: Option<Timecards>,
    /// What Amazon's roster says of them.
    pub roster: Option<Roster>,
    /// Their accounts in the DSP's chat workspaces.
    pub chat: Option<Chat>,
    /// Their DVIC pre-trip inspections shorter than the minimum.
    pub inspections: Option<Inspections>,
    /// Whether lists were cut short to fit, each counting what it left out: ask for fewer days or sections.
    pub trimmed: bool,
}
/// Who they are, as the DSP's directory tells people apart.
#[derive(Serialize, JsonSchema)]
pub struct Identity {
    /// Theirs for good: give it as `driver` to ask about them again.
    pub code: String,
    pub name: String,
    /// Every way their sources write their name, the one shown first.
    pub names: Vec<String>,
    /// driver, helper, office, or former for someone who has left.
    pub role: String,
    /// Every ID they hold, by its source, such as amazon for transporter IDs.
    pub ids: BTreeMap<String, Vec<String>>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// Whether someone confirmed who they are, rather than a match.
    pub confirmed: bool,
}
/// The first day and the last a feature has collected, YYYY-MM-DD in the DSP's own time.
#[derive(Serialize, JsonSchema)]
pub struct Collected {
    pub from: String,
    pub through: String,
}

/// Their weekly scorecards, each week Sunday to Saturday.
#[derive(Serialize, JsonSchema)]
pub struct Scorecards {
    /// The first week's Sunday and the last week's Saturday collected, or null for none yet.
    pub collected: Option<Collected>,
    /// The weeks overlapping the days that aren't collected, as Amazon names them: unknown, not missing.
    pub unknown_weeks: Vec<String>,
    /// Their scorecard for each week overlapping the days, the oldest first.
    pub weeks: Vec<Scorecard>,
    /// The packages those scorecards list for them, the oldest week first, the newest 40 at most.
    pub packages: Vec<Package>,
    /// How many more packages there are than listed.
    pub more_packages: usize,
}
/// Their scorecard for one week.
#[derive(Serialize, JsonSchema)]
pub struct Scorecard {
    /// Its week, as Amazon names it (2026-W38).
    pub week: String,
    /// Its Sunday and Saturday.
    pub from: String,
    pub through: String,
    /// Their ID it's under.
    pub id: String,
    pub overall_score: Option<f64>,
    /// As Amazon writes it, such as Platinum.
    pub overall_tier: Option<String>,
    /// The packages they delivered that week.
    pub delivered: Option<i64>,
    /// Each metric it grades, in the scorecard's order.
    pub metrics: Vec<Metric>,
}
/// A metric a scorecard grades.
#[derive(Serialize, JsonSchema)]
pub struct Metric {
    /// seatbelt, speeding, distractions, following_distance, sign_signal_violations, cdf_dpmo, ced, rts, dsb or pod.
    pub name: String,
    /// What it measured, a rate, DPMO or count, as Amazon writes it.
    pub value: Option<f64>,
    pub tier: Option<String>,
    pub score: Option<f64>,
}
/// A package a scorecard lists for them.
#[derive(Serialize, JsonSchema)]
pub struct Package {
    pub kind: PackageKind,
    /// The scorecard week it counts in.
    pub week: String,
    /// Its own day: delivered for a concession or feedback, planned for a return.
    pub date: Option<String>,
    pub tracking_id: String,
    /// Whether it counts against the scorecard, or null when Amazon doesn't say.
    pub impacting: Option<bool>,
    /// What happened, in Amazon's words.
    pub reasons: Vec<String>,
}
/// concession: reported missing or delivered badly; return: brought back to the station; feedback: the customer's.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Concession,
    Return,
    Feedback,
}

/// Amazon's Daily Quality and Day Safety of theirs, day by day.
#[derive(Serialize, JsonSchema)]
pub struct Daily {
    /// The first day collected and the last, or null for none yet.
    pub collected: Option<Collected>,
    /// The days asked that aren't collected: unknown, rather than nothing reported.
    pub unknown_days: Vec<String>,
    /// Each day Amazon reported something of theirs, by day and then ID, the oldest first.
    pub days: Vec<Day>,
    /// How many older days are left out to fit.
    pub more_days: usize,
    /// How many more returned packages there are than listed.
    pub more_returned: usize,
    /// How many more safety events there are than listed.
    pub more_events: usize,
}
/// What Amazon reported of one of their IDs on one day. A part it reported nothing of is null or empty.
#[derive(Serialize, JsonSchema)]
pub struct Day {
    pub date: String,
    /// Their ID it's reported under.
    pub id: String,
    pub quality: Option<Quality>,
    pub safety: Option<Safety>,
    pub feedback: Option<Feedback>,
    pub behaviors: Option<Behaviors>,
    pub returns: Option<Returns>,
    /// Each package they returned to the station.
    pub returned: Vec<Returned>,
    /// Each safety event, as assessed once Amazon has, else as reported live.
    pub events: Vec<Event>,
}
/// The day's Daily Quality, each number as Amazon gives it; a rate is a fraction of one.
#[derive(Serialize, JsonSchema)]
pub struct Quality {
    pub dispatched: Option<f64>,
    pub delivered: Option<f64>,
    pub delivery_success_rate: Option<f64>,
    pub completed_routes: Option<f64>,
    /// Packages delivered but not received.
    pub dnr: Option<f64>,
    /// Packages returned to the station.
    pub returned: Option<f64>,
    /// Deliveries that needed a photo, and those that had a good one.
    pub pod_opportunities: Option<f64>,
    pub pod_successes: Option<f64>,
    pub pod_success_rate: Option<f64>,
    pub pod_bypassed: Option<f64>,
    pub not_on_van: Option<f64>,
    pub transferred_out: Option<f64>,
}
/// The day's safety events, counted by kind.
#[derive(Serialize, JsonSchema)]
pub struct Safety {
    pub speeding: Option<f64>,
    pub seatbelt: Option<f64>,
    pub distraction: Option<f64>,
    pub sign_signal: Option<f64>,
    pub following_distance: Option<f64>,
}
/// The customers' feedback on the day's deliveries.
#[derive(Serialize, JsonSchema)]
pub struct Feedback {
    pub positive: Option<f64>,
    pub negative: Option<f64>,
    /// Each reason for negative feedback that isn't zero.
    pub negatives: Vec<Count>,
}
/// The day's delivery success behaviors.
#[derive(Serialize, JsonSchema)]
pub struct Behaviors {
    pub dsb: Option<f64>,
    /// Each kind of behavior that isn't zero.
    pub concerns: Vec<Count>,
}
/// A count, named by Amazon's field for it, such as never_received_delivery_cnt.
#[derive(Serialize, JsonSchema)]
pub struct Count {
    pub what: String,
    pub count: f64,
}
/// The day's packages returned to the station: in all, those they could have delivered, and those exempted.
#[derive(Serialize, JsonSchema)]
pub struct Returns {
    pub all: Option<f64>,
    pub controllable: Option<f64>,
    pub exempted: Option<f64>,
}
/// A package returned to the station.
#[derive(Serialize, JsonSchema)]
pub struct Returned {
    pub tracking_id: Option<String>,
    /// Why, as Amazon codes it, such as BUSINESS CLOSED.
    pub reason: Option<String>,
    /// Whether it counts against their delivery completion.
    pub impacting: Option<bool>,
    pub exempted: Option<bool>,
    pub exemption_reason: Option<String>,
    /// What Amazon coaches them to do about it.
    pub coaching: Option<String>,
}
/// A safety event.
#[derive(Serialize, JsonSchema)]
pub struct Event {
    /// What happened, as Amazon names it, such as Speeding.
    pub kind: Option<String>,
    pub subtype: Option<String>,
    pub severity: Option<String>,
    /// When it started, in local time, as Amazon writes it.
    pub started: Option<String>,
    /// Whether it counts against them, once assessed.
    pub impacting: Option<bool>,
    /// Whether Amazon has assessed it, rather than only reporting it live.
    pub assessed: bool,
}

/// The routes they drove.
#[derive(Serialize, JsonSchema)]
pub struct Routes {
    /// The first day collected and the last, or null for none yet.
    pub collected: Option<Collected>,
    /// The days asked that aren't collected: unknown, rather than days they didn't drive.
    pub unknown_days: Vec<String>,
    /// The days read while under way: their counts are so far, and a day without a route proves nothing.
    pub partial_days: Vec<String>,
    /// Each route they drove, the oldest first; a day may have several, and any other day collected whole they didn't drive.
    pub routes: Vec<Route>,
    /// How many older routes are left out to fit.
    pub more_routes: usize,
    /// How many more undelivered packages there are than listed.
    pub more_undelivered: usize,
}
/// A route they drove, its times HH:MM in the DSP's own time.
#[derive(Serialize, JsonSchema)]
pub struct Route {
    pub day: String,
    /// Their ID that drove it.
    pub id: String,
    pub route_code: Option<String>,
    /// How Amazon says it went, such as COMPLETE.
    pub status: Option<String>,
    /// How Amazon says it kept up, such as ON_TIME or BEHIND.
    pub progress: Option<String>,
    /// The van driven.
    pub vin: Option<String>,
    pub planned_departure: Option<String>,
    pub departed: Option<String>,
    pub first_delivery: Option<String>,
    pub last_delivery: Option<String>,
    pub ended: Option<String>,
    pub stops: i64,
    pub stops_completed: i64,
    /// The packages to drop off that were delivered, and that weren't.
    pub delivered: i64,
    pub not_delivered: i64,
    /// The packages taken off the route, as when another driver rescued them.
    pub removed: i64,
    pub break_minutes: Option<i64>,
    pub overtime_minutes: Option<i64>,
    /// Each package to drop off that wasn't delivered, in the order attempted.
    pub undelivered: Vec<Undelivered>,
}
/// A package to drop off that wasn't delivered.
#[derive(Serialize, JsonSchema)]
pub struct Undelivered {
    pub tracking_id: Option<String>,
    /// Where Amazon says it ended, such as UNDELIVERABLE.
    pub state: Option<String>,
    /// Why, as Amazon gives it, such as BUSINESS_CLOSED.
    pub reason: Option<String>,
    /// When it was attempted, HH:MM in the DSP's own time.
    pub attempted: Option<String>,
}

/// Their timecards, as Paycom records them.
#[derive(Serialize, JsonSchema)]
pub struct Timecards {
    /// The first day collected and the last, or null for none yet.
    pub collected: Option<Collected>,
    /// The days asked that no collection has reached: unknown, rather than days off.
    pub unknown_days: Vec<String>,
    /// The days read while under way: a timecard there is so far, and a day without one may not be a day off.
    pub partial_days: Vec<String>,
    /// Each day with punches or hours, by day and then ID, the oldest first; any other day neither unknown nor partial was a day off.
    pub days: Vec<Timecard>,
    /// How many older days are left out to fit.
    pub more_days: usize,
}
/// Their timecard for one day.
#[derive(Serialize, JsonSchema)]
pub struct Timecard {
    pub date: String,
    /// Their Paycom ID it's under.
    pub id: String,
    /// Paycom's word for it, such as Complete, Missing punch or No punches.
    pub status: String,
    /// The hours Paycom reports, which leave out meals and may include paid leave.
    pub hours: f64,
    /// The minutes on the clock between complete punches, meals left out; null when the punches can't say.
    pub worked_minutes: Option<i32>,
    /// Every punch, in order.
    pub punches: Vec<Punch>,
    /// Each meal break, as punched out and back in.
    pub meals: Vec<Meal>,
    /// Whether the punches need someone to look: unlabeled, unreadable or out of order.
    pub review: bool,
}
/// One punch on a timecard.
#[derive(Serialize, JsonSchema)]
pub struct Punch {
    pub kind: PunchKind,
    /// HH:MM in the DSP's own time, or null when Paycom's isn't a clock.
    pub time: Option<String>,
    /// Whether it fell on the day after the timecard's, as at the end of a night's shift.
    pub next_day: bool,
}
/// What a punch says: in or out for the day, out or back in for a meal, or unlabeled.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PunchKind {
    InDay,
    OutLunch,
    InLunch,
    OutDay,
    Unlabeled,
}
/// A meal break, as punched.
#[derive(Serialize, JsonSchema)]
pub struct Meal {
    /// When it started and ended, HH:MM; null for a punch missing.
    pub out: Option<String>,
    pub back: Option<String>,
    /// How long it was; null unless both punches are there and in order.
    pub minutes: Option<i32>,
}

/// What Amazon's roster says of them.
#[derive(Serialize, JsonSchema)]
pub struct Roster {
    /// When the roster was last collected, RFC 3339 in UTC, or null for never.
    pub collected: Option<String>,
    /// Each of their IDs a roster has listed, as the newest to list it did.
    pub associates: Vec<Associate>,
}
/// One of their IDs, as the newest roster to list it did.
#[derive(Serialize, JsonSchema)]
pub struct Associate {
    pub id: String,
    /// Whether the newest roster lists them; one it stopped listing keeps what the last to list them said.
    pub listed: bool,
    /// ACTIVE or INACTIVE, as Amazon writes it.
    pub status: String,
    /// What they may do, such as Driver or Helper.
    pub roles: Vec<String>,
    /// The vans and kinds of work they're qualified for, such as CDV or EDV.
    pub qualifications: Vec<String>,
    /// The day their driver's license expires, when Amazon has it.
    pub license_expires: Option<String>,
    /// When a roster last listed them, RFC 3339 in UTC.
    pub last_listed: String,
}

/// Their accounts in the DSP's chat workspaces.
#[derive(Serialize, JsonSchema)]
pub struct Chat {
    pub accounts: Vec<ChatAccount>,
}
/// One account in one of the DSP's workspaces.
#[derive(Serialize, JsonSchema)]
pub struct ChatAccount {
    pub workspace: String,
    /// Their member ID in the workspace, U…, as a mention (<@U…>) names them.
    pub user: String,
    /// The name they go by there, when it isn't the name on their profile.
    pub display_name: Option<String>,
    pub active: bool,
    pub guest: bool,
    /// Whether the workspace's access broke: the rest is as it last stood.
    pub broken: bool,
}

/// Their DVIC pre-trip inspections shorter than the minimum. DVIC records only those, so none
/// on a day collected means none fell short.
#[derive(Serialize, JsonSchema)]
pub struct Inspections {
    /// The first day collected and the last, or null for none yet.
    pub collected: Option<Collected>,
    /// The days asked outside what's collected: unknown, rather than none short.
    pub unknown_days: Vec<String>,
    /// Each short inspection, the newest first, 30 at most.
    pub inspections: Vec<Inspection>,
    /// How many more short inspections there are than listed.
    pub more_inspections: usize,
}
/// An inspection shorter than its minimum.
#[derive(Serialize, JsonSchema)]
pub struct Inspection {
    pub date: String,
    /// When it started, as Amazon's report gives it, with no time zone.
    pub start_time: String,
    /// Their ID it's under.
    pub id: String,
    /// CV, CDV or SV: 90 seconds is the minimum for the first two, 300 for a step van.
    pub vehicle_type: String,
    pub vin: String,
    pub duration_seconds: f64,
    pub minimum_seconds: usize,
    pub short_by_seconds: f64,
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
        let state = cx.state().clone();
        let today = today(&dsp.timezone);
        let wanted = input
            .sections
            .clone()
            .unwrap_or_else(|| Section::ALL.to_vec());
        let asked = wanted.clone();
        let found = cx
            .read(move |db| look_up(db, &state, &dsp, today, &input, &asked))
            .await?;
        let text = summary(&found, &wanted);
        Ok(Reply::new(found).text(text))
    }
}

/// The DSP's own date today, in its time zone.
fn today(timezone: &str) -> NaiveDate {
    dispatch_core::foundation::clock::today(timezone.parse().unwrap_or(chrono_tz::UTC))
}

/// The person `input` names, with each section of `wanted` whose feature is on, read at once.
fn look_up(
    db: &Store,
    state: &State,
    dsp: &Dsp,
    today: NaiveDate,
    input: &Lookup,
    wanted: &[Section],
) -> Answer<Found> {
    let span = range(input, today)?;
    let people = people::everyone(db, state, &dsp.id)?;
    let person = find(&people, &dsp.name, &input.driver)?;
    let readers = Readers::at(db, &dsp.id, wanted)?;
    let mut found = answer(&readers, db, dsp, person, span)?;
    fit(&mut found);
    Ok(found)
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
    /// The days outside what's `collected`: every one when nothing is.
    fn outside(self, collected: Option<&Collected>) -> Vec<String> {
        self.each()
            .filter(|day| {
                !collected.is_some_and(|collected| {
                    collected.from.as_str() <= day.as_str()
                        && day.as_str() <= collected.through.as_str()
                })
            })
            .collect()
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

/// Whom `asked` names at the DSP `dsp`: by their code first, else by any name, ID, email
/// address or phone number the directory has for them. No one is refused, and so is a name
/// that could be several people, with their names to choose from.
fn find<'a>(
    people: &'a Everyone,
    dsp: &str,
    asked: &str,
) -> std::result::Result<&'a Person, Refusal> {
    let asked = asked.trim();
    if asked.is_empty() || asked.len() > 100 {
        return Err(Refusal::new(
            "invalid_input",
            "Name the driver, in 100 characters at most.",
        ));
    }
    if let Some(person) = people.person(&asked.to_ascii_uppercase()) {
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
            let choices = found
                .iter()
                .take(CHOICES)
                .map(|person| format!("{} ({})", person.name, person.code))
                .collect();
            let message = if many > CHOICES {
                format!(
                    "`{asked}` could be any of {many} people, the first {CHOICES} listed. Ask \
                     which one, or for more of their name."
                )
            } else {
                format!("`{asked}` could be any of these people. Ask which one.")
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
            scorecards: if on(Section::Scorecards) {
                scorecard::for_dsp(db, dsp)?
            } else {
                None
            },
            daily: if on(Section::Daily) {
                performance::for_dsp(db, dsp)?
            } else {
                None
            },
            routes: if on(Section::Routes) {
                route::for_dsp(db, dsp)?
            } else {
                None
            },
            timecards: if on(Section::Timecards) {
                timecard::for_dsp(db, dsp)?
            } else {
                None
            },
            roster: if on(Section::Roster) {
                profiles::for_dsp(db, dsp)?
            } else {
                None
            },
            chat: if on(Section::Chat) {
                slack::for_dsp(db, dsp)?
            } else {
                None
            },
            inspections: if on(Section::Inspections) {
                dvic::for_dsp(db, dsp)?
            } else {
                None
            },
        })
    }
}

/// The answer about `person` over `span`, each section read by its reader, before it's fit.
fn answer(readers: &Readers, db: &Store, dsp: &Dsp, person: &Person, span: Span) -> Answer<Found> {
    let id = dsp.id.as_str();
    let zone: Tz = dsp.timezone.parse().unwrap_or(chrono_tz::UTC);
    Ok(Found {
        dsp: dsp.name.clone(),
        from: span.from.to_string(),
        to: span.to.to_string(),
        person: Identity::from(person),
        scorecards: (readers.scorecards)
            .map(|reader| scorecards(reader, db, id, person, span))
            .transpose()?,
        daily: (readers.daily)
            .map(|reader| daily(reader, db, id, person, span))
            .transpose()?,
        routes: (readers.routes)
            .map(|reader| routes(reader, db, id, person, span, zone))
            .transpose()?,
        timecards: (readers.timecards)
            .map(|reader| timecards(reader, db, id, person, span))
            .transpose()?,
        roster: (readers.roster)
            .map(|reader| roster(reader, db, id, person))
            .transpose()?,
        chat: (readers.chat)
            .map(|reader| chat(reader, db, id, person))
            .transpose()?,
        inspections: (readers.inspections)
            .map(|reader| inspections(reader, db, id, person, span))
            .transpose()?,
        trimmed: false,
    })
}

/// The person's IDs of `source` a feature is asked about, the first linked first: none it
/// would refuse.
fn ids(person: &Person, source: Source) -> Vec<String> {
    (person.ids_of(source).iter())
        .filter(|id| !id.is_empty() && id.len() <= LONGEST_ID)
        .take(IDS)
        .cloned()
        .collect()
}

fn scorecards(
    reader: &dyn DriverScorecards,
    db: &Store,
    dsp: &str,
    person: &Person,
    span: Span,
) -> Result<Scorecards> {
    let collected = reader.collected(db, dsp)?;
    let known = collected
        .as_ref()
        .map_or(&[][..], |collected| &collected.weeks[..]);
    let unknown_weeks = (span.weeks().into_iter())
        .filter(|week| !known.contains(week))
        .collect();
    let ids = ids(person, reader.source());
    let (from, to) = (span.from.to_string(), span.to.to_string());
    let (weeks, packages) = if ids.is_empty() {
        (vec![], vec![])
    } else {
        let weeks = reader.scorecards(db, dsp, &ids, &from, &to)?;
        (weeks, reader.packages(db, dsp, &ids, &from, &to)?)
    };
    Ok(Scorecards {
        collected: collected
            .map(|scorecard::Collected { from, through, .. }| Collected { from, through }),
        unknown_weeks,
        weeks: weeks.into_iter().map(Scorecard::from).collect(),
        packages: packages.into_iter().map(Package::from).collect(),
        more_packages: 0,
    })
}

fn daily(
    reader: &dyn DailyMetrics,
    db: &Store,
    dsp: &str,
    person: &Person,
    span: Span,
) -> Result<Daily> {
    let collected = (reader.collected(db, dsp)?)
        .map(|performance::Collected { from, through }| Collected { from, through });
    let ids = ids(person, reader.source());
    let (unknown_days, days) = if ids.is_empty() {
        (span.outside(collected.as_ref()), vec![])
    } else {
        let read = reader.days(db, dsp, &ids, &span.from.to_string(), &span.to.to_string())?;
        let unknown = span.each().filter(|day| !read.collected.contains(day));
        (unknown.collect(), read.days)
    };
    Ok(Daily {
        collected,
        unknown_days,
        days: days.into_iter().map(Day::from).collect(),
        more_days: 0,
        more_returned: 0,
        more_events: 0,
    })
}

fn routes(
    reader: &dyn DriverRoutes,
    db: &Store,
    dsp: &str,
    person: &Person,
    span: Span,
    zone: Tz,
) -> Result<Routes> {
    let collected = (reader.collected(db, dsp)?)
        .map(|route::Collected { from, through }| Collected { from, through });
    let ids = ids(person, reader.source());
    let driven = if ids.is_empty() {
        route::Driven {
            days: vec![],
            routes: vec![],
        }
    } else {
        reader.routes(db, dsp, &ids, &span.from.to_string(), &span.to.to_string())?
    };
    let unknown_days = if ids.is_empty() {
        span.outside(collected.as_ref())
    } else {
        let known = |day: &String| driven.days.iter().any(|known| known.day == *day);
        span.each().filter(|day| !known(day)).collect()
    };
    let partial_days = (driven.days.iter())
        .filter(|day| !day.complete)
        .map(|day| day.day.clone())
        .collect();
    Ok(Routes {
        collected,
        unknown_days,
        partial_days,
        routes: (driven.routes.into_iter())
            .map(|driven| route_of(driven, zone))
            .collect(),
        more_routes: 0,
        more_undelivered: 0,
    })
}

fn timecards(
    reader: &dyn timecard::Timecards,
    db: &Store,
    dsp: &str,
    person: &Person,
    span: Span,
) -> Result<Timecards> {
    let collected = (reader.collected(db, dsp)?)
        .map(|timecard::Collected { from, through }| Collected { from, through });
    let ids = ids(person, reader.source());
    let read = if ids.is_empty() {
        timecard::Days {
            days: vec![],
            unknown: span.outside(collected.as_ref()),
            partial: vec![],
        }
    } else {
        reader.timecards(db, dsp, &ids, &span.from.to_string(), &span.to.to_string())?
    };
    Ok(Timecards {
        collected,
        unknown_days: read.unknown,
        partial_days: read.partial,
        days: read.days.into_iter().map(Timecard::from).collect(),
        more_days: 0,
    })
}

fn roster(reader: &dyn AmazonRoster, db: &Store, dsp: &str, person: &Person) -> Result<Roster> {
    let ids = ids(person, reader.source());
    let associates = if ids.is_empty() {
        vec![]
    } else {
        reader.associates(db, dsp, &ids)?
    };
    Ok(Roster {
        collected: reader.collected(db, dsp)?,
        associates: associates.into_iter().map(Associate::from).collect(),
    })
}

fn chat(reader: &dyn ChatAccounts, db: &Store, dsp: &str, person: &Person) -> Result<Chat> {
    let ids = ids(person, reader.source());
    let accounts = if ids.is_empty() {
        vec![]
    } else {
        reader.accounts(db, dsp, &ids)?
    };
    Ok(Chat {
        accounts: accounts.into_iter().map(ChatAccount::from).collect(),
    })
}

fn inspections(
    reader: &dyn ShortInspections,
    db: &Store,
    dsp: &str,
    person: &Person,
    span: Span,
) -> Result<Inspections> {
    let collected = (reader.collected(db, dsp)?)
        .map(|dvic::Collected { from, through }| Collected { from, through });
    let (from, to) = (span.from.to_string(), span.to.to_string());
    let mut read = Vec::new();
    // DVIC refuses an ID of anything but letters, digits, `_`, `-` or `.`: it holds none.
    let ids = ids(person, reader.source());
    for id in ids.iter().filter(|id| validate::token(id, LONGEST_ID)) {
        let mut after = String::new();
        loop {
            let asked = Asked {
                from: &from,
                to: &to,
                driver: Some(id),
                after: &after,
                limit: 500,
            };
            let page = reader.inspections(db, dsp, &asked)?;
            read.extend(page.inspections);
            match page.next {
                Some(next) if read.len() < READ => after = next,
                _ => break,
            }
        }
    }
    read.sort_by(|a, b| {
        (b.date.as_str(), b.start_time.as_str()).cmp(&(a.date.as_str(), a.start_time.as_str()))
    });
    Ok(Inspections {
        unknown_days: span.outside(collected.as_ref()),
        collected,
        inspections: read.into_iter().map(Inspection::from).collect(),
        more_inspections: 0,
    })
}

/// A list an answer may cut short, the newest kept, counting the rest in its `more_` field.
#[derive(Clone, Copy, Debug, PartialEq)]
enum List {
    Packages,
    DailyDays,
    Returned,
    Events,
    Routes,
    Undelivered,
    TimecardDays,
    Inspections,
}
impl List {
    const ALL: [List; 8] = [
        List::Packages,
        List::DailyDays,
        List::Returned,
        List::Events,
        List::Routes,
        List::Undelivered,
        List::TimecardDays,
        List::Inspections,
    ];
    /// How many it holds in `found`, and their size as JSON.
    fn measure(self, found: &Found) -> (usize, usize) {
        let (daily, routes) = (found.daily.as_ref(), found.routes.as_ref());
        let none = (0, 0);
        match self {
            List::Packages => (found.scorecards.as_ref()).map_or(none, |s| sized(&s.packages)),
            List::DailyDays => daily.map_or(none, |daily| sized(&daily.days)),
            List::Returned => daily.map_or(none, |daily| {
                total(daily.days.iter().map(|day| sized(&day.returned)))
            }),
            List::Events => daily.map_or(none, |daily| {
                total(daily.days.iter().map(|day| sized(&day.events)))
            }),
            List::Routes => routes.map_or(none, |routes| sized(&routes.routes)),
            List::Undelivered => routes.map_or(none, |routes| {
                total(routes.routes.iter().map(|route| sized(&route.undelivered)))
            }),
            List::TimecardDays => (found.timecards.as_ref()).map_or(none, |t| sized(&t.days)),
            List::Inspections => {
                (found.inspections.as_ref()).map_or(none, |i| sized(&i.inspections))
            }
        }
    }
    /// Keeps the newest `most` of it in `found`.
    fn keep(self, found: &mut Found, most: usize) {
        match self {
            List::Packages => {
                if let Some(cards) = &mut found.scorecards {
                    cards.more_packages += cut_oldest(&mut cards.packages, most);
                }
            }
            List::DailyDays => {
                if let Some(daily) = &mut found.daily {
                    daily.more_days += cut_oldest(&mut daily.days, most);
                }
            }
            List::Returned => {
                if let Some(daily) = &mut found.daily {
                    let newest = daily.days.iter_mut().rev();
                    daily.more_returned += cut(newest.map(|day| &mut day.returned), most);
                }
            }
            List::Events => {
                if let Some(daily) = &mut found.daily {
                    let newest = daily.days.iter_mut().rev();
                    daily.more_events += cut(newest.map(|day| &mut day.events), most);
                }
            }
            List::Routes => {
                if let Some(routes) = &mut found.routes {
                    routes.more_routes += cut_oldest(&mut routes.routes, most);
                }
            }
            List::Undelivered => {
                if let Some(routes) = &mut found.routes {
                    let newest = routes.routes.iter_mut().rev();
                    routes.more_undelivered +=
                        cut(newest.map(|route| &mut route.undelivered), most);
                }
            }
            List::TimecardDays => {
                if let Some(timecards) = &mut found.timecards {
                    timecards.more_days += cut_oldest(&mut timecards.days, most);
                }
            }
            List::Inspections => {
                if let Some(short) = &mut found.inspections {
                    let newest = std::iter::once(&mut short.inspections);
                    short.more_inspections += cut(newest, most);
                }
            }
        }
    }
}

/// Cuts each long list to its cap; then, while the answer takes more than `ROOM`, halves the
/// list taking the most of it, and notes that the answer was trimmed.
fn fit(found: &mut Found) {
    for (list, most) in CAPS {
        list.keep(found, most);
    }
    while json_len(found) > ROOM {
        let longest = (List::ALL.into_iter())
            .map(|list| (list, list.measure(found)))
            .filter(|(_, (count, _))| *count > 0)
            .max_by_key(|(_, (_, bytes))| *bytes);
        let Some((list, (count, _))) = longest else {
            break;
        };
        list.keep(found, count / 2);
        found.trimmed = true;
    }
}
fn json_len<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(0, |json| json.len())
}
/// A list's length, and its size as JSON.
fn sized<T: Serialize>(items: &[T]) -> (usize, usize) {
    (items.len(), json_len(items))
}
fn total(each: impl Iterator<Item = (usize, usize)>) -> (usize, usize) {
    each.fold((0, 0), |(count, bytes), (more, size)| {
        (count + more, bytes + size)
    })
}
/// Keeps the last `most` of a list kept oldest first, answering how many it cut.
fn cut_oldest<T>(list: &mut Vec<T>, most: usize) -> usize {
    let gone = list.len().saturating_sub(most);
    list.drain(..gone);
    gone
}
/// Keeps the first `most` across `lists`, in their order, answering how many it cut.
fn cut<'a, T: 'a>(lists: impl Iterator<Item = &'a mut Vec<T>>, most: usize) -> usize {
    let (mut left, mut gone) = (most, 0);
    for list in lists {
        let kept = list.len().min(left);
        gone += list.len() - kept;
        list.truncate(kept);
        left -= kept;
    }
    gone
}

/// An epoch-milliseconds time as HH:MM in the DSP's own time.
fn clock(at: Option<i64>, zone: Tz) -> Option<String> {
    let at = DateTime::from_timestamp_millis(at?)?;
    Some(at.with_timezone(&zone).format("%H:%M").to_string())
}

impl From<&Person> for Identity {
    fn from(person: &Person) -> Self {
        Self {
            code: person.code.clone(),
            name: person.name.clone(),
            names: person.names.clone(),
            role: person.role.as_str().to_owned(),
            ids: (person.ids.iter())
                .map(|(source, ids)| (source.as_str().to_owned(), ids.clone()))
                .collect(),
            emails: person.emails.clone(),
            phones: person.phones.clone(),
            confirmed: person.confirmed,
        }
    }
}
impl From<scorecard::Scorecard> for Scorecard {
    fn from(card: scorecard::Scorecard) -> Self {
        Self {
            week: card.week,
            from: card.from,
            through: card.through,
            id: card.driver_id,
            overall_score: card.overall_score,
            overall_tier: card.overall_tier,
            delivered: card.delivered,
            metrics: (card.metrics.into_iter())
                .map(|metric| Metric {
                    name: metric.name,
                    value: metric.value,
                    tier: metric.tier,
                    score: metric.score,
                })
                .collect(),
        }
    }
}
impl From<scorecard::Package> for Package {
    fn from(package: scorecard::Package) -> Self {
        Self {
            kind: match package.kind {
                scorecard::PackageKind::Concession => PackageKind::Concession,
                scorecard::PackageKind::Return => PackageKind::Return,
                scorecard::PackageKind::Feedback => PackageKind::Feedback,
            },
            week: package.week,
            date: package.date,
            tracking_id: package.tracking_id,
            impacting: package.impacting,
            reasons: package.reasons,
        }
    }
}
impl From<performance::Day> for Day {
    fn from(day: performance::Day) -> Self {
        Self {
            date: day.date,
            id: day.id,
            quality: day.quality.map(|quality| Quality {
                dispatched: quality.dispatched,
                delivered: quality.delivered,
                delivery_success_rate: quality.delivery_success_rate,
                completed_routes: quality.completed_routes,
                dnr: quality.dnr,
                returned: quality.returned,
                pod_opportunities: quality.pod_opportunities,
                pod_successes: quality.pod_successes,
                pod_success_rate: quality.pod_success_rate,
                pod_bypassed: quality.pod_bypassed,
                not_on_van: quality.not_on_van,
                transferred_out: quality.transferred_out,
            }),
            safety: day.safety.map(|safety| Safety {
                speeding: safety.speeding,
                seatbelt: safety.seatbelt,
                distraction: safety.distraction,
                sign_signal: safety.sign_signal,
                following_distance: safety.following_distance,
            }),
            feedback: day.feedback.map(|feedback| Feedback {
                positive: feedback.positive,
                negative: feedback.negative,
                negatives: feedback.negatives.into_iter().map(Count::from).collect(),
            }),
            behaviors: day.behaviors.map(|behaviors| Behaviors {
                dsb: behaviors.dsb,
                concerns: behaviors.concerns.into_iter().map(Count::from).collect(),
            }),
            returns: day.returns.map(|returns| Returns {
                all: returns.all,
                controllable: returns.controllable,
                exempted: returns.exempted,
            }),
            returned: (day.returned.into_iter())
                .map(|returned| Returned {
                    tracking_id: returned.tracking_id,
                    reason: returned.reason,
                    impacting: returned.impacting,
                    exempted: returned.exempted,
                    exemption_reason: returned.exemption_reason,
                    coaching: returned.coaching,
                })
                .collect(),
            events: (day.events.into_iter())
                .map(|event| Event {
                    kind: event.kind,
                    subtype: event.subtype,
                    severity: event.severity,
                    started: event.started,
                    impacting: event.impacting,
                    assessed: event.assessed,
                })
                .collect(),
        }
    }
}
impl From<performance::Count> for Count {
    fn from(count: performance::Count) -> Self {
        Self {
            what: count.what,
            count: count.count,
        }
    }
}
/// A route as the answer gives it, its times on the DSP's clock.
fn route_of(driven: route::RouteDay, zone: Tz) -> Route {
    Route {
        day: driven.day,
        id: driven.driver_id,
        route_code: driven.route_code,
        status: driven.status,
        progress: driven.progress,
        vin: driven.vin,
        planned_departure: clock(driven.planned_departure_at, zone),
        departed: clock(driven.departed_at, zone),
        first_delivery: clock(driven.first_delivery_at, zone),
        last_delivery: clock(driven.last_delivery_at, zone),
        ended: clock(driven.session_end_at, zone),
        stops: driven.stops,
        stops_completed: driven.stops_completed,
        delivered: driven.delivered,
        not_delivered: driven.not_delivered,
        removed: driven.removed,
        break_minutes: driven.breaks_secs.map(|seconds| seconds / 60),
        overtime_minutes: driven.overtime_secs.map(|seconds| seconds / 60),
        undelivered: (driven.undelivered.into_iter())
            .map(|package| Undelivered {
                tracking_id: package.tracking_id,
                state: package.state,
                reason: package.reason,
                attempted: clock(package.attempted_at, zone),
            })
            .collect(),
    }
}
impl From<timecard::Day> for Timecard {
    fn from(day: timecard::Day) -> Self {
        Self {
            date: day.date,
            id: day.id,
            status: day.status,
            hours: day.hours,
            worked_minutes: day.worked_minutes,
            punches: (day.punches.into_iter())
                .map(|punch| Punch {
                    kind: match punch.kind {
                        timecard::PunchKind::InDay => PunchKind::InDay,
                        timecard::PunchKind::OutLunch => PunchKind::OutLunch,
                        timecard::PunchKind::InLunch => PunchKind::InLunch,
                        timecard::PunchKind::OutDay => PunchKind::OutDay,
                        timecard::PunchKind::Unlabeled => PunchKind::Unlabeled,
                    },
                    time: punch.time,
                    next_day: punch.next_day,
                })
                .collect(),
            meals: (day.meals.into_iter())
                .map(|meal| Meal {
                    out: meal.out,
                    back: meal.back,
                    minutes: meal.minutes,
                })
                .collect(),
            review: day.review,
        }
    }
}
impl From<profiles::Associate> for Associate {
    fn from(associate: profiles::Associate) -> Self {
        Self {
            id: associate.id,
            listed: associate.listed,
            status: associate.status,
            roles: associate.roles,
            qualifications: associate.qualifications,
            license_expires: associate.license_expires,
            last_listed: associate.last_listed,
        }
    }
}
impl From<slack::ChatAccount> for ChatAccount {
    fn from(account: slack::ChatAccount) -> Self {
        Self {
            workspace: account.workspace,
            user: account.user,
            display_name: account.display_name,
            active: account.active,
            guest: account.guest,
            broken: account.broken,
        }
    }
}
impl From<dvic::Inspection> for Inspection {
    fn from(row: dvic::Inspection) -> Self {
        Self {
            date: row.date,
            start_time: row.start_time,
            id: row.driver_id,
            vehicle_type: row.vehicle_type,
            vin: row.vin,
            duration_seconds: row.duration_seconds,
            minimum_seconds: row.minimum_seconds,
            short_by_seconds: row.short_by_seconds,
        }
    }
}

/// A day as people read it: Oct 4.
fn day(date: &str) -> String {
    parse(date).map_or_else(|| date.to_owned(), |date| date.format("%b %-d").to_string())
}
/// So many of something, in words: no routes, 1 route, 5 routes.
fn count(count: usize, one: &str, many: &str) -> String {
    match count {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        count => format!("{count} {many}"),
    }
}
/// What the answer says, in a sentence or a few, for the agent to read out: who, what each
/// section found, what isn't known, which sections asked for are off, and whether lists were
/// cut short.
fn summary(found: &Found, wanted: &[Section]) -> String {
    let person = &found.person;
    let days = if found.from == found.to {
        day(&found.from)
    } else {
        format!("{} to {}", day(&found.from), day(&found.to))
    };
    let at = match person.role.as_str() {
        "former" => format!("who has left {}", found.dsp),
        "office" => format!("office staff at {}", found.dsp),
        role => format!("a {role} at {}", found.dsp),
    };
    let mut told = Vec::new();
    if let Some(cards) = &found.scorecards {
        told.push(count(
            cards.weeks.len(),
            "weekly scorecard",
            "weekly scorecards",
        ));
        let packages = cards.packages.len() + cards.more_packages;
        if packages > 0 {
            told.push(count(packages, "scorecard package", "scorecard packages"));
        }
    }
    if let Some(daily) = &found.daily {
        let mut dates: Vec<&str> = daily.days.iter().map(|day| day.date.as_str()).collect();
        dates.dedup();
        let days = dates.len() + daily.more_days;
        told.push(count(days, "day of daily metrics", "days of daily metrics"));
    }
    if let Some(routes) = &found.routes {
        told.push(count(
            routes.routes.len() + routes.more_routes,
            "route",
            "routes",
        ));
    }
    if let Some(timecards) = &found.timecards {
        let days = timecards.days.len() + timecards.more_days;
        told.push(count(days, "timecard", "timecards"));
    }
    if let Some(roster) = &found.roster {
        let listed = roster.associates.iter().find(|associate| associate.listed);
        told.push(match (listed, &roster.collected) {
            (Some(associate), _) => format!("on Amazon's roster as {}", associate.status),
            (None, None) => "Amazon's roster not collected yet".to_owned(),
            (None, Some(_)) if roster.associates.is_empty() => "not on Amazon's roster".to_owned(),
            (None, Some(_)) => "no longer on Amazon's roster".to_owned(),
        });
    }
    if let Some(chat) = &found.chat {
        told.push(count(chat.accounts.len(), "chat account", "chat accounts"));
    }
    if let Some(short) = &found.inspections {
        let inspections = short.inspections.len() + short.more_inspections;
        told.push(count(
            inspections,
            "short DVIC inspection",
            "short DVIC inspections",
        ));
    }
    let mut text = format!("{} ({}), {at}, {days}", person.name, person.code);
    if !told.is_empty() {
        text.push_str(": ");
        text.push_str(&told.join(", "));
    }
    text.push('.');
    let unknown: Vec<&str> = [
        (
            "scorecards",
            (found.scorecards.as_ref()).is_some_and(|s| !s.unknown_weeks.is_empty()),
        ),
        (
            "daily",
            (found.daily.as_ref()).is_some_and(|d| !d.unknown_days.is_empty()),
        ),
        (
            "routes",
            (found.routes.as_ref()).is_some_and(|r| !r.unknown_days.is_empty()),
        ),
        (
            "timecards",
            (found.timecards.as_ref()).is_some_and(|t| !t.unknown_days.is_empty()),
        ),
        (
            "inspections",
            (found.inspections.as_ref()).is_some_and(|i| !i.unknown_days.is_empty()),
        ),
    ]
    .into_iter()
    .filter(|(_, unknown)| *unknown)
    .map(|(name, _)| name)
    .collect();
    if !unknown.is_empty() {
        text.push_str(&format!(
            " Some days aren't collected in {}, so they're unknown.",
            unknown.join(", ")
        ));
    }
    let partial: Vec<&str> = [
        (
            "routes",
            (found.routes.as_ref()).is_some_and(|r| !r.partial_days.is_empty()),
        ),
        (
            "timecards",
            (found.timecards.as_ref()).is_some_and(|t| !t.partial_days.is_empty()),
        ),
    ]
    .into_iter()
    .filter(|(_, partial)| *partial)
    .map(|(name, _)| name)
    .collect();
    if !partial.is_empty() {
        text.push_str(&format!(
            " Some days in {} were read while under way, so they show only what was known then.",
            partial.join(" and ")
        ));
    }
    let mut off: Vec<&str> = (wanted.iter())
        .filter(|section| !section.answered(found))
        .map(|section| section.name())
        .collect();
    off.sort_unstable();
    off.dedup();
    if !off.is_empty() {
        text.push_str(&format!(" Off at {}: {}.", found.dsp, off.join(", ")));
    }
    if found.trimmed {
        text.push_str(
            " Lists were cut short to fit: ask for fewer days or sections to see the rest.",
        );
    }
    text
}

#[cfg(test)]
#[path = "../tests/backend/tools/driver.rs"]
mod tests;
