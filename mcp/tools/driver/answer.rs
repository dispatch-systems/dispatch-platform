//! Published MCP projections and their conversions from feature contracts.
use chrono::DateTime;
use chrono_tz::Tz;
use dispatch_core::manifest::people::Person;
use dispatch_daily_performance_contract as performance;
use dispatch_driver_profiles_contract as profiles;
use dispatch_dvic_contract as dvic;
use dispatch_routes_contract as route;
use dispatch_slack_contract as slack;
use dispatch_timecard_contract as timecard;
use dispatch_weekly_scorecard_contract as scorecard;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;

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
    /// driver, helper, office for management, former for someone who has left, or offboarded for someone Amazon offboarded.
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
    pub(super) fn new(from: String, through: String) -> Self {
        Self { from, through }
    }
    pub(super) fn holds(&self, day: &str) -> bool {
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
    /// Whether the newest roster lists them, offboarded ones included; one it stopped listing keeps what the last to list them said.
    pub listed: bool,
    /// ACTIVE, INACTIVE or OFFBOARDED, as Amazon writes it.
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
    /// CV, CDV, EDV or SV: 90 seconds is the minimum for the first three, 300 for a step van.
    pub vehicle_type: String,
    pub vin: String,
    pub duration_seconds: f64,
    pub minimum_seconds: usize,
    pub short_by_seconds: f64,
}

impl Route {
    /// A route as the answer gives it, its times on the DSP's clock.
    pub(super) fn from_day(it: route::RouteDay, zone: Tz) -> Route {
        let at = |time: Option<i64>| local_time(time, zone, &it.day);
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
pub(super) fn mirrored<A, B: From<A>>(values: Vec<A>) -> Vec<B> {
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
