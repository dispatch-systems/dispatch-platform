//! One driver at one DSP: who they are, as the directory tells people apart, with every ID,
//! email address and phone number it has for them; and over some days, what each feature on at
//! the DSP knows of them, each read through what the feature exposes, under their IDs of the
//! source it names drivers by. A section whose feature is off is null. Each section names the
//! IDs it was read under, and says what it hasn't collected, which is unknown rather than
//! nothing.
use crate::toolbox::{Answer, Cx, Refusal, Reply, Tool};
use chrono::{DateTime, Datelike, Duration, NaiveDate};
use chrono_tz::Tz;
use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::Store,
    foundation::{clock, validate, weeks},
    manifest::people::{Everyone, Person, PersonTitle, Source},
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
    /// What its section of `found` tells, or none where the section is null.
    fn told(self, found: &Found) -> Option<Told> {
        Some(match self {
            Section::Scorecards => {
                let s = found.scorecards.as_ref()?;
                let mut words = count(s.weeks.len(), "weekly scorecard", "weekly scorecards");
                let packages = s.packages.len() + s.more_packages;
                if packages > 0 {
                    words += ", ";
                    words += &count(packages, "scorecard package", "scorecard packages");
                }
                Told::new(words, &s.ids, &s.unknown_weeks, &[])
            }
            Section::Daily => {
                let d = found.daily.as_ref()?;
                let mut dates: Vec<&str> = d.days.iter().map(|day| day.date.as_str()).collect();
                dates.dedup();
                let days = dates.len() + d.more_days;
                let words = count(days, "day of daily metrics", "days of daily metrics");
                Told::new(words, &d.ids, &d.unknown_days, &[])
            }
            Section::Routes => {
                let r = found.routes.as_ref()?;
                let words = count(r.routes.len() + r.more_routes, "route", "routes");
                Told::new(words, &r.ids, &r.unknown_days, &r.partial_days)
            }
            Section::Timecards => {
                let t = found.timecards.as_ref()?;
                let words = count(t.days.len() + t.more_days, "timecard", "timecards");
                Told::new(words, &t.ids, &t.unknown_days, &t.partial_days)
            }
            Section::Roster => {
                let r = found.roster.as_ref()?;
                let words = match r.associates.iter().find(|associate| associate.listed) {
                    Some(associate) => format!("on Amazon's roster as {}", associate.status),
                    None if r.collected.is_none() => "Amazon's roster not collected yet".into(),
                    None if r.associates.is_empty() => "not on Amazon's roster".into(),
                    None => "no longer on Amazon's roster".into(),
                };
                Told::new(words, &r.ids, &[], &[])
            }
            Section::Chat => {
                let c = found.chat.as_ref()?;
                let words = count(c.accounts.len(), "chat account", "chat accounts");
                Told::new(words, &c.ids, &[], &[])
            }
            Section::Inspections => {
                let i = found.inspections.as_ref()?;
                let short = i.inspections.len() + i.more_inspections;
                let words = count(short, "short DVIC inspection", "short DVIC inspections");
                Told::new(words, &i.ids, &i.unknown_days, &[])
            }
        })
    }
}
/// What a section tells the summary: what it found, in words, and whether it knows none of their
/// IDs, has days unknown, or has days read while under way.
struct Told {
    words: String,
    no_ids: bool,
    unknown: bool,
    partial: bool,
}
impl Told {
    fn new(words: String, ids: &[String], unknown: &[String], partial: &[String]) -> Self {
        Self {
            words,
            no_ids: ids.is_empty(),
            unknown: !unknown.is_empty(),
            partial: !partial.is_empty(),
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
    /// Whether any list was cut short, each counting what it left out: ask for fewer days or sections to see the rest.
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
    /// driver, helper, office for management, or former for someone who has left.
    pub role: String,
    /// What they do in management, when someone gave them a title: owner, ops_manager, fleet_manager, dispatch, hr_manager or manager.
    pub title: Option<String>,
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
impl Collected {
    fn new(from: String, through: String) -> Self {
        Self { from, through }
    }
    fn holds(&self, day: &str) -> bool {
        self.from.as_str() <= day && day <= self.through.as_str()
    }
}

/// Their weekly scorecards, each week Sunday to Saturday.
#[derive(Serialize, JsonSchema)]
pub struct Scorecards {
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
/// A route they drove, its times HH:MM in the DSP's own time, with the date before one on a later day.
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
    /// When it was attempted, as the route's times are.
    pub attempted: Option<String>,
}

/// Their timecards, as Paycom records them.
#[derive(Serialize, JsonSchema)]
pub struct Timecards {
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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

/// Their DVIC pre-trip inspections shorter than the minimum: DVIC records only these, so none on a day collected is none short.
#[derive(Serialize, JsonSchema)]
pub struct Inspections {
    /// The IDs of theirs it was read under; with none, it knows nothing of them.
    pub ids: Vec<String>,
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
            routes: driven.routes.into_iter().map(|it| self.route(it)).collect(),
            more_routes: 0,
            more_undelivered: 0,
            ids,
        })
    }

    /// A route as the answer gives it, its times on the DSP's clock.
    fn route(&self, it: route::RouteDay) -> Route {
        let at = |time: Option<i64>| local_time(time, self.zone, &it.day);
        let undelivered = it.undelivered.into_iter().map(|package| Undelivered {
            attempted: at(package.attempted_at),
            tracking_id: package.tracking_id,
            state: package.state,
            reason: package.reason,
        });
        Route {
            undelivered: undelivered.collect(),
            planned_departure: at(it.planned_departure_at),
            departed: at(it.departed_at),
            first_delivery: at(it.first_delivery_at),
            last_delivery: at(it.last_delivery_at),
            ended: at(it.session_end_at),
            day: it.day.clone(),
            id: it.driver_id,
            route_code: it.route_code,
            status: it.status,
            progress: it.progress,
            vin: it.vin,
            stops: it.stops,
            stops_completed: it.stops_completed,
            delivered: it.delivered,
            not_delivered: it.not_delivered,
            removed: it.removed,
            break_minutes: it.breaks_secs.map(|seconds| seconds / 60),
            overtime_minutes: it.overtime_secs.map(|seconds| seconds / 60),
        }
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

/// A list an answer may cut short.
#[derive(Clone, Copy)]
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
    fn size(self, found: &mut Found) -> (usize, usize) {
        self.of(found).map_or((0, 0), |list| list.size())
    }
    /// Keeps its newest `most` in `found`, answering how many it cut.
    fn keep(self, found: &mut Found, most: usize) -> usize {
        self.of(found).map_or(0, |mut list| list.keep(most))
    }
    /// The list in `found`, where its section is.
    fn of(self, found: &mut Found) -> Option<Box<dyn Trim + '_>> {
        Some(match self {
            List::Packages => {
                let s = found.scorecards.as_mut()?;
                cut([&mut s.packages], true, &mut s.more_packages)
            }
            List::DailyDays => {
                let d = found.daily.as_mut()?;
                cut([&mut d.days], true, &mut d.more_days)
            }
            List::Returned => {
                let d = found.daily.as_mut()?;
                let newest = d.days.iter_mut().rev().map(|day| &mut day.returned);
                cut(newest, false, &mut d.more_returned)
            }
            List::Events => {
                let d = found.daily.as_mut()?;
                let newest = d.days.iter_mut().rev().map(|day| &mut day.events);
                cut(newest, false, &mut d.more_events)
            }
            List::Routes => {
                let r = found.routes.as_mut()?;
                cut([&mut r.routes], true, &mut r.more_routes)
            }
            List::Undelivered => {
                let r = found.routes.as_mut()?;
                let newest = r.routes.iter_mut().rev().map(|it| &mut it.undelivered);
                cut(newest, false, &mut r.more_undelivered)
            }
            List::TimecardDays => {
                let t = found.timecards.as_mut()?;
                cut([&mut t.days], true, &mut t.more_days)
            }
            List::Inspections => {
                let i = found.inspections.as_mut()?;
                cut([&mut i.inspections], false, &mut i.more_inspections)
            }
        })
    }
}

/// A list of any kind, to measure and cut short.
trait Trim {
    /// How many it holds, and their size as JSON.
    fn size(&self) -> (usize, usize);
    /// Keeps its newest `most`, counting the rest, and answers how many it cut.
    fn keep(&mut self, most: usize) -> usize;
}
/// A list as its parts: one listed oldest first, cut `from_front`; or parts listed newest first,
/// each cut from its end. `more` counts what it leaves out.
struct Cut<'a, T> {
    parts: Vec<&'a mut Vec<T>>,
    from_front: bool,
    more: &'a mut usize,
}
fn cut<'a, T: Serialize + 'a>(
    parts: impl IntoIterator<Item = &'a mut Vec<T>>,
    from_front: bool,
    more: &'a mut usize,
) -> Box<dyn Trim + 'a> {
    let parts = parts.into_iter().collect();
    Box::new(Cut {
        parts,
        from_front,
        more,
    })
}
impl<T: Serialize> Trim for Cut<'_, T> {
    fn size(&self) -> (usize, usize) {
        let count = self.parts.iter().map(|part| part.len()).sum();
        (count, self.parts.iter().map(json_len).sum())
    }
    fn keep(&mut self, most: usize) -> usize {
        let (mut left, mut cut) = (most, 0);
        for part in &mut self.parts {
            let kept = part.len().min(left);
            let gone = part.len() - kept;
            if self.from_front {
                part.drain(..gone);
            } else {
                part.truncate(kept);
            }
            (left, cut) = (left - kept, cut + gone);
        }
        *self.more += cut;
        cut
    }
}

