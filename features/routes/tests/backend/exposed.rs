//! Each driver's routes, as a feature or agent tool reads them: only while Routes is on for the
//! DSP, the right driver's days for the IDs given, every driver's on a day, and the days
//! collected apart from the rest.
use super::*;
use crate::backend::{routes_today, tests::publish};
use chrono::Duration;
use dispatch_core::testing;
use dispatch_cortex::routes::Mode;
use serde_json::json;

#[test]
fn driver_routes_reads_a_drivers_days_only_while_routes_is_on() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    let offered = dispatch_routes_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Routes is on");
    let today = routes_today(&db, &dsp).unwrap();
    let day = |back: i64| (today - Duration::days(back)).to_string();
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    let driver = ids(&["driver-2"]);
    assert_eq!(offered.collected(&db, &dsp).unwrap(), None);

    // Three days ago and yesterday are left uncollected; today is read while under way.
    publish(&db, &dsp, &day(2), Mode::Final);
    publish(&db, &dsp, &day(0), Mode::Snapshot);
    assert_eq!(
        offered.collected(&db, &dsp).unwrap(),
        Some(Collected {
            from: day(2),
            through: day(0)
        })
    );
    let driven = offered
        .routes(&db, &dsp, &driver, &day(3), &day(0))
        .unwrap();
    let days: Vec<_> = driven
        .days
        .iter()
        .map(|d| (d.day.clone(), d.complete))
        .collect();
    assert_eq!(days, [(day(2), true), (day(0), false)]);
    assert_eq!(driven.routes.len(), 2);
    let route = &driven.routes[0];
    assert_eq!(
        (route.day.as_str(), route.driver_id.as_str()),
        (day(2).as_str(), "driver-2")
    );
    assert_eq!(route.route_code.as_deref(), Some("CX102"));
    assert_eq!(route.vin.as_deref(), Some("1FTFIXTURE00000"));
    assert_eq!((route.stops, route.stops_completed), (2, 2));
    assert_eq!(
        (route.delivered, route.not_delivered, route.removed),
        (1, 1, 1)
    );
    assert!(route.departed_at.is_some() && route.session_end_at.is_some());
    // Its wave as Amazon gives it: the fixture's starts 20 minutes before 08:00 UTC.
    let wave = format!("{}T07:40:00Z", day(2)).parse::<chrono::DateTime<chrono::Utc>>();
    assert_eq!(route.wave_start_at, Some(wave.unwrap().timestamp_millis()));
    assert_eq!(route.undelivered.len(), 1);
    let package = &route.undelivered[0];
    assert_eq!(package.tracking_id.as_deref(), Some("TBA000000000002"));
    assert_eq!(package.state.as_deref(), Some("UNDELIVERABLE"));
    assert_eq!(package.reason, None);
    assert!(package.attempted_at.is_some());

    // Every ID a person holds is read; a driver who wasn't out has collected days and no routes.
    let both = offered
        .routes(&db, &dsp, &ids(&["driver-1", "driver-2"]), &day(3), &day(0))
        .unwrap();
    assert_eq!(both.routes.len(), 4);
    assert!(
        both.routes
            .iter()
            .filter(|r| r.driver_id == "driver-1")
            .all(|r| r.delivered == 2 && r.undelivered.is_empty())
    );
    // Up to 16 IDs are matched exactly as written; none at all reads the days alone.
    let sixteen: Vec<String> = (1..=16).map(|n| format!("driver {n}")).collect();
    for asked in [sixteen, Vec::new()] {
        let absent = offered.routes(&db, &dsp, &asked, &day(3), &day(0)).unwrap();
        assert_eq!(absent.days.len(), 2);
        assert!(absent.routes.is_empty());
    }

    // A day read whole is every driver's itineraries that day; one uncollected, nothing.
    let today_all = offered.day(&db, &dsp, &day(0)).unwrap();
    let drivers: Vec<_> = today_all
        .routes
        .iter()
        .map(|r| (r.day.clone(), r.driver_id.clone()))
        .collect();
    assert_eq!(
        drivers,
        [(day(0), "driver-1".into()), (day(0), "driver-2".into())]
    );
    assert!(today_all.routes.iter().all(|r| r.wave_start_at.is_some()));
    let days: Vec<_> = today_all
        .days
        .iter()
        .map(|d| (d.day.clone(), d.complete))
        .collect();
    assert_eq!(days, [(day(0), false)]);
    let uncollected = offered.day(&db, &dsp, &day(1)).unwrap();
    assert!(uncollected.days.is_empty() && uncollected.routes.is_empty());
    let refused = offered.day(&db, &dsp, "2026-9-25").unwrap_err();
    assert_eq!(refused.code, "invalid_date");

    // Each refusal changes one thing from a read that's answered.
    let year = offered.routes(&db, &dsp, &driver, &day(365), &day(0));
    assert!(year.is_ok());
    let seventeen: Vec<String> = (0..17).map(|n| format!("driver-{n}")).collect();
    for (asked, from, to, code) in [
        (
            driver.clone(),
            "2026-9-25".to_owned(),
            day(0),
            "invalid_date",
        ),
        (driver.clone(), day(366), day(0), "invalid_input"),
        (seventeen, day(3), day(0), "invalid_input"),
    ] {
        let refused = offered.routes(&db, &dsp, &asked, &from, &to).unwrap_err();
        assert_eq!(refused.code, code, "{asked:?} {from} {to}");
    }

    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "routes", false, &owner).unwrap();
    assert!(
        dispatch_routes_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}
