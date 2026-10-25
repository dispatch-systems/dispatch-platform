//! A DSP's live route data through its days as the server reads it, on time the test moves, with
//! Cortex in fixture mode: each read is the fixture's whole day, read in the DSP's browser, and
//! never Amazon. Each day's first read empties live route data of the day before; reads are kept
//! on the DSP's grid through its window; its stop saves the day into historical route data when
//! the DSP chose so, and leaves live route data as it was either way; a restart inside the window
//! empties nothing; and switched off, it reads no more.
//! A program of its own, as its registry holds the parts it names, and a program holds one.
use chrono::{DateTime, Timelike};
use chrono_tz::Tz;
use dispatch_core::{
    Result, State,
    collection::readers,
    db::{self, s},
    manifest::settings::Section,
    settings::{self, Saved},
    testing,
};
use dispatch_cortex as cortex;
use dispatch_routes::RoutesStore;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time::Instant};

const EVERY: Duration = Duration::from_secs(5 * 60);
const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

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

/// One read as the server records it.
#[derive(Debug)]
struct Recorded {
    at: i64,
    outcome: String,
    counts: Value,
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
        }
    }
    fn run_readers(&mut self) {
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
    /// Keeps a whole read of today in live route data, as a reader before this one left it.
    async fn kept_before(&self) {
        let dsp = self.dsp.clone();
        let request = self
            .state
            .read(move |db| dispatch_routes::live_request(db, &dsp))
            .await
            .unwrap();
        let read = cortex::routes::fixture(&request).unwrap();
        dispatch_routes::keep_live(&self.state, &self.dsp, read)
            .await
            .unwrap();
    }
    /// The day live route data holds, by its publication's id: none while it holds none.
    async fn live(&self) -> Option<String> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| {
                let live = db.routes_live_db(&dsp)?;
                let held = live.one("SELECT id FROM route_publications", [])?;
                Ok(held.map(|row| s(&row, "id").to_owned()))
            })
            .await
            .unwrap()
    }
    /// The key of the read historical route data holds as today's.
    async fn history(&self) -> Option<String> {
        let dsp = self.dsp.clone();
        self.state
            .read(move |db| {
                let history = db.routes_db(&dsp)?;
                let read =
                    history.one("SELECT job_id FROM route_publications WHERE active=1", [])?;
                Ok(read.map(|row| s(&row, "job_id").to_owned()))
            })
            .await
            .unwrap()
    }
    /// The DSP's reads as the server records them, in order.
    async fn records(&self) -> Vec<Recorded> {
        let dsp = self.dsp.clone();
        let rows: Vec<(i64, String, Option<String>)> = self
            .state
            .read(move |db| {
                db.jobs.query_as(
                    "SELECT at,outcome,counts FROM live_reads WHERE dsp_id=? ORDER BY at",
                    [dsp],
                )
            })
            .await
            .unwrap();
        rows.into_iter()
            .map(|(at, outcome, counts)| Recorded {
                at,
                outcome,
                counts: counts.map_or(Value::Null, |c| serde_json::from_str(&c).unwrap()),
            })
            .collect()
    }
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

#[tokio::test(start_paused = true)]
async fn a_dsps_live_route_data_through_its_days() {
    let mut server = Server::start().await;
    server.kept_before().await;
    let before = server.live().await.unwrap();
    let ((start, open), (stop, close)) = (server.at(10), server.at(40));
    server.save(json!({"start":start,"stop":stop})).await;
    server.run_readers();

    // The day's first read empties live route data of the day before, and is kept.
    until(open + EVERY).await;
    let today = server.live().await.unwrap();
    assert_ne!(today, before);
    // Every read through the window is kept on the DSP's grid, in place: each the fixture's whole
    // day, read in the DSP's browser.
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
    assert_eq!(server.live().await, Some(today.clone()));
    // Its stop saved the day into historical route data, and live route data holds it still.
    let saved = server.history().await.unwrap();
    assert!(saved.starts_with("live:"), "{saved}");

    // The next day saves nothing as it stops.
    server.save(json!({"save_to_history":false})).await;
    let (open, close) = (open + DAY, close + DAY);
    until(open + EVERY).await;
    let tomorrow = server.live().await.unwrap();
    assert_ne!(tomorrow, today);
    // Started again inside its window, it reads on with the day as it was.
    let first = instant(server.records().await[6].at);
    until(first + MINUTE).await;
    server.stop_readers().await;
    until(first + 2 * MINUTE).await;
    server.run_readers();
    until(first + EVERY + MINUTE).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 6 + 2, "{reads:?}");
    assert_eq!(server.live().await, Some(tomorrow.clone()));
    until(close + 6 * MINUTE).await;
    assert_eq!(server.records().await.len(), 6 + 6);
    assert_eq!(server.live().await, Some(tomorrow));
    assert_eq!(server.history().await, Some(saved));

    // Switched off inside the window, it reads no more.
    let (open, close) = (open + DAY, close + DAY);
    until(open + EVERY).await;
    let reads = server.records().await;
    assert_eq!(reads.len(), 12 + 1);
    until(instant(reads[12].at) + MINUTE).await;
    server.switch(false).await;
    until(close + 6 * MINUTE).await;
    assert_eq!(server.records().await.len(), 12 + 1);
}