/// Cuts each long list to its cap; then, while the answer takes more than `ROOM`, halves the list
/// taking the most of it. Any cut notes that the answer was trimmed.
fn fit(found: &mut Found) {
    for (list, most) in CAPS {
        let cut = list.keep(found, most);
        found.trimmed |= cut > 0;
    }
    while json_len(found) > ROOM {
        let sizes = List::ALL.map(|list| (list, list.size(found)));
        let held = sizes.into_iter().filter(|(_, (count, _))| *count > 0);
        let Some((list, (count, _))) = held.max_by_key(|(_, (_, bytes))| *bytes) else {
            break;
        };
        list.keep(found, count / 2);
        found.trimmed = true;
    }
}
fn json_len<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(0, |json| json.len())
}

/// An epoch-milliseconds time as HH:MM in the DSP's own time, with its date before it when it
/// isn't on `day`, as after midnight.
fn local_time(at: Option<i64>, zone: Tz, day: &str) -> Option<String> {
    let at = DateTime::from_timestamp_millis(at?)?.with_timezone(&zone);
    let same = at.date_naive().to_string() == day;
    let format = if same { "%H:%M" } else { "%Y-%m-%d %H:%M" };
    Some(at.format(format).to_string())
}

/// Each of a contract's values, as the answer mirrors it.
fn mirrored<A, B: From<A>>(values: Vec<A>) -> Vec<B> {
    values.into_iter().map(B::from).collect()
}
impl From<scorecard::Scorecard> for Scorecard {
    fn from(it: scorecard::Scorecard) -> Self {
        Self {
            week: it.week,
            from: it.from,
            through: it.through,
            id: it.driver_id,
            overall_score: it.overall_score,
            overall_tier: it.overall_tier,
            delivered: it.delivered,
            metrics: mirrored(it.metrics),
        }
    }
}
impl From<scorecard::Metric> for Metric {
    fn from(it: scorecard::Metric) -> Self {
        Self {
            name: it.name,
            value: it.value,
            tier: it.tier,
            score: it.score,
        }
    }
}
impl From<scorecard::Package> for Package {
    fn from(it: scorecard::Package) -> Self {
        Self {
            kind: it.kind.into(),
            week: it.week,
            date: it.date,
            tracking_id: it.tracking_id,
            impacting: it.impacting,
            reasons: it.reasons,
        }
    }
}
impl From<scorecard::PackageKind> for PackageKind {
    fn from(kind: scorecard::PackageKind) -> Self {
        match kind {
            scorecard::PackageKind::Concession => Self::Concession,
            scorecard::PackageKind::Return => Self::Return,
            scorecard::PackageKind::Feedback => Self::Feedback,
        }
    }
}
impl From<performance::Day> for Day {
    fn from(it: performance::Day) -> Self {
        Self {
            date: it.date,
            id: it.id,
            quality: it.quality.map(Quality::from),
            safety: it.safety.map(Safety::from),
            feedback: it.feedback.map(Feedback::from),
            behaviors: it.behaviors.map(Behaviors::from),
            returns: it.returns.map(Returns::from),
            returned: mirrored(it.returned),
            events: mirrored(it.events),
        }
    }
}
impl From<performance::Quality> for Quality {
    fn from(it: performance::Quality) -> Self {
        Self {
            dispatched: it.dispatched,
            delivered: it.delivered,
            delivery_success_rate: it.delivery_success_rate,
            completed_routes: it.completed_routes,
            dnr: it.dnr,
            returned: it.returned,
            pod_opportunities: it.pod_opportunities,
            pod_successes: it.pod_successes,
            pod_success_rate: it.pod_success_rate,
            pod_bypassed: it.pod_bypassed,
            not_on_van: it.not_on_van,
            transferred_out: it.transferred_out,
        }
    }
}
impl From<performance::Safety> for Safety {
    fn from(it: performance::Safety) -> Self {
        Self {
            speeding: it.speeding,
            seatbelt: it.seatbelt,
            distraction: it.distraction,
            sign_signal: it.sign_signal,
            following_distance: it.following_distance,
        }
    }
}
impl From<performance::Feedback> for Feedback {
    fn from(it: performance::Feedback) -> Self {
        Self {
            positive: it.positive,
            negative: it.negative,
            negatives: mirrored(it.negatives),
        }
    }
}
impl From<performance::Behaviors> for Behaviors {
    fn from(it: performance::Behaviors) -> Self {
        Self {
            dsb: it.dsb,
            concerns: mirrored(it.concerns),
        }
    }
}
impl From<performance::Count> for Count {
    fn from(it: performance::Count) -> Self {
        Self {
            what: it.what,
            count: it.count,
        }
    }
}
impl From<performance::Returns> for Returns {
    fn from(it: performance::Returns) -> Self {
        Self {
            all: it.all,
            controllable: it.controllable,
            exempted: it.exempted,
        }
    }
}
impl From<performance::Returned> for Returned {
    fn from(it: performance::Returned) -> Self {
        Self {
            tracking_id: it.tracking_id,
            reason: it.reason,
            impacting: it.impacting,
            exempted: it.exempted,
            exemption_reason: it.exemption_reason,
            coaching: it.coaching,
        }
    }
}
impl From<performance::SafetyEvent> for Event {
    fn from(it: performance::SafetyEvent) -> Self {
        Self {
            kind: it.kind,
            subtype: it.subtype,
            severity: it.severity,
            started: it.started,
            impacting: it.impacting,
            assessed: it.assessed,
        }
    }
}
impl From<timecard::Day> for Timecard {
    fn from(it: timecard::Day) -> Self {
        Self {
            date: it.date,
            id: it.id,
            status: it.status,
            hours: it.hours,
            worked_minutes: it.worked_minutes,
            punches: mirrored(it.punches),
            meals: mirrored(it.meals),
            review: it.review,
        }
    }
}
impl From<timecard::Punch> for Punch {
    fn from(it: timecard::Punch) -> Self {
        Self {
            kind: it.kind.into(),
            time: it.time,
            next_day: it.next_day,
        }
    }
}
impl From<timecard::PunchKind> for PunchKind {
    fn from(kind: timecard::PunchKind) -> Self {
        match kind {
            timecard::PunchKind::InDay => Self::InDay,
            timecard::PunchKind::OutLunch => Self::OutLunch,
            timecard::PunchKind::InLunch => Self::InLunch,
            timecard::PunchKind::OutDay => Self::OutDay,
            timecard::PunchKind::Unlabeled => Self::Unlabeled,
        }
    }
}
impl From<timecard::Meal> for Meal {
    fn from(it: timecard::Meal) -> Self {
        Self {
            out: it.out,
            back: it.back,
            minutes: it.minutes,
        }
    }
}
impl From<profiles::Associate> for Associate {
    fn from(it: profiles::Associate) -> Self {
        Self {
            id: it.id,
            listed: it.listed,
            status: it.status,
            roles: it.roles,
            qualifications: it.qualifications,
            license_expires: it.license_expires,
            last_listed: it.last_listed,
        }
    }
}
impl From<slack::ChatAccount> for ChatAccount {
    fn from(it: slack::ChatAccount) -> Self {
        Self {
            workspace: it.workspace,
            user: it.user,
            display_name: it.display_name,
            active: it.active,
            guest: it.guest,
            broken: it.broken,
        }
    }
}
impl From<dvic::Inspection> for Inspection {
    fn from(it: dvic::Inspection) -> Self {
        Self {
            date: it.date,
            start_time: it.start_time,
            id: it.driver_id,
            vehicle_type: it.vehicle_type,
            vin: it.vin,
            duration_seconds: it.duration_seconds,
            minimum_seconds: it.minimum_seconds,
            short_by_seconds: it.short_by_seconds,
        }
    }
}
impl From<&Person> for Identity {
    fn from(person: &Person) -> Self {
        Self {
            code: person.code.clone(),
            name: person.name.clone(),
            names: person.names.clone(),
            role: person.role.as_str().to_owned(),
            title: person.title.map(|title| title.as_str().to_owned()),
            ids: (person.ids.iter())
                .map(|(source, ids)| (source.as_str().to_owned(), ids.clone()))
                .collect(),
            emails: person.emails.clone(),
            phones: person.phones.clone(),
            confirmed: person.confirmed,
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
/// section found and doesn't know, which sections asked for are off, and whether lists were cut
/// short.
fn summary(found: &Found, wanted: &[Section]) -> String {
    let (mut told, mut no_ids, mut unknown, mut partial, mut off) =
        (vec![], vec![], vec![], vec![], vec![]);
    for section in Section::ALL {
        let name = section.name();
        let Some(section_told) = section.told(found) else {
            if wanted.contains(&section) {
                off.push(name);
            }
            continue;
        };
        told.push(section_told.words);
        if section_told.no_ids {
            no_ids.push(name);
        }
        if section_told.unknown {
            unknown.push(name);
        }
        if section_told.partial {
            partial.push(name);
        }
    }
    let person = &found.person;
    let title = person.title.as_deref().and_then(PersonTitle::parse);
    let at = match (person.role.as_str(), title) {
        ("former", _) => format!("who has left {}", found.dsp),
        (_, Some(title)) => format!("{} at {}", title.label(), found.dsp),
        ("office", None) => format!("in management at {}", found.dsp),
        (role, None) => format!("a {role} at {}", found.dsp),
    };
    let (name, code, from) = (&person.name, &person.code, day(&found.from));
    let mut text = format!("{name} ({code}), {at}, {from}");
    if found.to != found.from {
        text += &format!(" to {}", day(&found.to));
    }
    if !told.is_empty() {
        text += &format!(": {}", told.join(", "));
    }
    text.push('.');
    if !no_ids.is_empty() {
        let names = no_ids.join(", ");
        text += &format!(" They hold no ID that {names} read, so nothing of theirs is there.");
    }
    if !unknown.is_empty() {
        let names = unknown.join(", ");
        text += &format!(" Some days aren't collected in {names}, so they're unknown.");
    }
    if !partial.is_empty() {
        let names = partial.join(", ");
        text += &format!(
            " Some days in {names} were read while under way, so they show only what was known \
             then."
        );
    }
    if !off.is_empty() {
        text += &format!(" Off at {}: {}.", found.dsp, off.join(", "));
    }
    if found.trimmed {
        text += " Some lists were cut short: ask for fewer days or sections to see the rest.";
    }
    text
}

#[cfg(test)]
#[path = "../tests/backend/tools/driver.rs"]
mod tests;
