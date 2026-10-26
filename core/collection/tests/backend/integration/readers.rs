//! The live readers as the server runs them, under a stand-in feature that reads every five
//! minutes, or as often as a DSP saves, inside the window each DSP saves, on time the test
//! moves: each DSP on its grid from its window's start, its days started and stopped, its slot
//! kept across a restart, no more reads or stops at once than the server allows, a read the
//! browsers' memory turned away tried once more within its slot, waits of five minutes at most
//! however seldom a DSP reads, a stalled DSP started again alone, switching off mid-window, a
//! switch or a saved window taking effect at once, the browser a hook opens, and how a DSP's
//! reading goes as Settings shows it.
//! A program of its own, as its registry holds the stand-ins, and a program holds one registry.
use chrono::{DateTime, NaiveDate, Timelike, Weekday::*};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result, State,
    collection::{
        api::types::LiveReading,
        browser::{self, Collected, Pending, browseros},
        readers::{self, READS_AT_ONCE, Reading},
        registry::{self, Provider},
    },
    db::{self, Kind, Migration, Migrations, Store, migrations::Apply},
    manifest::{
        Capability, Collection, Collector, Counts, Feature, LiveReader, Schedule, feature,
        optional, perm, section,
        settings::{Section, number},
        time,
    },
    settings::{self, Saved},
    testing,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Semaphore, task::JoinHandle, time::Instant};

const EVERY: Duration = Duration::from_secs(300);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);
const SECOND: Duration = Duration::from_secs(1);
/// How long a read waits for its turn, and runs, at most, however seldom it reads.
const LONGEST: Duration = Duration::from_secs(5 * 60);

// The stand-in feature: a DSP reads every day, its window its section's two times, every five
// minutes unless it saves another interval.

const WINDOW: Section = section(
    "probe.window",
    "Window",
    "probe.manage",
    &[
        time("start", "Start", "08:00"),
        time("stop", "Stop", "21:00"),
        number("every", "Every", 1, 60, Some(5)),
    ],
);
#[derive(serde::Deserialize)]
struct Times {
    start: String,
    stop: String,
    /// Minutes between reads.
    every: u64,
}
static PROBE: Feature = Feature {
    switch: optional("probe", "Probe", &[]),
    permissions: &[perm("probe.manage", "Manage Probe", 1)],
    settings: &[WINDOW],
    live_readers: &[&READER],
    ..feature("probe")
};

/// What one test's DSPs saw, and what they do as they read.
struct Probe {
    seen: Mutex<Vec<(String, Seen)>>,
    /// Reads of `Gated` DSPs wait for it.
    gate: Semaphore,
    flying: AtomicUsize,
    most: AtomicUsize,
}
#[derive(Clone, Copy, PartialEq)]
enum Script {
    Reads,
    /// Each read waits for the probe's gate.
    Gated,
    /// Its next keeping never ends.
    Hangs,
    /// Each read fails.
    Fails,
    /// Each read opens the DSP's browser.
    Browses,
    /// Each read gives up opening the DSP's browser as it signs in, then opens it.
    CutsOff,
    /// Each keeping opens the DSP's browser and waits for the probe's gate.
    KeepsBrowsing,
    /// Each stop fails, as one that emptied what its day kept and failed after does.
    FailsToStop,
    /// Each read finds the browsers' memory busy.
    MemoryBusy,
    /// Its next read finds the browsers' memory busy; those after read.
    MemoryBusyOnce,
}
#[derive(Clone, Debug, PartialEq)]
enum Seen {
    /// The read, of the day's memory, and when.
    Read(u64, Instant),
    /// A read dropped before it ended.
    Dropped,
    /// A keeping dropped before it ended.
    Abandoned,
    Started(NaiveDate),
    Kept(u64),
    Stopped(NaiveDate),
    /// What the browser a read opened was, what the next opening answered meanwhile, and
    /// whether it was still open after a job at its revision ended.
    Browser {
        open: usize,
        busy: bool,
        second: String,
        after_a_job: bool,
    },
    /// Whether a browser given up on as it signed in was still there, and what opening it
    /// again answered.
    CutOff {
        left: bool,
        again: String,
    },
    /// Whether the browser a keeping held was closed as it waited.
    KeptBrowsing {
        closed: bool,
    },
}
static SCRIPTS: Mutex<Vec<(String, Arc<Probe>, Script)>> = Mutex::new(Vec::new());

