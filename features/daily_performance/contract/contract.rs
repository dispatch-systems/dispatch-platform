//! A driver's Amazon Daily Quality and Day Safety, as any feature or agent tool reads them without
//! depending on Daily Performance: their numbers day by day, the packages they returned to the
//! station and their safety events. Each day is collected whole or not at all, so a day collected
//! without a driver's rows had nothing of theirs reported, and a day not collected is unknown.
//! Daily Performance exposes it as `daily_metrics`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads a DSP's daily metrics, a driver's at a time, as Daily Performance keeps them.
pub trait DailyMetrics: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The first day collected and the last, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>>;
    /// A driver's days from `from` to `to`, YYYY-MM-DD in the DSP's own time, under any of
    /// `ids`. It refuses a day that isn't YYYY-MM-DD with `invalid_date`; and with
    /// `invalid_input`, `from` after `to`, more than 31 days, none or more than 20 IDs, or an ID
    /// that isn't up to 128 letters, digits, `_`, `-` or `.`.
    fn days(&self, db: &Store, dsp: &str, ids: &[String], from: &str, to: &str) -> Result<Days>;
}

/// The days collected, YYYY-MM-DD in the DSP's own time.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    pub from: String,
    pub through: String,
}

/// A driver's days over the days asked for.
#[derive(Clone, Debug)]
pub struct Days {
    /// The days asked for that were collected: any other is unknown.
    pub collected: Vec<String>,
    /// Each day one of their IDs has rows, by day and then ID.
    pub days: Vec<Day>,
}

/// What Amazon reported of one of a driver's IDs on one day. A part it reported nothing of is
/// none or empty, which doesn't prove there was nothing to report.
#[derive(Clone, Debug)]
pub struct Day {
    /// The day, YYYY-MM-DD in the DSP's own time.
    pub date: String,
    /// The ID in `source` it's reported under.
    pub id: String,
    pub quality: Option<Quality>,
    pub safety: Option<Safety>,
    pub feedback: Option<Feedback>,
    pub behaviors: Option<Behaviors>,
    pub returns: Option<Returns>,
    /// Each package they returned to the station.
    pub returned: Vec<Returned>,
    /// Each safety event, once: as assessed when Amazon has assessed it, otherwise as it was
    /// reported live.
    pub events: Vec<SafetyEvent>,
}

/// The day's Daily Quality. Each number is as Amazon gives it, and none when it gives none; a
/// rate is a fraction of one.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct Safety {
    pub speeding: Option<f64>,
    pub seatbelt: Option<f64>,
    pub distraction: Option<f64>,
    pub sign_signal: Option<f64>,
    pub following_distance: Option<f64>,
}

/// The customers' feedback on the day's deliveries.
#[derive(Clone, Debug)]
pub struct Feedback {
    pub positive: Option<f64>,
    pub negative: Option<f64>,
    /// Each reason for negative feedback that isn't zero.
    pub negatives: Vec<Count>,
}

/// The day's delivery success behaviors.
#[derive(Clone, Debug)]
pub struct Behaviors {
    pub dsb: Option<f64>,
    /// Each kind of behavior that isn't zero.
    pub concerns: Vec<Count>,
}

/// A count, named by Amazon's field for it, such as `never_received_delivery_cnt`.
#[derive(Clone, Debug)]
pub struct Count {
    pub what: String,
    pub count: f64,
}

/// The day's packages returned to the station, in all, those the driver could have delivered,
/// and those exempted.
#[derive(Clone, Debug)]
pub struct Returns {
    pub all: Option<f64>,
    pub controllable: Option<f64>,
    pub exempted: Option<f64>,
}

/// A package returned to the station.
#[derive(Clone, Debug)]
pub struct Returned {
    pub tracking_id: Option<String>,
    /// Why, as Amazon codes it, such as `BUSINESS CLOSED`.
    pub reason: Option<String>,
    /// Whether it counts against the driver's delivery completion.
    pub impacting: Option<bool>,
    pub exempted: Option<bool>,
    pub exemption_reason: Option<String>,
    /// What Amazon coaches the driver to do about it.
    pub coaching: Option<String>,
}

/// A safety event, without where it happened, its video or the vehicle.
#[derive(Clone, Debug)]
pub struct SafetyEvent {
    pub event_id: Option<String>,
    /// What happened, as Amazon names it, such as `Speeding`.
    pub kind: Option<String>,
    pub subtype: Option<String>,
    pub severity: Option<String>,
    /// When it started, in local time, as Amazon writes it.
    pub started: Option<String>,
    /// Whether it counts against the driver, once assessed.
    pub impacting: Option<bool>,
    /// Whether Amazon has assessed it, rather than only reporting it live.
    pub assessed: bool,
}

/// What reads the DSP's daily metrics: only while Daily Performance is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn DailyMetrics>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn DailyMetrics>(db, dsp)
}
