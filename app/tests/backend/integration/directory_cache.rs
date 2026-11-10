//! Coherence across real live Routes, State, the directory and cached HTTP match counts.
//! A separate binary installs one registry with real owners and an event observer; it never
//! replaces the directory or changes the combined integration binary's registry.
#[allow(dead_code)]
mod support;
use dispatch_core::{
    Error, State,
    collection::browser::Collected,
    db::s,
    manifest::{
        Feature, Upkeep,
        events::{CollectionSucceeded, PeopleChanged},
        feature, optional,
        people::{self, Everyone},
        registry,
    },
    server::cache::{DataDomain, PeopleImpact, Scope},
    testing as common,
};
use dispatch_cortex::{
    self as cortex,
    routes::{self, Capture},
};
use dispatch_driver_match::DriverMatchStore;
use dispatch_routes::RoutesStore;
use dispatch_routes_contract::RouteDataChanged;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{Call, Server, Who};

static HEARING: Feature = Feature {
    switch: optional("directory_observer", "Observer", &[]),
    on: &[
        dispatch_core::on!(PeopleChanged, named),
        dispatch_core::on!(RouteDataChanged, day_changed),
    ],
    ..feature("directory_observer")
};
static HEARD: Mutex<Vec<(String, &'static str)>> = Mutex::new(Vec::new());
fn named(_: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    HEARD.lock().unwrap().push((dsp, "people"));
    Box::pin(async {})
}
fn day_changed(_: Arc<State>, dsp: String, _: RouteDataChanged) -> Upkeep {
    HEARD.lock().unwrap().push((dsp, "day"));
    Box::pin(async {})
}
fn heard(dsp: &str, event: &str) -> usize {
    HEARD
        .lock()
        .unwrap()
        .iter()
        .filter(|(d, e)| d == dsp && *e == event)
        .count()
}
async fn ready() -> (Server, String, String, Who, Capture) {
    let mut features = dispatch_backend::REGISTRY.features.to_vec();
    features.push(&HEARING);
    common::install_with(
        dispatch_backend::REGISTRY.agents,
        dispatch_backend::REGISTRY.collectors,
        &features,
    );
    let (server, a) = Server::start_installed(|db| {
        // Fresh tenant with all real contributors installed, but no demo route history
        // supplying a higher-priority spelling of this test's live IDs.
        let dsp = db
            .new_dsp("Directory Proof", "UTC", &common::a_user(db), false)
            .unwrap();
        db.enable_all_features(&dsp.id).unwrap();
        db.set_profile(
            &dsp.id,
            json!({"stationCode":"TST1","abbreviation":"PROOF","setupRequired":false}),
        )
        .unwrap();
        common::ready_connection(db, &dsp.id, cortex::PROVIDER).unwrap();
        dsp.id
    })
    .await;
    let b = server.dsp("Summit Delivery").await;
    let mut who = server.owner().await;
    let view = server
        .send(Call::post("/api/session/dsp", json!({"dspId":a})).who(&who))
        .await;
    assert_eq!(view.status, 200, "{}", view.body);
    who.view = Some(s(&view.body, "token").to_owned());
    let at = a.clone();
    let capture = server
        .state
        .read(move |db| routes::fixture(&dispatch_routes::live_request(db, &at)?))
        .await
        .unwrap();
    (server, a, b, who, capture)
}
fn later(read: &Capture) -> Capture {
    Capture {
        started_at: read.started_at + 1,
        finished_at: read.finished_at + 1,
        ..read.clone()
    }
}
async fn keep(
    state: &Arc<State>,
    dsp: &str,
    read: Capture,
) -> dispatch_core::Result<dispatch_routes::LiveKept> {
    let at = dsp.to_owned();
    let revision = state
        .read(move |db| db.connection_revision(&at, cortex::PROVIDER))
        .await?
        .unwrap();
    dispatch_routes::keep_live(state, dsp, read, revision).await
}
async fn everyone(state: &Arc<State>, dsp: &str) -> Arc<Everyone> {
    let (at, shared) = (dsp.to_owned(), state.clone());
    state
        .read(move |db| people::everyone(db, &shared, &at))
        .await
        .unwrap()
}
async fn counts(server: &Server, who: &Who) -> Value {
    let answer = server
        .send(Call::get("/api/dsp/driver-match/counts").who(who))
        .await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    answer.body
}
fn rename(read: &mut Capture, n: usize, first: Value) {
    read.summaries["transporters"][n]["firstName"] = first.clone();
    read.route_summaries["transporters"][n]["firstName"] = first.clone();
    if let Some(itinerary) = read.itineraries.get_mut(n) {
        let mut detail: Value = serde_json::from_str(&itinerary.detail).unwrap();
        detail["transporters"][0]["firstName"] = first;
        itinerary.detail = detail.to_string();
    }
}
fn surname(read: &mut Capture, n: usize, last: &str) {
    read.summaries["transporters"][n]["lastName"] = json!(last);
    read.route_summaries["transporters"][n]["lastName"] = json!(last);
    if let Some(itinerary) = read.itineraries.get_mut(n) {
        let mut detail: Value = serde_json::from_str(&itinerary.detail).unwrap();
        detail["transporters"][0]["lastName"] = json!(last);
        itinerary.detail = detail.to_string();
    }
}
fn name(everyone: &Everyone, id: &str) -> String {
    everyone.holding(cortex::SOURCE, id).unwrap().name.clone()
}
async fn live_pairs(state: &Arc<State>, dsp: &str) -> Vec<(String, i64)> {
    let at = dsp.to_owned();
    state.read(move |db| db.routes_live_db(&at)?.query_as(
        "SELECT transporter_id,count(DISTINCT day) FROM itineraries GROUP BY transporter_id ORDER BY transporter_id", []
    )).await.unwrap()
}
// An actual other cached consumer: the core Settings read's dependency predicate must run
// even on the proof's Unchanged branch. Its tiny value makes rebuilding observable.
async fn other_read(state: &Arc<State>, dsp: &str, load: i64) -> i64 {
    let (at, shared) = (dsp.to_owned(), state.clone());
    state
        .read(move |_| {
            shared.read_cache.read(
                Scope::tenant(dispatch_core::settings::CACHED, at),
                "directory.regression".into(),
                shared
                    .data_revision
                    .load(std::sync::atomic::Ordering::Relaxed),
                || Ok(load),
            )
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn live_proof_keeps_only_unchanged_people_and_still_refreshes_routes() {
    let (server, a, b, who, mut read) = ready().await;
    let state = &server.state;
    // Known names before itineraries arrive: the route list's assignment-only writer.
    let summaries = read.summaries["itinerarySummaries"].clone();
    let itineraries = read.itineraries.clone();
    read.summaries["itinerarySummaries"] = json!([]);
    read.itineraries.clear();
    keep(state, &a, read.clone()).await.unwrap();
    let mut before = everyone(state, &a).await;
    let second = everyone(state, &b).await;
    let initial_counts = counts(&server, &who).await;
    let initial_events = heard(&a, "people");
    assert_eq!(name(&before, "driver-1"), "Fixture Driver");
    assert_eq!(name(&before, "driver-2"), "Second Driver");
    assert_eq!(other_read(state, &a, 1).await, 1);
    read = later(&read);
    read.route_summaries["backendTime"] = json!(42);
    let kept = keep(state, &a, read.clone()).await.unwrap();
    assert!(kept.changed > 0);
    let at = a.clone();
    let raw = state
        .read(move |db| {
            let live = db.routes_live_db(&at)?;
            let body: Vec<u8> = live.0.query_row(
                "SELECT body FROM route_raw WHERE name='route_summaries'",
                [],
                |row| row.get(0),
            )?;
            Ok(serde_json::from_slice::<Value>(&dispatch_routes::gunzip(
                &body,
            )?)?)
        })
        .await
        .unwrap();
    assert_eq!(raw["backendTime"], 42);
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
    assert_eq!(counts(&server, &who).await, initial_counts);
    assert_eq!(other_read(state, &a, 2).await, 2);
    assert_eq!(heard(&a, "people"), initial_events);

    read = later(&read);
    rename(&mut read, 0, json!("Ada"));
    keep(state, &a, read.clone()).await.unwrap();
    let renamed = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &renamed));
    assert_eq!(name(&renamed, "driver-1"), "Ada Driver");
    assert_eq!(heard(&a, "people"), initial_events);
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
    before = renamed;
    // Blanks become SQL NULL through the production text conversion, overwriting names.
    for first in [json!("  "), json!("Grace"), Value::Null] {
        read = later(&read);
        rename(&mut read, 0, first.clone());
        keep(state, &a, read.clone()).await.unwrap();
        let after = everyone(state, &a).await;
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(
            name(&after, "driver-1"),
            if first == json!("Grace") {
                "Grace Driver"
            } else {
                "Driver"
            }
        );
        before = after;
    }
    // Duplicate assignment records retain input order. A temporary rename followed by
    // restoration is deliberately conservative, even though the final spelling is equal.
    read = later(&read);
    let original = read.route_summaries["transporters"].clone();
    let mut transient = original[0].clone();
    transient["firstName"] = json!("Transient");
    read.route_summaries["transporters"]
        .as_array_mut()
        .unwrap()
        .insert(0, transient);
    keep(state, &a, read.clone()).await.unwrap();
    let after = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(name(&after, "driver-1"), "Driver");
    before = after;
    read.route_summaries["transporters"] = original;
    // Itinerary rows now override assigned values; adding activity without a new driver
    // must invalidate even if names/Everyone fields would happen to be identical.
    read = later(&read);
    read.summaries["itinerarySummaries"] = summaries.clone();
    read.itineraries = itineraries.clone();
    rename(&mut read, 0, json!("Katherine"));
    keep(state, &a, read.clone()).await.unwrap();
    let active = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &active));
    assert_eq!(name(&active, "driver-1"), "Katherine Driver");
    assert_eq!(
        live_pairs(state, &a).await,
        [("driver-1".into(), 1), ("driver-2".into(), 1)]
    );
    before = active;
    let day_events = heard(&a, "day");
    read = later(&read);
    read.summaries["backendTime"] = json!(99);
    read.summaries["transporters"][0]["lastLocation"] = json!({"latitude":34.2,"longitude":-118.2});
    read.summaries["itinerarySummaries"][0]["completedLocations"] = json!(1);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert_eq!(heard(&a, "day"), day_events + 1);
    let at = a.clone();
    assert_eq!(
        state
            .read(move |db| {
                db.routes_live_db(&at)?.count(
                    "SELECT completed_locations FROM itineraries WHERE itinerary_id='itinerary-1'",
                    [],
                )
            })
            .await
            .unwrap(),
        1
    );
    // A second itinerary for one driver contributes no extra distinct active day.
    read = later(&read);
    let mut duplicate = read.summaries["itinerarySummaries"][0].clone();
    duplicate["itineraryId"] = json!("itinerary-extra");
    read.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let mut itinerary = read.itineraries[0].clone();
    itinerary.id = "itinerary-extra".into();
    let mut detail: Value = serde_json::from_str(&itinerary.detail).unwrap();
    detail["itineraryDetails"]["itineraryId"] = json!("itinerary-extra");
    itinerary.detail = detail.to_string();
    read.itineraries.push(itinerary);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert_eq!(
        live_pairs(state, &a).await,
        [("driver-1".into(), 1), ("driver-2".into(), 1)]
    );
    // Remove driver-2's activity while its assigned name remains in the driver table.
    read = later(&read);
    read.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    read.itineraries.remove(1);
    keep(state, &a, read.clone()).await.unwrap();
    let removed = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &removed));
    assert_eq!(live_pairs(state, &a).await, [("driver-1".into(), 1)]);
    assert_eq!(name(&removed, "driver-2"), "Second Driver");
    assert_eq!(heard(&a, "people"), initial_events);
    // Equal distinct counts with different active IDs are a semantic change too.
    read = later(&read);
    read.summaries["itinerarySummaries"] = json!([summaries[1]]);
    read.itineraries = vec![itineraries[1].clone()];
    read.route_summaries["transporters"][0]["firstName"] = json!("Katherine");
    keep(state, &a, read.clone()).await.unwrap();
    let swapped = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&removed, &swapped));
    assert_eq!(live_pairs(state, &a).await, [("driver-2".into(), 1)]);
    assert_eq!(name(&swapped, "driver-1"), "Katherine Driver");
    // The same public historical save still invalidates; it is not a proved live poll.
    assert_eq!(
        dispatch_routes::save_live_day(state, &a).await.unwrap(),
        Some(true)
    );
    let saved = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&swapped, &saved));
    dispatch_routes::clear_live(state, &a).await.unwrap();
    let cleared = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&saved, &cleared));
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
    // Authoritative source workdays survive clearing the live day: history still drove.
    let at = a.clone();
    let saved_day = read.scope.date.clone();
    let (worked, collected) = state
        .read(move |db| {
            let kind = dispatch_routes::FEATURE
                .people
                .iter()
                .find(|p| p.kind.id == "routes")
                .unwrap();
            let days = (kind.workdays.unwrap())(db, &at)?;
            Ok((days.worked, days.collected))
        })
        .await
        .unwrap();
    assert_eq!(worked, [("driver-2".into(), saved_day.clone())]);
    assert_eq!(collected, [saved_day]);
}