/// The scripts, though another test failed holding them.
fn scripts() -> std::sync::MutexGuard<'static, Vec<(String, Arc<Probe>, Script)>> {
    SCRIPTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn script(dsp: &str) -> (Arc<Probe>, Script) {
    let scripts = scripts();
    let (_, probe, script) = scripts
        .iter()
        .find(|(id, ..)| id == dsp)
        .expect("a DSP of a test's");
    (probe.clone(), *script)
}
fn set_script(dsp: &str, to: Script) {
    let mut scripts = scripts();
    scripts.iter_mut().find(|(id, ..)| id == dsp).unwrap().2 = to;
}
impl Probe {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            gate: Semaphore::new(0),
            flying: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
        }
    }
    fn saw(&self, dsp: &str, seen: Seen) {
        self.seen.lock().unwrap().push((dsp.to_owned(), seen));
    }
    /// What `dsp` saw, in order.
    fn of(&self, dsp: &str) -> Vec<Seen> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|(id, _)| id == dsp)
            .map(|(_, seen)| seen.clone())
            .collect()
    }
    fn reads(&self, dsp: &str) -> Vec<(u64, Instant)> {
        let seen = self.of(dsp);
        seen.iter()
            .filter_map(|seen| match seen {
                Seen::Read(held, at) => Some((*held, *at)),
                _ => None,
            })
            .collect()
    }
}
/// A keeping that never ends, seen abandoned once it's dropped.
struct Hang(Arc<Probe>, String);
impl Drop for Hang {
    fn drop(&mut self) {
        self.0.saw(&self.1, Seen::Abandoned);
    }
}
/// What opening the DSP's browser answers: its error's code, or `opened`.
async fn opening(reading: &Reading) -> String {
    match reading.browser(PROVIDER).await {
        Ok(_) => "opened".to_owned(),
        Err(error) => error.code,
    }
}
/// A read under way: counted while it flies, and seen dropped unless it lands.
struct Flight<'a> {
    probe: &'a Probe,
    dsp: &'a str,
    landed: bool,
}
impl<'a> Flight<'a> {
    fn new(probe: &'a Probe, dsp: &'a str) -> Self {
        let flying = probe.flying.fetch_add(1, Ordering::SeqCst) + 1;
        probe.most.fetch_max(flying, Ordering::SeqCst);
        Self {
            probe,
            dsp,
            landed: false,
        }
    }
}
impl Drop for Flight<'_> {
    fn drop(&mut self) {
        self.probe.flying.fetch_sub(1, Ordering::SeqCst);
        if !self.landed {
            self.probe.saw(self.dsp, Seen::Dropped);
        }
    }
}

struct Reader;
static READER: Reader = Reader;
/// How many reads the day's memory has seen.
#[derive(Default)]
struct Held(u64);
impl LiveReader for Reader {
    type Held = Held;
    type Read = u64;
    fn id(&self) -> &'static str {
        "probe.routes"
    }
    fn schedule(&self, db: &Store, dsp: &str) -> Result<Option<Schedule>> {
        let times: Times = settings::read(db, dsp, &WINDOW)?;
        let days = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
        let every = Duration::from_secs(times.every * 60);
        Schedule::new(&days, &times.start, &times.stop, every).map(Some)
    }
    fn read<'a>(&'a self, reading: &'a Reading, held: &'a mut Held) -> Pending<'a, u64> {
        Box::pin(async move {
            let (probe, script) = script(&reading.dsp);
            held.0 += 1;
            probe.saw(&reading.dsp, Seen::Read(held.0, Instant::now()));
            let mut flight = Flight::new(&probe, &reading.dsp);
            match script {
                Script::Gated => drop(probe.gate.acquire().await.unwrap()),
                Script::Browses => {
                    let open = reading.browser(PROVIDER).await?;
                    let session = reading.state.browsers.get_for(&reading.dsp, PROVIDER);
                    let second = opening(reading).await;
                    let (open_now, busy, revision) = session.map_or((0, false, 0), |session| {
                        (
                            reading.state.browsers.active(),
                            session.busy(),
                            session.revision,
                        )
                    });
                    let browsers = &reading.state.browsers;
                    browsers
                        .revoke_provider_revision(&reading.dsp, revision, PROVIDER)
                        .await;
                    let after_a_job = browsers.get_for(&reading.dsp, PROVIDER).is_some();
                    probe.saw(
                        &reading.dsp,
                        Seen::Browser {
                            open: open_now,
                            busy,
                            second,
                            after_a_job,
                        },
                    );
                    open.close().await;
                }
                Script::CutsOff => {
                    let signing_in = reading.browser(PROVIDER);
                    let gave_up = tokio::time::timeout(Duration::from_millis(100), signing_in);
                    assert!(gave_up.await.is_err(), "signing in takes longer");
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    let left = reading.state.browsers.get_for(&reading.dsp, PROVIDER);
                    let again = opening(reading).await;
                    probe.saw(
                        &reading.dsp,
                        Seen::CutOff {
                            left: left.is_some(),
                            again,
                        },
                    );
                }
                Script::Fails => return Err(Error::new("probe_unavailable", 503)),
                Script::MemoryBusy => return Err(Error::new("browser_memory_busy", 503)),
                Script::MemoryBusyOnce => {
                    set_script(&reading.dsp, Script::Reads);
                    return Err(Error::new("browser_memory_busy", 503));
                }
                Script::Reads | Script::Hangs | Script::KeepsBrowsing | Script::FailsToStop => {}
            }
            flight.landed = true;
            Ok(held.0)
        })
    }
    fn started<'a>(&'a self, reading: &'a Reading, _: &'a u64) -> Pending<'a, ()> {
        Box::pin(async move {
            script(&reading.dsp)
                .0
                .saw(&reading.dsp, Seen::Started(reading.day));
            Ok(())
        })
    }
    fn kept<'a>(&'a self, reading: &'a Reading, read: u64) -> Pending<'a, Counts> {
        Box::pin(async move {
            let (probe, script) = script(&reading.dsp);
            if script == Script::Hangs {
                set_script(&reading.dsp, Script::Reads);
                let _hang = Hang(probe.clone(), reading.dsp.clone());
                std::future::pending::<()>().await;
            }
            if script == Script::KeepsBrowsing {
                let _open = reading.browser(PROVIDER).await?;
                drop(probe.gate.acquire().await.unwrap());
                let open = reading.state.browsers.get_for(&reading.dsp, PROVIDER);
                let closed = open.is_none();
                probe.saw(&reading.dsp, Seen::KeptBrowsing { closed });
            }
            probe.saw(&reading.dsp, Seen::Kept(read));
            Ok(vec![("reads", read)])
        })
    }
    fn stopped<'a>(&'a self, reading: &'a Reading) -> Pending<'a, ()> {
        Box::pin(async move {
            let (probe, script) = script(&reading.dsp);
            if script == Script::FailsToStop {
                return Err(Error::new("probe_stop_failed", 503));
            }
            probe.saw(&reading.dsp, Seen::Stopped(reading.day));
            Ok(())
        })
    }
}

