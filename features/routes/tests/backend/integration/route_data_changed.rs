//! A day's route data changed, as every feature hears it once it's kept (`routes.data_changed`):
//! after a live read that changed it, live route data emptied, and the day saved, collected or
//! reprocessed. Made-up days only.
use dispatch_core::{
    State,
    collection::browser::Collected,
    db::{Store, s},
    manifest::{
        Feature, Upkeep,
        events::{self, CollectionSucceeded},
        feature, optional, registry,
    },
    testing as common,
};
use dispatch_cortex::{
    self as cortex,
    routes::{self, Capture, JOB_KIND, Mode, Request},
};
use dispatch_routes::RoutesStore;
use dispatch_routes_contract::RouteDataChanged;
use serde_json::json;
use std::sync::{Arc, Mutex};

/// A feature hearing a day's route data changed, on for a test's DSP as every feature is.
static HEARING: Feature = Feature {
    switch: optional("hearing", "Hearing", &[]),
    on: &[dispatch_core::on!(RouteDataChanged, changed)],
    ..feature("hearing")
};
/// Each DSP and day the stand-in heard of, once each time.
static HEARD: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
fn changed(_: Arc<State>, dsp: String, changed: RouteDataChanged) -> Upkeep {
    let mut heard = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    heard.push((dsp, changed.day));
    Box::pin(async {})
}
/// The days the stand-in heard of the DSP's route data changing since it was last asked, in order.
fn heard(dsp: &str) -> Vec<String> {
    let mut heard = HEARD.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut days = Vec::new();
    heard.retain(|(at, day)| {
        let theirs = at == dsp;
        if theirs {
            days.push(day.clone());
        }
        !theirs
    });
    days
}

