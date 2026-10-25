//! A DSP's live route data through its days as the server reads it, on time the test moves, with
//! Cortex in fixture mode: each read is the fixture's whole day, read in the DSP's browser, and
//! never Amazon. Each day's first read empties live route data of the day before; reads are kept
//! on the DSP's grid through its window; its stop saves the day into historical route data when
//! the DSP chose so, emptying live route data once it's saved and leaving it as it was otherwise;
//! a restart inside the window empties nothing; and switched off, or with its Cortex connection
//! disabled, it reads no more.
//! Each day is the day Routes reckons too, pinned as the test moves to it. What Routes keeps is
//! read as other features read it, through its contracts.
//! A program of its own, as its registry holds the parts it names, and a program holds one.
use chrono::{DateTime, NaiveDate, Timelike};
use chrono_tz::Tz;
use dispatch_core::{
    Result, State,
    collection::readers,
    db,
    manifest::settings::Section,
    settings::{self, Saved},
    testing,
};
use dispatch_cortex as cortex;
use dispatch_routes_contract as routes;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time::Instant};

const EVERY: Duration = Duration::from_secs(5 * 60);
const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const SECOND: Duration = Duration::from_secs(1);
/// How the fixture's routes progress, as each read shows them.
const READ: &str = "ON_TIME";
/// How a read kept before shows them.
const BEFORE: &str = "AT_RISK";

/// The time as the readers keep it: the system's, moved on as the paused timers are, and not by
/// the real time the test spends meanwhile.
fn now() -> i64 {
    let (timers, system) = (Instant::now().into_std(), std::time::Instant::now());
    let ahead = match timers.checked_duration_since(system) {
        Some(ahead) => ahead.as_millis() as i64,
        None => -(system.duration_since(timers).as_millis() as i64),
    };
    db::now() + ahead
}

/// Live Route Data's section of Settings, as its manifest declares it.
fn collection() -> &'static Section {
    let mut sections = dispatch_live_route_data::FEATURE.settings.iter();
    sections
        .find(|section| section.id == "live_route_data.collection")
        .unwrap()
}