// The stand-in collector, whose browser a hook opens: in fixture mode, signed in by the
// password its connection keeps.

const PROVIDER: Provider = Provider::new("probe_site");
const DATABASE: Kind = Kind::new("probe_site", 1);
const MIGRATIONS: &[Migrations] = &[Migrations {
    kind: DATABASE,
    list: &[Migration {
        id: 1,
        name: "baseline",
        apply: Apply::Sql(
            "CREATE TABLE connections (provider TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 0, \
             status TEXT NOT NULL DEFAULT 'not_connected', error TEXT, account_label TEXT, \
             updated_at TEXT NOT NULL, verified_at TEXT, revision INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE storage_identity (dsp_id TEXT NOT NULL, provider TEXT NOT NULL, \
             source TEXT NOT NULL);
             CREATE TABLE collection_live_runs (job_id TEXT PRIMARY KEY, owner TEXT NOT NULL, \
             metadata TEXT NOT NULL);
             CREATE TABLE collection_live_items (job_id TEXT NOT NULL, item_key TEXT NOT NULL, \
             date TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(job_id,date,item_key));",
        ),
    }],
}];
struct Site;
static SITE: Site = Site;
impl Collector for Site {
    fn id(&self) -> &'static str {
        PROVIDER.id()
    }
    fn label(&self) -> &'static str {
        "Probe site"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[]
    }
    fn collections(&self) -> &'static [Collection] {
        &[]
    }
    fn database(&self) -> Kind {
        DATABASE
    }
    fn migrations(&self) -> &'static [Migrations] {
        MIGRATIONS
    }
    fn seed(&self, dsp: &str) -> String {
        format!(
            "{}\n{}",
            registry::identity_seed(dsp, PROVIDER, "probe-site"),
            registry::connection_seed(PROVIDER)
        )
    }
    fn marker(&self) -> Option<&'static str> {
        None
    }
    fn browser_entries(&self) -> &'static [&'static str] {
        &[]
    }
    fn network(&self) -> browseros::NetworkPolicy {
        unreachable!("no browser runs in fixture mode")
    }
    fn validate_credentials(&self, _: &Value) -> Result<()> {
        Ok(())
    }
    fn driver<'a>(
        &self,
        _: browseros::Session,
        _: &'a Path,
        _: Option<&'a str>,
    ) -> Pending<'a, Box<dyn browser::Driver>> {
        unreachable!("no browser runs in fixture mode")
    }
    fn fixture(&self, _: &str, _: &Value) -> Result<Collected> {
        Err(Error::new("nothing_collected", 500))
    }
    fn progress(&self, _: &Value) -> &'static str {
        "Reading"
    }
}

// The server under test, its time moved by the test.

/// The time as the readers keep it: the system's, moved on as the paused timers are, and
/// not by the real time the test spends meanwhile.
fn now() -> i64 {
    let (timers, system) = (Instant::now().into_std(), std::time::Instant::now());
    let ahead = match timers.checked_duration_since(system) {
        Some(ahead) => ahead.as_millis() as i64,
        None => -(system.duration_since(timers).as_millis() as i64),
    };
    db::now() + ahead
}