/// A DSP with a station and Cortex connected, Routes on, with its server's state.
fn ready() -> (tempfile::TempDir, Store, String, Arc<State>) {
    common::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_routes::FEATURE, &HEARING],
    );
    let (root, db, dsp) = common::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &dsp, cortex::PROVIDER).unwrap();
    let state = State::new(db.config.clone()).unwrap();
    (root, db, dsp, state)
}
/// The bootstrapped DSP's day `back` days before today: it keeps UTC.
fn day(back: i64) -> String {
    (chrono::Utc::now().date_naive() - chrono::Duration::days(back)).to_string()
}
/// The fixture's read of `day`, at the DSP's station: final, or a snapshot of today.
fn read(day: &str, mode: Mode) -> Capture {
    let request = Request::parse(&json!({"collection":"routes","mode":mode.as_str(),"date":day,
        "station":"TST1","timezone":"UTC","dspName":"Test Owner","dspAbbreviation":"NLOG"}))
    .unwrap()
    .unwrap();
    routes::fixture(&request).unwrap()
}
/// Keeps `read` as a live reader does, under the DSP's Cortex connection as it stands.
async fn keep_live(state: &Arc<State>, dsp: &str, read: Capture) -> dispatch_routes::LiveKept {
    let d = dsp.to_owned();
    let revision = state
        .read(move |db| db.connection_revision(&d, cortex::PROVIDER))
        .await
        .unwrap()
        .expect("Cortex is connected");
    dispatch_routes::keep_live(state, dsp, read, revision)
        .await
        .unwrap()
}
/// Collects `capture` under `key` as the standard collection does: queued, staged and published
/// by Routes' keeper, as the executor publishes a job.
fn collect(db: &Store, dsp: &str, key: &str, capture: Capture) {
    let day = capture.scope.date.clone();
    let jobs = db
        .enqueue_routes(dsp, None, key, Some(&day), capture.mode, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    let staged = db.stage_routes(dsp, &job, capture).unwrap();
    let collected = Collected {
        data: serde_json::to_value(staged).unwrap(),
        scope: None,
    };
    let keeper = registry().keeper(JOB_KIND);
    keeper.publish(db, dsp, &job, collected).unwrap();
}
/// Routes' upkeep, as the scheduler offers it every minute.
async fn upkeep(state: &Arc<State>) {
    (dispatch_routes::FEATURE.maintenance[0].run)(state.clone(), false).await;
}

#[tokio::test]
async fn a_live_day_is_heard_of_once_a_read_changed_it_and_as_its_emptied_or_saved() {
    let (_root, db, dsp, state) = ready();
    let today = day(0);
    let first = read(&today, Mode::Snapshot);
    keep_live(&state, &dsp, first.clone()).await;
    assert_eq!(heard(&dsp), [today.clone()], "a first read");
    // Read again as it was, or before, it changed nothing.
    keep_live(&state, &dsp, first.clone()).await;
    let mut unchanged = first.clone();
    unchanged.finished_at += 1;
    assert_eq!(keep_live(&state, &dsp, unchanged.clone()).await.changed, 0);
    assert_eq!(heard(&dsp), Vec::<String>::new(), "reads changing nothing");
    let mut moved = unchanged;
    moved.finished_at += 1;
    moved.summaries["transporters"][0]["workPhoneNumber"] = json!("5550100003");
    keep_live(&state, &dsp, moved.clone()).await;
    assert_eq!(heard(&dsp), [today.clone()], "a read moving a phone");

    // Emptied; then read, saved and emptied as a stopped day is; then emptied holding none.
    dispatch_routes::clear_live(&state, &dsp).await.unwrap();
    keep_live(&state, &dsp, first).await;
    dispatch_routes::save_and_clear_live(&state, &dsp)
        .await
        .unwrap();
    dispatch_routes::clear_live(&state, &dsp).await.unwrap();
    assert_eq!(heard(&dsp), vec![today.clone(); 5]);

    // Another feature's read of the day before, saved: of that day, unless one the day has is later.
    let before = read(&day(1), Mode::Final);
    let saved = dispatch_routes::save_day(&state, &dsp, "before", before.clone(), Vec::new());
    assert!(saved.await.unwrap());
    let mut earlier = before;
    (earlier.started_at, earlier.finished_at) = (earlier.started_at - 1, earlier.finished_at - 1);
    let saved = dispatch_routes::save_day(&state, &dsp, "earlier", earlier, Vec::new());
    assert!(!saved.await.unwrap());
    assert_eq!(heard(&dsp), [day(1)]);
}

#[tokio::test]
async fn a_collected_or_reprocessed_day_is_heard_of_once_its_collection_succeeds_or_by_the_upkeep()
{
    let (_root, db, dsp, state) = ready();
    // Collected, it's told once the collection is: the executor publishes in its database work.
    collect(&db, &dsp, "first", read(&day(1), Mode::Final));
    assert_eq!(heard(&dsp), Vec::<String>::new());
    let succeeded = |keeps| CollectionSucceeded { keeps };
    events::publish(&state, &dsp, succeeded(JOB_KIND)).await;
    events::publish(&state, &dsp, succeeded(JOB_KIND)).await;
    assert_eq!(heard(&dsp), [day(1)], "told once");

    // One whose collection was never told succeeded, as when its job couldn't finish, and another
    // collection's success: told by the next upkeep.
    collect(&db, &dsp, "second", read(&day(2), Mode::Final));
    events::publish(&state, &dsp, succeeded(cortex::dvic::JOB_KIND)).await;
    assert_eq!(heard(&dsp), Vec::<String>::new());
    upkeep(&state).await;
    assert_eq!(heard(&dsp), [day(2)]);

    // A read collected before the day's own changes nothing, told of nothing.
    let mut earlier = read(&day(1), Mode::Final);
    (earlier.started_at, earlier.finished_at) = (earlier.started_at - 60_000, earlier.finished_at - 60_000);
    collect(&db, &dsp, "earlier", earlier);
    upkeep(&state).await;
    assert_eq!(heard(&dsp), Vec::<String>::new());

    // Reprocessed, as the platform owner asks: each day it rebuilt, by the next upkeep.
    db.reprocess_routes(&dsp, None).unwrap();
    upkeep(&state).await;
    assert_eq!(heard(&dsp), [day(2), day(1)]);
}