struct Server {
    _root: tempfile::TempDir,
    state: Arc<State>,
    owner: String,
    dsp: String,
    /// Where the DSP is: where it's late morning now, so a window keeps to one day.
    zone: Tz,
    revision: i64,
    stop: watch::Sender<bool>,
    readers: Option<JoinHandle<Result<()>>>,
    /// When the readers last started, which plan again each minute from then.
    since: Instant,
}
impl Server {
    /// A DSP at a station, its Cortex connected, Routes and Live Route Data on, and its live
    /// route data holding a read from before: the readers not started yet.
    async fn start() -> Self {
        testing::install(
            &[&cortex::COLLECTOR],
            &[
                &dispatch_routes::FEATURE,
                &dispatch_live_route_data::FEATURE,
            ],
        );
        let (root, store, dsp) = testing::bootstrapped();
        let owner = testing::platform_owner(&store);
        let hour = DateTime::from_timestamp_millis(now()).unwrap().hour() as i32;
        // Etc/GMT-N is N hours ahead of UTC: here it's eleven, and some minutes.
        let zone: Tz = format!("Etc/GMT{:+}", hour - 11).parse().unwrap();
        testing::set_dsp(&store, &dsp, "Live DSP", zone.name()).unwrap();
        store
            .set_profile(
                &dsp,
                json!({"stationCode":"TST1","abbreviation":"LIVE","setupRequired":false}),
            )
            .unwrap();
        let credentials = json!({"username":"fixture@example.test","password":"fixture-password"});
        testing::connect(&store, &dsp, cortex::PROVIDER, &credentials).unwrap();
        for feature in ["routes", "live_route_data"] {
            store.set_feature(&dsp, feature, true, &owner).unwrap();
        }
        let state = State::new(store.config.clone()).unwrap();
        let (stop, _) = watch::channel(false);
        Self {
            _root: root,
            state,
            owner,
            dsp,
            zone,
            revision: 0,
            stop,
            readers: None,
            since: Instant::now(),
        }
    }
    fn run_readers(&mut self) {
        self.since = Instant::now();
        self.stop.send_replace(false);
        let readers = readers::start(self.state.clone(), self.stop.subscribe());
        self.readers = Some(tokio::spawn(readers));
    }
    /// Stops the readers, as the server does as it stops.
    async fn stop_readers(&mut self) {
        self.stop.send_replace(true);
        self.readers.take().unwrap().await.unwrap().unwrap();
    }
    /// Saves the DSP's Collection section with `values`.
    async fn save(&mut self, values: Value) {
        let (dsp, owner, revision) = (self.dsp.clone(), self.owner.clone(), self.revision);
        let saved = self
            .state
            .run(move |db| settings::save(db, &dsp, &owner, collection(), &values, revision, true))
            .await
            .unwrap();
        let Saved::Saved { revision, .. } = saved else {
            panic!("saved without asking")
        };
        self.revision = revision;
    }
    async fn switch(&self, on: bool) {
        let (dsp, owner) = (self.dsp.clone(), self.owner.clone());
        self.state
            .run(move |db| {
                db.set_feature(&dsp, "live_route_data", on, &owner)
                    .map(drop)
            })
            .await
            .unwrap();
    }
    /// The local time `minutes` whole minutes past the current one where the DSP is, and when
    /// that is.
    fn at(&self, minutes: i64) -> (String, Instant) {
        let now = now();
        let minute = now - now.rem_euclid(60_000) + minutes * 60_000;
        let local = DateTime::from_timestamp_millis(minute)
            .unwrap()
            .with_timezone(&self.zone);
        let when = Instant::now() + Duration::from_millis((minute - now) as u64);
        (local.format("%H:%M").to_string(), when)
    }
    /// Keeps a whole read of today in live route data, as a reader before this one left it: its
    /// routes `BEFORE`, collected two hours from now, so no read before then changes it.
    async fn kept_before(&self) {
        let dsp = self.dsp.clone();
        let (request, revision) = self
            .state
            .read(move |db| {
                let request = dispatch_routes::live_request(db, &dsp)?;
                Ok((request, db.connection_revision(&dsp, cortex::PROVIDER)?))
            })
            .await
            .unwrap();
        let mut read = cortex::routes::fixture(&request).unwrap();
        for summary in read.summaries["itinerarySummaries"].as_array_mut().unwrap() {
            summary["progressStatus"] = json!(BEFORE);
        }
        read.started_at += 2 * 60 * 60 * 1000;
        read.finished_at = read.started_at;
        dispatch_routes::keep_live(&self.state, &self.dsp, read, revision.unwrap())
            .await
            .unwrap();
    }
    /// Saves a whole read of today into historical route data, as the standard collection would:
    /// its routes `BEFORE`, collected now, later than any read live route data holds.
    async fn saved_later(&self) {
        let dsp = self.dsp.clone();
        let request = self
            .state
            .read(move |db| dispatch_routes::live_request(db, &dsp))
            .await
            .unwrap();
        let mut read = cortex::routes::fixture(&request).unwrap();
        for summary in read.summaries["itinerarySummaries"].as_array_mut().unwrap() {
            summary["progressStatus"] = json!(BEFORE);
        }
        read.finished_at = now();
        read.started_at = read.finished_at - 1000;
        let saved = dispatch_routes::save_day(&self.state, &self.dsp, "later", read, Vec::new());
        assert!(saved.await.unwrap());
    }
    /// How the first driver's route progresses in live route data: none while it holds no day.
    async fn live(&self) -> Option<String> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| {
                let live = routes::live_for_dsp(db, &dsp)?.expect("Routes is on");
                Ok(first(live.day(db, &dsp)?))
            })
            .await
            .unwrap()
    }
    /// The day live route data holds: none while it holds none.
    async fn live_day(&self) -> Option<String> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| {
                let live = routes::live_for_dsp(db, &dsp)?.expect("Routes is on");
                live.live_day(db, &dsp)
            })
            .await
            .unwrap()
    }
    /// How the first driver's route progresses on `day` in historical route data.
    async fn history(&self, day: NaiveDate) -> Option<String> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| {
                let history = routes::for_dsp(db, &dsp)?.expect("Routes is on");
                Ok(first(history.day(db, &dsp, &day.to_string())?))
            })
            .await
            .unwrap()
    }
    /// Disables the DSP's Cortex connection, as its owner does, or connects it again.
    async fn connected(&self, on: bool) {
        let dsp = self.dsp.clone();
        self.state
            .run(move |db| match on {
                true => testing::ready_connection(db, &dsp, cortex::PROVIDER),
                false => testing::disable_connection(db, &dsp, cortex::PROVIDER),
            })
            .await
            .unwrap();
    }
    /// Moves the DSP to another station.
    async fn station(&self, code: &'static str) {
        let dsp = self.dsp.clone();
        self.state
            .run(move |db| db.set_profile(&dsp, json!({"stationCode":code})).map(drop))
            .await
            .unwrap();
    }
    /// Holds historical route data's database for writing, as a writer of its own busy with it
    /// does, until dropped.
    async fn hold_history(&self) -> rusqlite::Connection {
        let dsp = self.dsp.clone();
        let data = self.state.read(move |db| db.area(&dsp, "data")).await;
        let busy = rusqlite::Connection::open(data.unwrap().join("routedata/routedata.sqlite"));
        let busy = busy.unwrap();
        busy.execute_batch("BEGIN IMMEDIATE").unwrap();
        busy
    }
    /// The readers' last plan before `at`: they plan again each minute from their start.
    fn plan_before(&self, at: Instant) -> Instant {
        let minutes = ((at - self.since).as_millis() - 1) / 60_000;
        self.since + Duration::from_secs(minutes as u64 * 60)
    }
    /// The DSP's reads as the server records them, in order.
    async fn records(&self) -> Vec<testing::LiveRead> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| testing::live_reads(db, &dsp))
            .await
            .unwrap()
    }
}
/// How the first driver's route progresses on `driven`.
fn first(driven: routes::Driven) -> Option<String> {
    let mut routes = driven.routes.into_iter();
    routes
        .find(|route| route.driver_id == "driver-1")
        .and_then(|route| route.progress)
}
async fn until(at: Instant) {
    tokio::time::sleep_until(at).await;
}
/// When a read the server recorded at `at` began.
fn instant(at: i64) -> Instant {
    let now = now();
    if at >= now {
        Instant::now() + Duration::from_millis((at - now) as u64)
    } else {
        Instant::now() - Duration::from_millis((now - at) as u64)
    }
}