struct Server {
    _root: tempfile::TempDir,
    state: Arc<State>,
    owner: String,
    /// Where every DSP of the test is: where it's late morning now, so a test's windows keep
    /// to one day.
    zone: Tz,
    probe: Arc<Probe>,
    revisions: Mutex<HashMap<String, i64>>,
    stop: tokio::sync::watch::Sender<bool>,
    readers: Option<JoinHandle<Result<()>>>,
    /// When the readers last started, which plan again each minute from then.
    since: Instant,
}
impl Server {
    async fn start() -> Self {
        testing::install(&[&SITE], &[&PROBE]);
        // The first DSP, a preview's, has every feature on but this one, as it reads live: it
        // reads only in the DSPs a test makes.
        let (root, store, _) = testing::bootstrapped();
        let owner = testing::platform_owner(&store);
        let state = State::new(store.config.clone()).unwrap();
        drop(store);
        let hour = DateTime::from_timestamp_millis(now()).unwrap().hour() as i32;
        // Etc/GMT-N is N hours ahead of UTC: here it's eleven, and some minutes.
        let ahead = 11 - hour;
        let zone = format!("Etc/GMT{:+}", -ahead).parse().unwrap();
        let (stop, _) = tokio::sync::watch::channel(false);
        let mut server = Self {
            _root: root,
            state,
            owner,
            zone,
            probe: Arc::new(Probe::new()),
            revisions: Mutex::new(HashMap::new()),
            stop,
            readers: None,
            since: Instant::now(),
        };
        server.run_readers();
        server
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
    /// A new DSP of the test's, with the feature off, reading as `script` says once it's on.
    async fn dsp(&self, script: Script) -> String {
        let (owner, zone) = (self.owner.clone(), self.zone.name());
        let dsp = self
            .state
            .run(move |db| Ok(db.new_dsp("Probe DSP", zone, &owner, false)?.id))
            .await
            .unwrap();
        scripts().push((dsp.clone(), self.probe.clone(), script));
        dsp
    }
    async fn switch(&self, dsp: &str, on: bool) {
        let (dsp, owner) = (dsp.to_owned(), self.owner.clone());
        self.state
            .run(move |db| db.set_feature(&dsp, "probe", on, &owner).map(drop))
            .await
            .unwrap();
    }
    /// Saves the DSP's window, its two times as the clock shows them where it is, to read
    /// every five minutes.
    async fn window(&self, dsp: &str, start: &str, stop: &str) {
        self.schedule(dsp, start, stop, EVERY).await;
    }
    /// Saves the DSP's window, to read every `every`, a whole number of minutes.
    async fn schedule(&self, dsp: &str, start: &str, stop: &str, every: Duration) {
        let revision = *self.revisions.lock().unwrap().get(dsp).unwrap_or(&0);
        let (at, owner) = (dsp.to_owned(), self.owner.clone());
        let values = json!({"start":start,"stop":stop,"every":every.as_secs() / 60});
        let saved = self
            .state
            .run(move |db| settings::save(db, &at, &owner, &WINDOW, &values, revision, true))
            .await
            .unwrap();
        let Saved::Saved { revision, .. } = saved else {
            panic!("a window is saved without asking")
        };
        self.revisions
            .lock()
            .unwrap()
            .insert(dsp.to_owned(), revision);
    }
    /// The local time `minutes` whole minutes past the current one where the DSPs are, and when
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
    fn today(&self) -> NaiveDate {
        DateTime::from_timestamp_millis(now())
            .unwrap()
            .with_timezone(&self.zone)
            .date_naive()
    }
    /// Connects the DSP to the stand-in collector, its password saying how signing in goes.
    async fn connect(&self, dsp: &str, password: &str) {
        let credentials = json!({"username":"probe","password":password});
        let at = dsp.to_owned();
        self.state
            .run(move |db| testing::connect(db, &at, PROVIDER, &credentials))
            .await
            .unwrap();
    }
    /// The DSP's reads as the server keeps them, in order: each one's outcome and code.
    async fn records(&self, dsp: &str) -> Vec<(String, Option<String>)> {
        let dsp = dsp.to_owned();
        self.state
            .read(move |db| {
                db.jobs.query_as(
                    "SELECT outcome,code FROM live_reads WHERE dsp_id=? ORDER BY at",
                    [dsp],
                )
            })
            .await
            .unwrap()
    }
    /// How the DSP's reading goes, as Settings shows it.
    async fn reading(&self, dsp: &str) -> Option<LiveReading> {
        let dsp = dsp.to_owned();
        self.state
            .read(move |db| readers::reading(db, &db.find_dsp(&dsp)?, "probe.routes"))
            .await
            .unwrap()
    }
    /// Half a minute after one of the readers' own plans, so what follows within seconds
    /// follows from a wake alone.
    fn between_plans(&self, after: Instant) -> Instant {
        let minutes = (after - self.since).as_secs() / 60 + 1;
        self.since + Duration::from_secs(minutes * 60 + 30)
    }
}
async fn until(at: Instant) {
    tokio::time::sleep_until(at).await;
}
fn near(at: Instant, expected: Instant) -> bool {
    let apart = if at > expected {
        at - expected
    } else {
        expected - at
    };
    apart <= Duration::from_secs(1)
}
/// When each of `reads` was, near each `expected`, as many.
fn all_near(reads: &[(u64, Instant)], expected: &[Instant]) -> bool {
    reads.len() == expected.len()
        && (reads.iter().zip(expected)).all(|((_, at), expected)| near(*at, *expected))
}
/// The instant an answer names, on the time the test moves.
fn instant(at: &str) -> Instant {
    let at = DateTime::parse_from_rfc3339(at).unwrap().timestamp_millis();
    let ahead = at - now();
    let now = Instant::now();
    if ahead >= 0 {
        now + Duration::from_millis(ahead as u64)
    } else {
        now - Duration::from_millis(-ahead as u64)
    }
}
/// A record of a read that failed with `code`.
fn failed(code: &str) -> (String, Option<String>) {
    ("failed".to_owned(), Some(code.to_owned()))
}
fn kept() -> (String, Option<String>) {
    ("kept".to_owned(), None)
}

#[tokio::test(start_paused = true)]
async fn a_dsp_reads_on_its_grid_through_its_window_and_each_day_starts_and_stops() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Reads).await;
    let ((start, open), (stop, close)) = (server.at(10), server.at(40));
    server.window(&dsp, &start, &stop).await;
    server.switch(&dsp, true).await;
    let day = server.today();

    until(close + 6 * MINUTE).await;
    let reads = server.probe.reads(&dsp);
    let (first, last) = (reads[0].1, reads.last().unwrap().1);
    assert!(near(first, open), "its first slot is its window's start");
    for pair in reads.windows(2) {
        assert!(near(pair[1].1, pair[0].1 + EVERY), "a read every interval");
    }
    assert!(
        last < close && last + EVERY >= close,
        "every slot until the close"
    );
    assert_eq!(reads.len(), 6);
    // The day starts before its first read is kept, and stops once, after its window ends.
    let seen = server.probe.of(&dsp);
    assert_eq!(
        seen[..3],
        [Seen::Read(1, first), Seen::Started(day), Seen::Kept(1)]
    );
    assert_eq!(seen.last(), Some(&Seen::Stopped(day)));
    assert_eq!(
        seen.iter()
            .filter(|seen| matches!(seen, Seen::Started(_) | Seen::Stopped(_)))
            .count(),
        2
    );

    // The next day starts anew, with its memory made anew.
    until(open + Duration::from_secs(24 * 60 * 60) + EVERY).await;
    let tomorrow = day.succ_opt().unwrap();
    let seen = server.probe.of(&dsp);
    let next = seen
        .iter()
        .position(|seen| *seen == Seen::Stopped(day))
        .unwrap()
        + 1;
    assert!(matches!(seen[next], Seen::Read(1, _)));
    assert_eq!(
        seen[next + 1..next + 3],
        [Seen::Started(tomorrow), Seen::Kept(1)]
    );
}

