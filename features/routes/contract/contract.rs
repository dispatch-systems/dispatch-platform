//! Each driver's routes, as any feature or agent tool reads them without depending on Routes:
//! the itineraries a driver drove day by day, with their stops, times and packages, the
//! packages they didn't deliver, and the days collected. Routes collects a day's routes whole,
//! so a complete day without a driver's itinerary is a day they didn't drive; a day read while
//! under way proves nothing by what it lacks, since it may be read before dispatch; and a day
//! outside what's collected is unknown. Routes exposes it as `driver_routes`, from historical
//! route data; and today as live reads leave it, with the work phone each driver holds now, as
//! `live_route_data` (`LiveRouteData`). Each driver's latest work phone of a day, today and on a
//! saved day, is in both (`day_phones`), and `RouteDataChanged` says when a day's route data
//! changed. Who held which work phone, from when until when, is in both too, and
//! `WorkPhonesChanged` says when it changed, and of which day.
use dispatch_core::{
    Result,
    db::Store,
    manifest::{events::Event, people::Source},
};

/// The most IDs one read of a driver's routes names.
pub const MOST_IDS: usize = 16;
/// The longest ID a read of a driver's routes names, in characters.
pub const MOST_ID_LENGTH: usize = 128;
/// The most days one read of a driver's routes covers, its first and last among them.
pub const MOST_DAYS: i64 = 366;

/// Reads the routes a DSP's drivers drove, as Routes keeps them for the DSP's station.
pub trait DriverRoutes: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The first and last days collected, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>>;
    /// What the driver holding `ids` drove from `from` to `to`, YYYY-MM-DD in the DSP's own
    /// time, and which of those days were collected; no IDs answer the days alone. An ID is
    /// matched exactly. Routes refuses a day that isn't YYYY-MM-DD with `invalid_date`; and
    /// with `invalid_input`, `from` after `to`, more than `MOST_DAYS` days, more than `MOST_IDS`
    /// IDs, or an ID that's empty or over `MOST_ID_LENGTH` characters.
    fn routes(&self, db: &Store, dsp: &str, ids: &[String], from: &str, to: &str)
    -> Result<Driven>;
    /// What every driver drove on `day`, YYYY-MM-DD in the DSP's own time, each itinerary by
    /// the ID that drove it, and whether the day was collected. Routes refuses a day that isn't
    /// YYYY-MM-DD with `invalid_date`.
    fn day(&self, db: &Store, dsp: &str, day: &str) -> Result<Driven>;
    /// Each driver's latest work phone on `day`, YYYY-MM-DD in the DSP's own time, as historical
    /// route data keeps the day's read. Routes refuses a day that isn't YYYY-MM-DD with
    /// `invalid_date`.
    fn day_phones(&self, db: &Store, dsp: &str, day: &str) -> Result<DayPhones>;
    /// Whether `phone`, by its last ten digits, is an itinerary's work phone on a day route data
    /// keeps, live or historical: a work phone nobody has today is still one. A number with fewer
    /// than ten digits never is.
    fn ever_work_phone(&self, db: &Store, dsp: &str, phone: &str) -> Result<bool>;
    /// Every hold on a work phone historical route data keeps of `day`, YYYY-MM-DD in the DSP's
    /// own time, ended ones too, oldest first: those of the day's saved read, kept once live route
    /// data is emptied. Routes refuses a day that isn't YYYY-MM-DD with `invalid_date`.
    fn work_phone_holds(&self, db: &Store, dsp: &str, day: &str) -> Result<Vec<WorkPhoneHold>>;
    /// Whether `phone`, by its last ten digits, was ever held as a work phone in live or
    /// historical route data: a work phone nobody holds now is still one. A number with fewer
    /// than ten digits never was.
    fn was_work_phone(&self, db: &Store, dsp: &str, phone: &str) -> Result<bool>;
}

/// The days collected, YYYY-MM-DD in the DSP's own time. Days between may be missing.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    pub from: String,
    pub through: String,
}

