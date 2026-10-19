//! What Daily Performance exposes, as a feature using it finds it: only while it's on for the
//! DSP.
use crate::DailyPerformanceStore;
use dispatch_core::{
    db::{Store, s},
    testing,
};
use dispatch_cortex::{
    daily_performance::{self, Capture, Request},
    discovery::CollectionRequest,
};
use dispatch_daily_performance_contract::{self as metrics, Collected};
use serde_json::{Value, json};

fn ready() -> (tempfile::TempDir, Store, String) {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (root, store, dsp) = testing::bootstrapped();
    testing::set_dsp(&store, &dsp, "Fixture Delivery", "America/Los_Angeles").unwrap();
    store
        .set_profile(
            &dsp,
            json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
        )
        .unwrap();
    testing::ready_connection(&store, &dsp, dispatch_cortex::PROVIDER).unwrap();
    store.enable_all_features(&dsp).unwrap();
    (root, store, dsp)
}
/// Publishes the fixture's day, as `change` leaves it.
fn publish(store: &Store, dsp: &str, date: &str, change: fn(&mut Capture)) {
    let queued = store
        .enqueue_daily_performance(dsp, None, date, date, date)
        .unwrap();
    let job = s(&queued[0], "id");
    let request: Value =
        serde_json::from_str(&store.job_row(job, Some(dsp)).unwrap().request).unwrap();
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let request: Request =
        serde_json::from_value(keeper.bind(store, dsp, &request).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!("discover")
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let mut capture = daily_performance::fixture(&request).unwrap();
    change(&mut capture);
    store
        .publish_daily_performance(dsp, job, &capture, &scope)
        .unwrap();
}

#[test]
fn a_drivers_days_are_read_for_their_ids_with_unknown_days_told_apart_while_it_is_on() {
    let (_root, db, dsp) = ready();
    let offered = metrics::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Daily Performance is on");
    assert_eq!(offered.collected(&db, &dsp).unwrap(), None);
    publish(&db, &dsp, "2026-10-01", |_| {});
    // Amazon writes a number as text, or as something that isn't one.
    publish(&db, &dsp, "2026-10-03", |capture| {
        capture.datasets[0].rows[0]["delivered"] = json!("97");
        capture.datasets[0].rows[0]["dispatched"] = json!({"count": 99});
    });
    // Collected before Amazon posted it, every dataset empty: unknown, not a day of nothing.
    publish(&db, &dsp, "2026-10-04", |capture| {
        capture
            .datasets
            .iter_mut()
            .for_each(|dataset| dataset.rows.clear())
    });
    assert_eq!(
        offered.collected(&db, &dsp).unwrap(),
        Some(Collected {
            from: "2026-10-01".into(),
            through: "2026-10-03".into(),
        })
    );

    // driver-2's rows, not asked for, are left out.
    let ids = ["driver-1".to_owned(), "driver-3".to_owned()];
    let read = offered
        .days(&db, &dsp, &ids, "2026-09-30", "2026-10-04")
        .unwrap();
    assert_eq!(read.collected, ["2026-10-01", "2026-10-03"]);
    let days: Vec<_> = read
        .days
        .iter()
        .map(|day| (day.date.as_str(), day.id.as_str()))
        .collect();
    assert_eq!(
        days,
        [
            ("2026-10-01", "driver-1"),
            ("2026-10-01", "driver-3"),
            ("2026-10-03", "driver-1"),
            ("2026-10-03", "driver-3"),
        ]
    );
    let first = &read.days[0];
    assert_eq!(first.quality.as_ref().unwrap().delivered, Some(101.0));
    assert!(first.feedback.as_ref().unwrap().negatives.is_empty());
    let returned = &first.returned[0];
    assert_eq!(
        (
            returned.tracking_id.as_deref(),
            returned.reason.as_deref(),
            returned.impacting,
            returned.exempted,
        ),
        (
            Some("TBA000000000001"),
            Some("BUSINESS CLOSED"),
            Some(true),
            Some(false)
        )
    );
    let third = &read.days[1];
    assert_eq!(third.safety.as_ref().unwrap().speeding, Some(2.0));
    let negatives: Vec<_> = third
        .feedback
        .as_ref()
        .unwrap()
        .negatives
        .iter()
        .map(|n| (n.what.as_str(), n.count))
        .collect();
    assert_eq!(negatives, [("driver_mishandled_package_cnt", 2.0)]);
    // Reported both live and assessed, the event is told once, as assessed.
    let events: Vec<_> = third
        .events
        .iter()
        .map(|e| (e.kind.as_deref(), e.impacting, e.assessed))
        .collect();
    assert_eq!(events, [(Some("Speeding"), Some(true), true)]);
    let quality = read.days[2].quality.as_ref().unwrap();
    assert_eq!((quality.delivered, quality.dispatched), (Some(97.0), None));

    // An ID with no rows still sees the days collected, so it reads as nothing, not unknown.
    let other = ["driver-9".to_owned()];
    let nobody = offered
        .days(&db, &dsp, &other, "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(nobody.collected, ["2026-10-01", "2026-10-03"]);
    assert!(nobody.days.is_empty());

    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "daily_performance", false, &owner)
        .unwrap();
    assert!(metrics::for_dsp(&db, &dsp).unwrap().is_none());
}

#[test]
fn bad_days_and_ids_are_refused() {
    let (_root, db, dsp) = ready();
    let offered = metrics::for_dsp(&db, &dsp).unwrap().unwrap();
    let one = ["driver-1".to_owned()];
    // Any ID a source holds, up to 128 characters, and none at all.
    let odd = ["driver 1/é".to_owned(), "x".repeat(128)];
    for ids in [&odd[..], &[][..]] {
        let read = offered
            .days(&db, &dsp, ids, "2026-09-03", "2026-10-03")
            .unwrap();
        assert!(read.days.is_empty());
    }
    let many: Vec<String> = (0..21).map(|n| format!("driver-{n}")).collect();
    let empty = ["driver-1".to_owned(), String::new()];
    let long = ["x".repeat(129)];
    for (ids, from, to, code) in [
        (&one[..], "2026-10-1", "2026-10-03", "invalid_date"),
        (&one[..], "2026-10-03", "2026-10-01", "invalid_input"),
        (&one[..], "2026-09-02", "2026-10-03", "invalid_input"),
        (&many[..], "2026-10-01", "2026-10-03", "invalid_input"),
        (&empty[..], "2026-10-01", "2026-10-03", "invalid_input"),
        (&long[..], "2026-10-01", "2026-10-03", "invalid_input"),
    ] {
        let error = offered.days(&db, &dsp, ids, from, to).unwrap_err();
        assert_eq!(error.code, code, "{ids:?} {from} {to}");
    }
}