#[tokio::test(start_paused = true)]
async fn started_again_a_dsp_keeps_its_slot_its_day_and_stops_a_day_it_missed() {
    let mut server = Server::start().await;
    let dsp = server.dsp(Script::Reads).await;
    let ((start, open), (stop, close)) = (server.at(5), server.at(30));
    server.window(&dsp, &start, &stop).await;
    server.switch(&dsp, true).await;
    until(open + 2 * EVERY + MINUTE).await;
    let last = server.probe.reads(&dsp).last().unwrap().1;

    // Down for a minute, it reads at its own next slot, not as it's back, and its day goes
    // on: nothing starts it again.
    until(last + MINUTE).await;
    server.stop_readers().await;
    until(last + 2 * MINUTE).await;
    server.run_readers();
    until(last + EVERY + MINUTE).await;
    let reads = server.probe.reads(&dsp);
    let after = reads.iter().find(|(_, at)| *at > last).unwrap();
    assert!(
        near(after.1, last + EVERY),
        "its next slot, not the restart"
    );
    assert_eq!(after.0, 1, "its memory is made anew");

    // Down before the window closed and back after, it stops the day as it's back.
    server.stop_readers().await;
    until(close + 20 * MINUTE).await;
    assert!(
        !server
            .probe
            .of(&dsp)
            .contains(&Seen::Stopped(server.today()))
    );
    server.run_readers();
    until(close + 21 * MINUTE).await;
    let seen = server.probe.of(&dsp);
    let day = server.today();
    assert_eq!(
        seen.iter()
            .filter(|seen| **seen == Seen::Started(day))
            .count(),
        1
    );
    assert_eq!(seen.last(), Some(&Seen::Stopped(day)));
}

#[tokio::test(start_paused = true)]
async fn a_day_whose_stop_began_reads_no_more_though_its_window_moves_later() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Reads).await;
    let ((start, open), (stop, close)) = (server.at(5), server.at(30));
    server.window(&dsp, &start, &stop).await;
    server.switch(&dsp, true).await;
    until(open + EVERY).await;
    // Its stop fails each time it's asked, as one that emptied what the day kept and failed
    // after; then its stop is moved later, opening its window again.
    set_script(&dsp, Script::FailsToStop);
    until(close + EVERY).await;
    let (later, reopened_close) = server.at(30);
    server.window(&dsp, &start, &later).await;
    until(close + 3 * EVERY).await;
    // The day reads no more: its stop is finished first, once it can be.
    let read_in_window = |server: &Server| {
        let reads = server.probe.reads(&dsp);
        reads.iter().all(|(_, at)| *at < close)
    };
    assert!(read_in_window(&server));
    let day = server.today();
    assert!(!server.probe.of(&dsp).contains(&Seen::Stopped(day)));
    set_script(&dsp, Script::Reads);
    until(close + 3 * EVERY + 2 * MINUTE).await;
    assert_eq!(server.probe.of(&dsp).last(), Some(&Seen::Stopped(day)));
    until(reopened_close + EVERY).await;
    assert!(read_in_window(&server));
    let stopped = server.probe.of(&dsp);
    let stopped = stopped.iter().filter(|seen| **seen == Seen::Stopped(day));
    assert_eq!(stopped.count(), 1);
}