/// What a driver, or every driver, drove over the days asked.
#[derive(Clone, Debug)]
pub struct Driven {
    /// Each day asked that was collected, oldest first. A day missing is unknown.
    pub days: Vec<CollectedDay>,
    /// Each itinerary they drove on those days, oldest first. A day may have several.
    pub routes: Vec<RouteDay>,
}

/// A day collected.
#[derive(Clone, Debug)]
pub struct CollectedDay {
    /// YYYY-MM-DD in the DSP's own time.
    pub day: String,
    /// False for a day read while still under way: its counts are so far, a package not
    /// delivered may still be out, and a driver without a route may not be dispatched yet.
    pub complete: bool,
}

/// One itinerary a driver drove on a day, or, in live route data, a route they're assigned to and
/// haven't started: its plan, with nothing departed or done. Times are epoch milliseconds.
#[derive(Clone, Debug)]
pub struct RouteDay {
    /// Its day, YYYY-MM-DD in the DSP's own time.
    pub day: String,
    /// The ID that drove it: one of those asked for.
    pub driver_id: String,
    pub route_code: Option<String>,
    /// How Amazon says it went, such as COMPLETE.
    pub status: Option<String>,
    /// How Amazon says it kept up, such as ON_TIME or BEHIND.
    pub progress: Option<String>,
    /// The van driven.
    pub vin: Option<String>,
    /// When its wave starts at the station.
    pub wave_start_at: Option<i64>,
    pub planned_departure_at: Option<i64>,
    pub departed_at: Option<i64>,
    pub first_delivery_at: Option<i64>,
    pub last_delivery_at: Option<i64>,
    pub session_end_at: Option<i64>,
    pub stops: i64,
    pub stops_completed: i64,
    /// The packages to drop off that were delivered, and that weren't.
    pub delivered: i64,
    pub not_delivered: i64,
    /// The packages taken off the route, as when another driver rescued them.
    pub removed: i64,
    pub breaks_secs: Option<i64>,
    pub overtime_secs: Option<i64>,
    /// Every package to drop off that wasn't delivered, in the order they were attempted.
    pub undelivered: Vec<Undelivered>,
}

/// A package to drop off that wasn't delivered.
#[derive(Clone, Debug)]
pub struct Undelivered {
    /// Its tracking ID, such as TBA000000000001.
    pub tracking_id: Option<String>,
    /// Where Amazon says it ended, such as UNDELIVERABLE.
    pub state: Option<String>,
    /// Why, as Amazon gives it, such as BUSINESS_CLOSED, or none when it gives none.
    pub reason: Option<String>,
    /// When it was attempted.
    pub attempted_at: Option<i64>,
}

/// What reads the DSP's drivers' routes: only while Routes is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn DriverRoutes>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn DriverRoutes>(db, dsp)
}

/// Live route data: today's routes as the last whole live read left them, each driver's latest
/// work phone, the company work phone each driver holds now and every hold of the day; nothing of
/// another day. Apart from the historical route data
/// `DriverRoutes` reads: a day joins it once saved there, and leaves live route data as a stop
/// saves it (`save_and_clear_live`); a day left unsaved, as when the DSP saves none, stays until
/// the next day's first read.
pub trait LiveRouteData: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The day live route data holds, YYYY-MM-DD in the DSP's own time: today, or none when it
    /// holds no read of today.
    fn live_day(&self, db: &Store, dsp: &str) -> Result<Option<String>>;
    /// Every driver's itineraries today as last read live, and each route a driver is assigned to
    /// without an itinerary on it yet, as before dispatch; none without a live day. Its day is
    /// never complete.
    fn day(&self, db: &Store, dsp: &str) -> Result<Driven>;
    /// The itineraries today of the drivers holding `ids` as last read live, and the routes
    /// they're assigned to without one yet. An ID is matched exactly. Routes refuses more than
    /// `MOST_IDS` IDs, or one that's empty or over `MOST_ID_LENGTH` characters, with
    /// `invalid_input`.
    fn routes(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Driven>;
    /// Each driver's latest work phone today, as the last whole live read left it; `Uncollected`
    /// without a live day.
    fn day_phones(&self, db: &Store, dsp: &str) -> Result<DayPhones>;
    /// The work phone each driver holds now, and since when; none without a live day.
    fn work_phones(&self, db: &Store, dsp: &str) -> Result<Vec<WorkPhone>>;
    /// Every hold on a work phone of the live day, ended ones too, oldest first; none without a
    /// live day.
    fn work_phone_holds(&self, db: &Store, dsp: &str) -> Result<Vec<WorkPhoneHold>>;
}