/// Pins today, where the DSP is, as the day the readers read now: the day Routes reckons in.
fn pin(server: &Server) -> NaiveDate {
    let today = DateTime::from_timestamp_millis(now())
        .unwrap()
        .with_timezone(&server.zone)
        .date_naive();
    testing::pin_today(Some(today));
    today
}

#[tokio::test(start_paused = true)]
async fn a_dsps_live_route_data_through_its_days() {
    let mut server = Server::start().await;
    let first_day = pin(&server);
    server.kept_before().await;
    assert_eq!(server.live().await.as_deref(), Some(BEFORE));
    let ((start, open), (stop, close)) = (server.at(10), server.at(40));
    server.save(json!({"start":start,"stop":stop})).await;
    server.run_readers();

    // The day's first read empties live route data of the day before, and is kept.
    until(open + EVERY).await;
    assert_eq!(server.live().await.as_deref(), Some(READ));
    assert_eq!(server.live_day().await, Some(first_day.to_string()));
    // Every read through the window is kept on the DSP's grid: each the fixture's whole day,
    // read in the DSP's browser.
    until(close + 6 * MINUTE).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 6, "{reads:?}");
    for read in &reads {
        assert_eq!(read.outcome, "kept");
        let counted = (&read.counts["listed"], &read.counts["browser"]);
        assert_eq!(counted, (&json!(2), &json!(1)), "{read:?}");
    }
    for pair in reads.windows(2) {
        assert!((pair[1].at - pair[0].at - EVERY.as_millis() as i64).abs() <= 1000);
    }
    // Its stop saved the day into historical route data, and emptied live route data.
    assert_eq!(server.history(first_day).await.as_deref(), Some(READ));
    assert_eq!(server.live_day().await, None);

    // The next day's first read empties live route data again.
    server.save(json!({"save_to_history":false})).await;
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    let second_day = pin(&server);
    assert_eq!(second_day, first_day.succ_opt().unwrap());
    server.kept_before().await;
    until(open + EVERY).await;
    assert_eq!(server.live().await.as_deref(), Some(READ));
    assert_eq!(server.live_day().await, Some(second_day.to_string()));
    // Started again inside its window, it reads on with the day as it was, emptying nothing.
    server.kept_before().await;
    let first = instant(server.records().await[6].at);
    until(first + MINUTE).await;
    server.stop_readers().await;
    // Half a minute off the grid, so its plans each minute from then fall between its slots.
    until(first + 2 * MINUTE + 30 * SECOND).await;
    server.run_readers();
    until(first + EVERY + MINUTE).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 6 + 2, "{reads:?}");
    assert_eq!(reads[7].outcome, "kept");
    assert_eq!(server.live().await.as_deref(), Some(BEFORE));
    // Its stop saves nothing, as the DSP chose: historical route data has the first day alone,
    // and live route data holds the second still.
    until(close + 6 * MINUTE).await;
    assert_eq!(server.records().await.len(), 6 + 6);
    assert_eq!(server.history(first_day).await.as_deref(), Some(READ));
    assert_eq!(server.history(second_day).await, None);
    assert_eq!(server.live().await.as_deref(), Some(BEFORE));
    assert_eq!(server.live_day().await, Some(second_day.to_string()));

    // Switched off inside the window, it reads no more.
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    pin(&server);
    until(open + EVERY).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 12 + 1);
    until(instant(reads[12].at) + MINUTE).await;
    server.switch(false).await;
    until(close + 6 * MINUTE).await;
    assert_eq!(server.records().await.len(), 12 + 1);

    // Nor once its Cortex connection is disabled inside the window: a read due before the next
    // minute's plan is refused, reading nothing, and none follows.
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    pin(&server);
    server.switch(true).await;
    until(open + EVERY).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 13 + 1);
    let slot = instant(reads[13].at) + EVERY;
    until(server.plan_before(slot) + Duration::from_millis(1)).await;
    server.connected(false).await;
    until(slot + SECOND).await;
    let refused = server.records().await;
    assert_eq!(refused.len(), 14 + 1);
    assert_eq!(refused[14].code.as_deref(), Some("connection_required"));
    until(close + 6 * MINUTE).await;
    assert_eq!(server.records().await.len(), 14 + 1);

    // Saved again, a day whose save meets a busy database stops once it's saved, a minute on.
    server.connected(true).await;
    server.save(json!({"save_to_history":true})).await;
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    let fifth_day = pin(&server);
    until(open + EVERY).await;
    let first = instant(server.records().await[15].at);
    until(first + 5 * EVERY + SECOND).await;
    let busy = server.hold_history().await;
    // Its stop is due as its window closes, at its place after the close.
    until(first + 6 * EVERY + 30 * SECOND).await;
    assert_eq!(server.history(fifth_day).await, None);
    assert_eq!(server.live_day().await, Some(fifth_day.to_string()));
    drop(busy);
    until(first + 6 * EVERY + 2 * MINUTE).await;
    assert_eq!(server.history(fifth_day).await.as_deref(), Some(READ));
    assert_eq!(server.live_day().await, None);
    assert_eq!(server.records().await.len(), 15 + 6);

    // Moved to another station after the day's last read, the day can never be saved as read:
    // its stop ends it unsaved, live route data keeping it, and the next day reads at the new one.
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    let sixth_day = pin(&server);
    until(open + EVERY).await;
    let first = instant(server.records().await[21].at);
    until(first + 5 * EVERY + SECOND).await;
    server.station("TST2").await;
    until(close + 6 * MINUTE).await;
    assert_eq!(server.history(sixth_day).await, None);
    assert_eq!(server.live_day().await, Some(sixth_day.to_string()));
    let (open, close) = (open + DAY, close + DAY);
    until(open - MINUTE).await;
    let seventh_day = pin(&server);
    until(open + EVERY).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 27 + 1, "{:?}", &reads[26..]);
    assert_eq!(reads[27].outcome, "kept");
    assert_eq!(server.live_day().await, Some(seventh_day.to_string()));

    // A day historical route data has a later read of by its stop is historical route data's
    // already: that read stays the day's, and live route data is emptied all the same.
    let first = instant(reads[27].at);
    until(first + 5 * EVERY + SECOND).await;
    server.saved_later().await;
    until(close + 6 * MINUTE).await;
    assert_eq!(server.history(seventh_day).await.as_deref(), Some(BEFORE));
    assert_eq!(server.live_day().await, None);
    assert_eq!(server.records().await.len(), 27 + 6);
    testing::pin_today(None);
}