#[tokio::test]
async fn fallback_refusal_and_errors_keep_existing_coherence() {
    let (server, a, b, _who, mut read) = ready().await;
    let state = &server.state;
    keep(state, &a, read.clone()).await.unwrap();
    let second = everyone(state, &b).await;
    // Valid existing seen intervals, once ahead of this capture and once behind it.
    // Both updates are fixture setup through State before warming, not SQL behind a hot cache.
    for offset in [1_i64, -1] {
        let at = a.clone();
        state
            .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
                let live = db.routes_live_db(&at)?;
                let day = live.one("SELECT day FROM route_publications", [])?.unwrap();
                live.exec(
                    "UPDATE drivers SET first_seen_day=date(?1,?2),last_seen_day=date(?1,?2)",
                    rusqlite::params![s(&day, "day"), format!("{offset:+} day")],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let before = everyone(state, &a).await;
        read = later(&read);
        keep(state, &a, read.clone()).await.unwrap();
        assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
        let (at, day) = (a.clone(), read.scope.date.clone());
        let (first, last) = server
            .state
            .read(move |db| {
                let page = db.driver_match(&at)?;
                let id = page
                    .people
                    .iter()
                    .flat_map(|p| &p.ids)
                    .find(|id| id.id == "driver-1")
                    .unwrap();
                Ok((id.first_seen.clone(), id.last_seen.clone()))
            })
            .await
            .unwrap();
        let shifted = (chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").unwrap()
            + chrono::Duration::days(offset))
        .to_string();
        assert_eq!(
            (first, last),
            if offset > 0 {
                (Some(day), Some(shifted))
            } else {
                (Some(shifted), Some(day))
            }
        );
    }
    // An unsupported stored type makes the additional typed proof read fail. The existing
    // writer can still repair it; proof errors must conservatively evict, not reject the poll.
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
            db.routes_live_db(&at)?
                .exec("UPDATE drivers SET first_seen_day=x'01'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    let before = everyone(state, &a).await;
    read = later(&read);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
            db.routes_live_db(&at)?
                .exec("UPDATE route_publications SET adapter_version=0", [])?;
            Ok(())
        })
        .await
        .unwrap();
    let before = everyone(state, &a).await;
    read = later(&read);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
    // Older captures still do nothing to route data; their invalidation stays conservative.
    let before = everyone(state, &a).await;
    let old = keep(state, &a, read.clone()).await.unwrap();
    assert_eq!(old.changed, 0);
    assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
    // A held different day takes the actual reset branch, even with the same names.
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
            db.routes_live_db(&at)?
                .exec("UPDATE route_publications SET day=date(day,'-1 day')", [])?;
            Ok(())
        })
        .await
        .unwrap();
    let before = everyone(state, &a).await;
    read = later(&read);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
    // Real live SQL rollback: reject the ordinary itinerary update before warming.
    let at = a.clone();
    state.run_bookkeeping(move |db| {
        db.routes_live_db(&at)?.0.execute_batch(
            "CREATE TRIGGER refuse_itinerary BEFORE UPDATE ON itineraries BEGIN SELECT RAISE(ABORT,'fixture refusal'); END"
        )?; Ok(())
    }).await.unwrap();
    let before = everyone(state, &a).await;
    let mut rejected = later(&read);
    rejected.summaries["itinerarySummaries"][0]["completedLocations"] = json!(0);
    assert!(keep(state, &a, rejected).await.is_err());
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    // On MayChange the same rollback may evict, and must never resurrect the old Arc.
    read = later(&read);
    rename(&mut read, 0, json!("RolledBack"));
    let error = keep(state, &a, read.clone()).await.unwrap_err();
    assert_ne!(error.code, "routes_capture_invalid");
    let after = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(name(&after, "driver-1"), "Fixture Driver");
    let at = a.clone();
    state
        .run_bookkeeping(move |db| {
            db.routes_live_db(&at)?
                .0
                .execute_batch("DROP TRIGGER refuse_itinerary")?;
            Ok(())
        })
        .await
        .unwrap();
    // A real autocommitted name mutation followed by an error still evicts before writing.
    let before = everyone(state, &a).await;
    let error = state
        .run_scoped_prepared(
            a.clone(),
            DataDomain::new("routes"),
            |_, _| Ok(((), PeopleImpact::MayChange)),
            |db, dsp, ()| -> dispatch_core::Result<()> {
                db.routes_live_db(dsp)?.exec(
                    "UPDATE drivers SET first_name='Committed' WHERE transporter_id='driver-1'",
                    [],
                )?;
                Err(Error::new("fixture_after_commit", 500))
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "fixture_after_commit");
    let after = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(name(&after, "driver-1"), "Committed Driver");
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
    // Unclassified writes keep global revision semantics, including another DSP.
    state.run(|_| Ok(())).await.unwrap();
    assert!(!Arc::ptr_eq(&after, &everyone(state, &a).await));
    assert!(!Arc::ptr_eq(&second, &everyone(state, &b).await));
    // Existing connection gate still refuses before mutation, even after shaping.
    let at = a.clone();
    let revision = state
        .read(move |db| db.connection_revision(&at, cortex::PROVIDER))
        .await
        .unwrap()
        .unwrap();
    let error = dispatch_routes::keep_live(state, &a, later(&read), revision + 1)
        .await
        .unwrap_err();
    assert_eq!(error.code, "connection_changed");
    assert_eq!(
        name(everyone(state, &a).await.as_ref(), "driver-1"),
        "Committed Driver"
    );
}