/// Each driver's latest work phone of a day, from the work phone each of their itineraries shows:
/// the one on their itinerary of the day that began last, by when it started, or else was planned
/// to depart, of those showing one; of two no time tells apart, the one whose ID sorts last. An
/// itinerary shows the phone its driver's records show with it: the day's list's, else its own
/// response's. A phone passes from driver to driver, so a driver's latest is the one they had last:
/// of two who swapped phones, each has the one they swapped to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DayPhones {
    /// Route data keeps no read of the day: it wasn't collected, or isn't kept any more; or, live,
    /// it holds no read of today.
    Uncollected,
    /// The day's read was kept before Routes kept its itineraries' work phones, and they aren't
    /// extracted from its stored responses yet.
    Unextracted,
    /// Each driver's latest work phone of the day, one each, by their ID: none when no itinerary
    /// showed one.
    Read(Vec<DriverPhone>),
}

/// A driver's latest work phone of a day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverPhone {
    /// The ID that had it: one of the source's.
    pub driver_id: String,
    /// Its ten digits.
    pub phone: String,
    /// Whether it's another driver ID's latest phone that day too: then the day names no one
    /// driver whose phone it is, and each of them has it here.
    pub shared: bool,
}

/// A company work phone a driver holds now: a phone the DSP passes from driver to driver, so
/// never a sign that two drivers are one person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkPhone {
    /// The ID that holds it: one of `LiveRouteData::source`'s.
    pub driver_id: String,
    /// Its ten digits.
    pub phone: String,
    /// When they took it, epoch milliseconds.
    pub since: i64,
}

/// A driver's hold on a company work phone on a day: from their claim until they switched to
/// another or someone else claimed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkPhoneHold {
    /// The ID that held it: one of the source's.
    pub driver_id: String,
    /// Its ten digits.
    pub phone: String,
    /// When they took it, epoch milliseconds.
    pub since: i64,
    /// When it ended, epoch milliseconds: none while they hold it, or on a saved day, when they
    /// held it still as its read was taken.
    pub until: Option<i64>,
}

/// Who held the DSP's work phones on a day changed, once it's kept: a live read claimed, moved,
/// handed over or withdrew a hold; live route data was emptied of the day; or the day was saved
/// into historical route data with its holds. Published after `people.changed` when both are, so
/// the drivers it names are among the DSP's people.
#[derive(Clone, Debug)]
pub struct WorkPhonesChanged {
    /// The day whose holds changed, YYYY-MM-DD in the DSP's own time.
    pub day: String,
}
impl Event for WorkPhonesChanged {
    const NAME: &'static str = "routes.work_phones_changed";
}

/// A day's route data changed, once it's kept: a live read changed it; live route data was emptied
/// of it, or emptied holding none, of today; a read of it became the day's in historical route
/// data, saved, collected or reprocessed; or its itineraries' work phones were extracted from its
/// stored responses. Published after `people.changed` when a live read or a save names drivers
/// anew. A collected or reprocessed day is published once Routes hears `collection.succeeded` of
/// its collection, before the features after it in the registry hear it, or by the next minute's
/// upkeep, whichever is first: until then it's noted only in the server's memory, so a restart
/// before then publishes none.
#[derive(Clone, Debug)]
pub struct RouteDataChanged {
    /// The day whose route data changed, YYYY-MM-DD in the DSP's own time.
    pub day: String,
}
impl Event for RouteDataChanged {
    const NAME: &'static str = "routes.data_changed";
}

/// What reads the DSP's live route data: only while Routes is on there.
pub fn live_for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn LiveRouteData>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn LiveRouteData>(db, dsp)
}