#[tokio::test(start_paused = true)]
async fn no_more_reads_run_at_once_than_the_server_allows() {
    let server = Server::start().await;
    let (start, open) = server.at(5);
    let (stop, _) = server.at(60);
    let mut dsps = Vec::new();
    for _ in 0..READS_AT_ONCE + 2 {
        let dsp = server.dsp(Script::Gated).await;
        server.window(&dsp, &start, &stop).await;
        server.switch(&dsp, true).await;
        dsps.push(dsp);
    }
    // Every DSP's first slot has come, and no read has ended.
    until(open + EVERY - Duration::from_secs(1)).await;
    let started: usize = dsps.iter().map(|dsp| server.probe.reads(dsp).len()).sum();
    assert_eq!(started, READS_AT_ONCE);
    assert_eq!(server.probe.flying.load(Ordering::SeqCst), READS_AT_ONCE);
    // As reads end, those waiting have their turn.
    server.probe.gate.add_permits(1000);
    until(open + EVERY).await;
    let started: usize = dsps.iter().map(|dsp| server.probe.reads(dsp).len()).sum();
    assert!(started > READS_AT_ONCE);
    assert_eq!(server.probe.most.load(Ordering::SeqCst), READS_AT_ONCE);
}

#[tokio::test(start_paused = true)]
async fn a_days_stop_waits_its_turn_among_the_reads_and_still_happens() {
    let server = Server::start().await;
    let (opened, _) = server.at(-30);
    // Its window closes within two minutes, its day started.
    let stops = server.dsp(Script::Reads).await;
    let (closes, close) = server.at(2);
    server.window(&stops, &opened, &closes).await;
    server.switch(&stops, true).await;
    tokio::time::sleep(SECOND).await;
    assert_eq!(server.records(&stops).await, [kept()]);
    // Every turn is held by a read that waits on its provider as the window closes.
    let (stop, _) = server.at(60);
    for _ in 0..READS_AT_ONCE {
        let dsp = server.dsp(Script::Gated).await;
        server.window(&dsp, &opened, &stop).await;
        server.switch(&dsp, true).await;
    }
    until(close + MINUTE).await;
    let day = server.today();
    assert!(!server.probe.of(&stops).contains(&Seen::Stopped(day)));
    // As the reads end, its turn comes, and the day stops.
    server.probe.gate.add_permits(READS_AT_ONCE);
    tokio::time::sleep(SECOND).await;
    assert_eq!(server.probe.of(&stops).last(), Some(&Seen::Stopped(day)));
}

#[tokio::test(start_paused = true)]
async fn a_read_the_browsers_memory_turns_away_is_tried_once_more_within_its_slot() {
    let server = Server::start().await;
    let (recovers, stays_busy, every_minute) = (
        server.dsp(Script::MemoryBusyOnce).await,
        server.dsp(Script::MemoryBusy).await,
        server.dsp(Script::MemoryBusy).await,
    );
    let (start, open) = server.at(5);
    let (stop, _) = server.at(30);
    server.window(&recovers, &start, &stop).await;
    // Its window closes a minute after its second slot.
    let (closes, _) = server.at(11);
    server.window(&stays_busy, &start, &closes).await;
    // Its next slot is a minute after each.
    let (three_slots, _) = server.at(8);
    server
        .schedule(&every_minute, &start, &three_slots, MINUTE)
        .await;
    for dsp in [&recovers, &stays_busy, &every_minute] {
        server.switch(dsp, true).await;
    }
    until(open + 2 * EVERY + MINUTE).await;

    // Tried again a minute on, it's kept, and the slot keeps that read's record alone.
    let slots = [open, open + MINUTE, open + EVERY, open + 2 * EVERY];
    assert!(all_near(&server.probe.reads(&recovers), &slots));
    assert_eq!(server.records(&recovers).await, [kept(), kept(), kept()]);
    // Busy again, the slot keeps the retry's failure; it's tried once only, and not once a
    // minute on is its window's close.
    let slots = [open, open + MINUTE, open + EVERY];
    assert!(all_near(&server.probe.reads(&stays_busy), &slots));
    let busy = failed("browser_memory_busy");
    assert_eq!(server.records(&stays_busy).await, [busy.clone(), busy.clone()]);
    // Nor once a minute on is its next slot.
    let slots = [open, open + MINUTE, open + 2 * MINUTE];
    assert!(all_near(&server.probe.reads(&every_minute), &slots));
    assert_eq!(server.records(&every_minute).await.len(), 3);
}

#[tokio::test(start_paused = true)]
async fn a_read_waits_its_turn_and_runs_five_minutes_at_most_however_seldom_it_reads() {
    let server = Server::start().await;
    let (opened, _) = server.at(-30);
    let (stop, _) = server.at(180);
    // Every turn is held by a DSP reading hourly whose keeping never ends.
    let mut hung = Vec::new();
    for _ in 0..READS_AT_ONCE {
        let dsp = server.dsp(Script::Hangs).await;
        server.schedule(&dsp, &opened, &stop, HOUR).await;
        server.switch(&dsp, true).await;
        hung.push(dsp);
    }
    tokio::time::sleep(SECOND).await;
    let held = Instant::now();
    let waits = server.dsp(Script::Reads).await;
    server.schedule(&waits, &opened, &stop, HOUR).await;
    server.switch(&waits, true).await;
    // Its turn not come in five minutes, though it reads hourly, it misses its slot.
    until(held + LONGEST + SECOND).await;
    assert_eq!(server.records(&waits).await, [failed("live_reads_busy")]);
    // Three of those on, each hung DSP is started again, and reads on.
    until(held + 3 * LONGEST + 2 * MINUTE).await;
    for dsp in &hung {
        assert!(matches!(
            server.probe.of(dsp)[..],
            [
                Seen::Read(1, _),
                Seen::Started(_),
                Seen::Abandoned,
                Seen::Read(1, _),
                Seen::Kept(1)
            ]
        ));
    }
    // A read that never ends is given up on in five minutes too.
    let hangs = server.dsp(Script::Gated).await;
    server.schedule(&hangs, &opened, &stop, HOUR).await;
    server.switch(&hangs, true).await;
    let read = Instant::now();
    until(read + LONGEST + SECOND).await;
    assert_eq!(server.records(&hangs).await, [failed("live_read_timeout")]);
}