#[tokio::test]
async fn acknowledging_a_delivered_note_keeps_people_and_cached_counts() {
    let (server, a, _b, who, read) = ready().await;
    let state = &server.state;
    // This is the actual publisher that creates changed-day notes, not a fixture answer: the
    // day's job leased, staged and published as the executor runs it.
    let (at, day, mode) = (a.clone(), read.scope.date.clone(), read.mode);
    let job = state
        .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
            let queued = db.enqueue_routes(&at, None, "directory-note", Some(&day), mode, 1)?;
            let job = db.claim_job("worker", |id, _| id == at)?.unwrap();
            assert_eq!(job.id, s(&queued[0], "id"));
            Ok(job.id)
        })
        .await
        .unwrap();
    let collected = Collected {
        data: serde_json::to_value(read).unwrap(),
        scope: None,
    };
    let staged = registry()
        .keeper(routes::JOB_KIND)
        .stage(state, &a, &job, "worker", collected)
        .await
        .unwrap();
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("routes"), move |db| {
            db.publish_routes(&at, &job, &serde_json::from_value(staged.data)?)
        })
        .await
        .map(|published| assert!(published))
        .unwrap();
    let event = CollectionSucceeded {
        keeps: routes::JOB_KIND,
    };
    let assign = dispatch_driver_match::FEATURE
        .on
        .iter()
        .find(|on| (on.hears)(&event))
        .unwrap();
    (assign.run)(state.clone(), a.clone(), &event).await;
    let before = everyone(state, &a).await;
    assert_eq!(name(&before, "driver-1"), "Fixture Driver");
    let before_counts = counts(&server, &who).await;
    let events = heard(&a, "day");
    // Call only Routes' registered collection handler: Driver Match's independent
    // CollectionSucceeded assignment is deliberately a different broad writer.
    let event = CollectionSucceeded {
        keeps: routes::JOB_KIND,
    };
    let on = dispatch_routes::FEATURE
        .on
        .iter()
        .find(|on| (on.hears)(&event))
        .unwrap();
    (on.run)(state.clone(), a.clone(), &event).await;
    assert_eq!(heard(&a, "day"), events + 1);
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert_eq!(counts(&server, &who).await, before_counts);
    let at = a.clone();
    assert_eq!(
        state
            .read(move |db| db
                .routes_db(&at)?
                .count("SELECT count(*) FROM route_days_untold", []))
            .await
            .unwrap(),
        0
    );
}

// Cached questions consume complete contributor/matching inputs, beyond Everyone's fields.
#[tokio::test]
async fn cached_questions_and_decisions_refresh_while_unchanged_live_inputs_reuse_them() {
    let (server, a, b, who, mut read) = ready().await;
    let state = &server.state;
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("paycom"), move |db| {
            let mut roster = dispatch_paycom::fixtures::fixture_date("UTC", None)?;
            let antonio = roster["employees"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|employee| employee["code"] == "E002")
                .unwrap();
            antonio["name"] = json!("REYES, ANTONIO");
            db.publish_timecards(&at, &roster)
        })
        .await
        .unwrap();
    rename(&mut read, 0, json!("Tony"));
    surname(&mut read, 0, "Reyes");
    keep(state, &a, read.clone()).await.unwrap();
    let before = everyone(state, &a).await;
    let second = everyone(state, &b).await;
    assert_eq!(counts(&server, &who).await, json!({"questions":1}));
    read = later(&read);
    read.summaries["backendTime"] = json!(33);
    keep(state, &a, read.clone()).await.unwrap();
    assert!(Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert_eq!(counts(&server, &who).await, json!({"questions":1}));
    // An existing driver's spelling ceases to suggest the pair; no new ID/event supplies
    // invalidation. This catches preserving cached counts using only new-driver counts.
    read = later(&read);
    rename(&mut read, 0, json!("Zelda"));
    surname(&mut read, 0, "Unique");
    keep(state, &a, read.clone()).await.unwrap();
    let renamed = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &renamed));
    assert_eq!(name(&renamed, "driver-1"), "Zelda Unique");
    assert_eq!(counts(&server, &who).await, json!({"questions":0}));
    // Restore the pair, then a real authorized decision removes the question without
    // changing the exported person's name. The whole PEOPLE scope still invalidates.
    read = later(&read);
    rename(&mut read, 0, json!("Tony"));
    surname(&mut read, 0, "Reyes");
    keep(state, &a, read.clone()).await.unwrap();
    let before = everyone(state, &a).await;
    assert_eq!(counts(&server, &who).await, json!({"questions":1}));
    let code = &before.holding(cortex::SOURCE, "driver-1").unwrap().code;
    let other = &before
        .holding(dispatch_paycom::SOURCE, "E002")
        .unwrap()
        .code;
    server
        .expect(
            Call::post(
                "/api/dsp/driver-match/apart",
                json!({"code":code,"other":other}),
            )
            .who(&who),
            200,
            "",
        )
        .await;
    let after = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(name(&after, "driver-1"), "Tony Reyes");
    assert_eq!(counts(&server, &who).await, json!({"questions":0}));
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
    let at = a.clone();
    state
        .run_scoped(a.clone(), DataDomain::new("paycom"), move |db| {
            let mut roster = dispatch_paycom::fixtures::fixture_date("UTC", None)?;
            roster["collectedAt"] = json!(dispatch_core::db::at(dispatch_core::db::now() + 1_000));
            let antonio = roster["employees"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|employee| employee["code"] == "E002")
                .unwrap();
            antonio["name"] = json!("REYES, ROBERTO");
            db.publish_timecards(&at, &roster)
        })
        .await
        .unwrap();
    let payroll = everyone(state, &a).await;
    assert!(!Arc::ptr_eq(&after, &payroll));
    assert_eq!(
        payroll
            .holding(dispatch_paycom::SOURCE, "E002")
            .unwrap()
            .name,
        "Roberto Reyes"
    );
    assert!(Arc::ptr_eq(&second, &everyone(state, &b).await));
}