#[tokio::test(start_paused = true)]
async fn a_dsp_whose_reads_stop_moving_is_started_again_alone() {
    let server = Server::start().await;
    let (stalls, unplugged, reads) = (
        server.dsp(Script::Hangs).await,
        server.dsp(Script::Hangs).await,
        server.dsp(Script::Reads).await,
    );
    let (start, open) = server.at(5);
    let (stop, _) = server.at(120);
    for dsp in [&stalls, &unplugged, &reads] {
        server.window(dsp, &start, &stop).await;
        server.switch(dsp, true).await;
    }
    // Switched off as it hangs, its keeping is let go of all the same: its turn isn't held.
    until(open + EVERY).await;
    server.switch(&unplugged, false).await;
    until(open + 7 * EVERY).await;
    let let_go = server.probe.of(&unplugged);
    assert!(matches!(
        let_go[..],
        [Seen::Read(1, _), Seen::Started(_), Seen::Abandoned]
    ));
    // Its first read was never kept: three intervals on, it read again at once, with its
    // memory made anew, and goes on from there.
    let stalled = server.probe.of(&stalls);
    assert!(matches!(
        stalled[..5],
        [
            Seen::Read(1, _),
            Seen::Started(_),
            Seen::Abandoned,
            Seen::Read(1, _),
            Seen::Kept(1)
        ]
    ));
    let again = server.probe.reads(&stalls);
    assert!(again[1].1 - again[0].1 > 3 * EVERY);
    assert_eq!(again[2].0, 2);
    // The other read on throughout, its memory kept.
    let other: Vec<u64> = server
        .probe
        .reads(&reads)
        .iter()
        .map(|(held, _)| *held)
        .collect();
    assert_eq!(other, (1..=other.len() as u64).collect::<Vec<_>>());
    assert!(other.len() >= 6);
}

#[tokio::test(start_paused = true)]
async fn switched_off_mid_window_a_dsp_stops_at_once_and_switched_on_goes_on_with_its_day() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Reads).await;
    let (start, open) = server.at(5);
    let (stop, _) = server.at(120);
    server.window(&dsp, &start, &stop).await;
    server.switch(&dsp, true).await;
    until(open + 2 * EVERY + MINUTE).await;
    // Its next read waits on its provider as the feature is switched off: it's dropped, and
    // no other starts.
    set_script(&dsp, Script::Gated);
    let read = server.probe.reads(&dsp).len();
    tokio::time::sleep(EVERY).await;
    assert!(matches!(server.probe.of(&dsp).last(), Some(Seen::Read(..))));
    server.switch(&dsp, false).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(server.probe.of(&dsp).last(), Some(&Seen::Dropped));
    tokio::time::sleep(5 * EVERY).await;
    assert_eq!(server.probe.reads(&dsp).len(), read + 1);
    let records = server.records(&dsp).await;
    assert_eq!(records.len(), read + 1);
    assert_eq!(records.last(), Some(&failed("live_read_stopped")));

    // Switched on again, it reads at once, going on with its day.
    set_script(&dsp, Script::Reads);
    server.switch(&dsp, true).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let seen = server.probe.of(&dsp);
    assert!(matches!(seen[seen.len() - 2], Seen::Read(1, _)));
    assert_eq!(seen.last(), Some(&Seen::Kept(1)));
    let started = seen.iter().filter(|seen| matches!(seen, Seen::Started(_)));
    assert_eq!(started.count(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_switch_or_a_saved_window_takes_effect_at_once() {
    let server = Server::start().await;
    let (switched, saved) = (
        server.dsp(Script::Reads).await,
        server.dsp(Script::Reads).await,
    );
    // Open since half an hour ago, its first slot has passed: switched on, it reads at once.
    let (opened, _) = server.at(-30);
    let (stop, _) = server.at(90);
    server.window(&switched, &opened, &stop).await;
    until(server.between_plans(Instant::now())).await;
    server.switch(&switched, true).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(server.probe.reads(&switched).len(), 1);
    // Switched on with a window still to come, it reads as soon as an open one is saved.
    let (later, _) = server.at(60);
    server.window(&saved, &later, &stop).await;
    server.switch(&saved, true).await;
    until(server.between_plans(Instant::now() + 5 * MINUTE)).await;
    assert!(server.probe.reads(&saved).is_empty());
    server.window(&saved, &opened, &stop).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(server.probe.reads(&saved).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_hooks_browser_is_its_own_within_the_servers_limits_and_closes_after_it() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Browses).await;
    server.connect(&dsp, "fixture-password").await;
    let (start, open) = server.at(5);
    let (stop, _) = server.at(60);
    server.window(&dsp, &start, &stop).await;
    server.switch(&dsp, true).await;
    until(open + EVERY).await;
    // Open, it's the server's one browser, which no job takes nor closes as it ends; another
    // opening is refused.
    let browser = Seen::Browser {
        open: 1,
        busy: true,
        second: "connection_busy".into(),
        after_a_job: true,
    };
    assert_eq!(server.probe.of(&dsp)[1], browser);
    assert_eq!(server.probe.of(&dsp)[3], Seen::Kept(1));
    assert_eq!(server.state.browsers.active(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_hooks_browser_leaves_nothing_behind_cut_off_and_closes_as_its_reader_stops() {
    let server = Server::start().await;
    let (cuts, keeps) = (
        server.dsp(Script::CutsOff).await,
        server.dsp(Script::KeepsBrowsing).await,
    );
    // Signing in takes a quarter of a second.
    server.connect(&cuts, "slow-valid-password").await;
    server.connect(&keeps, "fixture-password").await;
    let (start, open) = server.at(5);
    let (stop, _) = server.at(60);
    for dsp in [&cuts, &keeps] {
        server.window(dsp, &start, &stop).await;
        server.switch(dsp, true).await;
    }
    until(open + EVERY).await;
    // Given up on as it signed in, nothing of it is left to refuse the next.
    let cut = Seen::CutOff {
        left: false,
        again: "opened".into(),
    };
    assert_eq!(server.probe.of(&cuts)[1], cut);
    // Switched off while a keeping holds its browser, the browser closes under it.
    server.switch(&keeps, false).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    server.probe.gate.add_permits(1);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let kept = server.probe.of(&keeps);
    assert_eq!(kept[kept.len() - 2], Seen::KeptBrowsing { closed: true });
}

#[tokio::test(start_paused = true)]
async fn a_read_whose_turn_comes_after_its_window_closed_never_starts() {
    let server = Server::start().await;
    let (opened, _) = server.at(-30);
    let (stop, _) = server.at(60);
    // Every turn to read is held by a read that waits on its provider.
    for _ in 0..READS_AT_ONCE {
        let dsp = server.dsp(Script::Gated).await;
        server.window(&dsp, &opened, &stop).await;
        server.switch(&dsp, true).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(server.probe.flying.load(Ordering::SeqCst), READS_AT_ONCE);
    // Its window closes within the minute, as it waits for its turn.
    let late = server.dsp(Script::Reads).await;
    let (closes, close) = server.at(1);
    server.window(&late, &opened, &closes).await;
    server.switch(&late, true).await;
    until(close + MINUTE).await;
    server.probe.gate.add_permits(READS_AT_ONCE);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(server.probe.flying.load(Ordering::SeqCst) < READS_AT_ONCE);
    assert!(server.probe.reads(&late).is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_dsps_records_go_after_two_weeks_though_none_of_its_reads_was_kept() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Fails).await;
    let (start, open) = server.at(5);
    let (stop, _) = server.at(60);
    server.window(&dsp, &start, &stop).await;
    let day = 24 * 60 * 60 * 1000;
    let (at, older) = (dsp.clone(), now());
    server
        .state
        .run_bookkeeping(move |db| {
            for (ago, code) in [(15, "too_old"), (13, "kept_on")] {
                db.jobs.exec(
                    "INSERT INTO live_reads(reader,dsp_id,day,at,ms,outcome,code) \
                     VALUES ('probe.routes',?,'2026-01-01',?,0,'failed',?)",
                    rusqlite::params![at, older - ago * day, code],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    server.switch(&dsp, true).await;
    until(open + MINUTE).await;
    let records = server.records(&dsp).await;
    assert_eq!(records, [failed("kept_on"), failed("probe_unavailable")]);
}

#[tokio::test(start_paused = true)]
async fn a_dsps_reading_shows_its_last_read_its_next_and_its_last_error_while_it_reads() {
    let server = Server::start().await;
    let dsp = server.dsp(Script::Fails).await;
    let ((start, open), (stop, close)) = (server.at(5), server.at(30));
    server.window(&dsp, &start, &stop).await;
    // Off, it reads nothing there.
    assert!(server.reading(&dsp).await.is_none());
    server.switch(&dsp, true).await;
    let before = server.reading(&dsp).await.unwrap();
    assert!(before.last_read.is_none() && before.last_error.is_none());
    assert!(near(instant(&before.next_read), open));

    // Its first read fails, the next is kept: its next is the slot after.
    until(open + MINUTE).await;
    set_script(&dsp, Script::Reads);
    until(open + EVERY + MINUTE).await;
    let reading = server.reading(&dsp).await.unwrap();
    let last = reading.last_read.unwrap();
    assert!(last.kept && near(instant(&last.at), open + EVERY));
    let error = reading.last_error.unwrap();
    assert_eq!(error.code, "probe_unavailable");
    assert!(near(instant(&error.at), open));
    assert!(near(instant(&reading.next_read), open + 2 * EVERY));

    // Its day stopped, its next is tomorrow's start.
    until(close + MINUTE).await;
    let tomorrow = open + Duration::from_secs(24 * 60 * 60);
    let after = server.reading(&dsp).await.unwrap();
    assert!(near(instant(&after.next_read), tomorrow));
    server.switch(&dsp, false).await;
    assert!(server.reading(&dsp).await.is_none());
}