#[tokio::test]
async fn historical_workday_membership_remains_a_broad_matching_input() {
    let (server, a, _b, who, read) = ready().await;
    let state = &server.state;
    let today = chrono::NaiveDate::parse_from_str(&read.scope.date, "%Y-%m-%d").unwrap();
    // Two real payroll publications cover both days even across a pay-period boundary.
    for back in [2, 1] {
        let (at, date) = (a.clone(), today - chrono::Duration::days(back));
        state
            .run_scoped(a.clone(), DataDomain::new("paycom"), move |db| {
                let mut roster = dispatch_paycom::fixtures::fixture_date("UTC", Some(date))?;
                let antonio = roster["employees"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|employee| employee["code"] == "E002")
                    .unwrap();
                antonio["name"] = json!("REYES, ANTONIO");
                db.publish_timecards(&at, &roster)
            })
            .await
            .unwrap();
    }
    let mut second_day = None;
    for back in [2, 1] {
        let date = (today - chrono::Duration::days(back)).to_string();
        let request = routes::Request::parse(&json!({"collection":"routes","mode":"final","date":date,
            "station":"TST1","timezone":"UTC","dspName":"Directory Proof","dspAbbreviation":"PROOF"}))
            .unwrap().unwrap();
        let mut capture = routes::fixture(&request).unwrap();
        rename(&mut capture, 0, json!("Tony"));
        surname(&mut capture, 0, "Reyes");
        assert!(
            dispatch_routes::save_day(
                state,
                &a,
                &format!("directory-workday-{back}"),
                capture.clone()
            )
            .await
            .unwrap()
        );
        second_day = Some(capture);
    }
    assert_eq!(counts(&server, &who).await, json!({"questions":1}));
    async fn evidence(state: &Arc<State>, dsp: &str) -> Vec<(Option<usize>, Option<usize>, bool)> {
        let at = dsp.to_owned();
        state
            .read(move |db| {
                Ok(db
                    .driver_match(&at)?
                    .questions
                    .into_iter()
                    .flat_map(|question| match question {
                        dispatch_driver_match::Question::Pair { evidence, .. } => evidence,
                        _ => vec![],
                    })
                    .filter(|e| e.kind == dispatch_driver_match::EvidenceKind::WorkedDays)
                    .map(|e| (e.same, e.of, e.supports))
                    .collect())
            })
            .await
            .unwrap()
    }
    assert_eq!(evidence(state, &a).await, [(Some(2), Some(2), true)]);
    let before = everyone(state, &a).await;
    // Change active workdays with the same retained driver rows/names, replacing only the
    // second historical day with driver-2. Historical writers never use the live exception.
    let mut capture = later(&second_day.unwrap());
    let second_summary = capture.summaries["itinerarySummaries"][1].clone();
    capture.summaries["itinerarySummaries"] = json!([second_summary]);
    capture.itineraries.remove(0);
    assert!(
        dispatch_routes::save_day(state, &a, "directory-workday-replaced", capture)
            .await
            .unwrap()
    );
    assert!(!Arc::ptr_eq(&before, &everyone(state, &a).await));
    assert!(evidence(state, &a).await.is_empty());
    assert_eq!(counts(&server, &who).await, json!({"questions":1}));
}
