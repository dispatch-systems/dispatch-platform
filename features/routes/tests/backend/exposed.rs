//! Each driver's routes, as a feature or agent tool reads them: only while Routes is on for the
//! DSP, the right driver's days for the IDs given, found by driver, every driver's on a day,
//! and the days collected apart from the rest; and today as live route data holds it; and each
//! driver's latest work phone of a day.
use super::*;
use crate::backend::{
    day_request, routes_today,
    tests::{publish, publish_edited},
};
use chrono::Duration;
use dispatch_core::testing;
use dispatch_cortex::routes::{Capture, ItineraryCapture, Mode};
use dispatch_routes_contract::{DriverPhone, MOST_ID_LENGTH};
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
    let refused = offered.work_phone_holds(&db, &dsp, "2026-9-25");
    assert_eq!(refused.unwrap_err().code, "invalid_date");

    // Each refusal changes one thing from a read that's answered.
    let year = offered.routes(&db, &dsp, &driver, &day(365), &day(0));
    assert!(year.is_ok());
    let longest = vec!["d".repeat(MOST_ID_LENGTH)];
    assert!(
        offered
            .routes(&db, &dsp, &longest, &day(3), &day(0))
            .is_ok()
    );
    let longer = vec!["d".repeat(MOST_ID_LENGTH + 1)];
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
        (longer, day(3), day(0), "invalid_input"),
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

#[test]
fn route_data_is_read_through_the_indexes_made_for_each_read() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let ids = serde_json::to_string(&["driver-1"]).unwrap();
    let (historical, live) = (
        db.routes_db(&dsp).unwrap(),
        db.routes_live_db(&dsp).unwrap(),
    );
    // Each partial index holds only the tasks it names, so a driver's read reaches no other
    // task: through the indexes on every task of the days, 60 of one driver's were about
    // 9,000 rows. No read finds tasks by driver and day otherwise. Live route data holds one
    // day, whose undelivered drop-offs are all its every driver's read reaches.
    for (routes, sql, table, step) in [
        (
            &historical,
            ROUTES[1],
            "d",
            "SEARCH d USING INDEX driver_days_driver_day ",
        ),
        (
            &historical,
            ROUTES[0],
            "t",
            "SEARCH t USING COVERING INDEX tasks_removed ",
        ),
        (
            &historical,
            ROUTES[1],
            "t",
            "SEARCH t USING COVERING INDEX tasks_removed ",
        ),
        (
            &historical,
            UNDELIVERED[0],
            "t",
            "SEARCH t USING INDEX tasks_day_driver ",
        ),
        (
            &historical,
            UNDELIVERED[1],
            "t",
            "SEARCH t USING INDEX tasks_undelivered ",
        ),
        (
            &live,
            ROUTES[0],
            "t",
            "SEARCH t USING COVERING INDEX tasks_removed ",
        ),
        (
            &live,
            ROUTES[1],
            "t",
            "SEARCH t USING COVERING INDEX tasks_removed ",
        ),
        (
            &live,
            UNDELIVERED[0],
            "t",
            "SCAN t USING INDEX tasks_undelivered",
        ),
        (
            &live,
            UNDELIVERED[1],
            "t",
            "SEARCH t USING INDEX tasks_undelivered ",
        ),
    ] {
        let plan = routes
            .all(
                &format!("EXPLAIN QUERY PLAN {sql}"),
                params!["TST1", "2026-08-01", "2026-09-30", ids],
            )
            .unwrap();
        let steps: Vec<_> = plan
            .iter()
            .map(|step| s(step, "detail"))
            .filter(|step| step.split_whitespace().nth(1) == Some(table))
            .collect();
        assert!(
            !steps.is_empty() && steps.iter().all(|found| found.starts_with(step)),
            "{table}: {steps:?}"
        );
    }
    // Whether a number was ever a work phone reaches only the itineraries showing it, whatever the
    // days kept.
    let plan = historical
        .all(
            &format!("EXPLAIN QUERY PLAN {}", work_phones::EVER),
            ["5550000000"],
        )
        .unwrap();
    let steps: Vec<_> = plan.iter().map(|step| s(step, "detail")).collect();
    assert!(
        steps[0].starts_with("SEARCH i USING INDEX itineraries_work_phone (work_phone=?)"),
        "{steps:?}"
    );
    // Whether a number was ever a work phone reaches only its holds, whatever the days kept.
    let plan = historical
        .all(
            &format!("EXPLAIN QUERY PLAN {}", phones::EVER_HELD),
            ["5550000000"],
        )
        .unwrap();
    let steps: Vec<_> = plan.iter().map(|step| s(step, "detail")).collect();
    assert_eq!(
        steps,
        ["SEARCH work_phones USING COVERING INDEX work_phones_phone (phone=?)"]
    );
}

#[tokio::test]
async fn live_route_data_reads_today_with_its_assigned_routes_and_phones_only_while_routes_is_on() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    let state = dispatch_core::State::new(db.config.clone()).unwrap();
    let offered = dispatch_routes_contract::live_for_dsp(&db, &dsp)
        .unwrap()
        .expect("Routes is on");
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    let nothing = |driven: Driven| driven.days.is_empty() && driven.routes.is_empty();
    assert_eq!(offered.live_day(&db, &dsp).unwrap(), None);
    assert!(nothing(offered.day(&db, &dsp).unwrap()));

    // The first driver dispatched, on their phone since their first event; the second only
    // assigned to the day's other route.
    let today = routes_today(&db, &dsp).unwrap().to_string();
    let request = dispatch_cortex::routes::Request::parse(&json!({"collection":"routes",
        "mode":"snapshot","date":today,"station":"TST1","timezone":"UTC","dspName":"Test Owner",
        "dspAbbreviation":"NLOG"}))
    .unwrap()
    .unwrap();
    let mut read = dispatch_cortex::routes::fixture(&request).unwrap();
    let first = read.summaries["itinerarySummaries"][0].clone();
    read.summaries["itinerarySummaries"] = json!([first]);
    read.itineraries.truncate(1);
    // Another provider's route at the station, its driver assigned, is none of the DSP's.
    let mut other = read.route_summaries["rmsRouteSummaries"][1].clone();
    other["routeId"] = json!("2000");
    other["companyId"] = json!("provider-other");
    other["transporters"] = json!([{"transporterId":"driver-9"}]);
    let routes = read.route_summaries["rmsRouteSummaries"]
        .as_array_mut()
        .unwrap();
    routes.push(other);
    let people = read.route_summaries["transporters"].as_array_mut().unwrap();
    people.push(
        json!({"transporterId":"driver-9","firstName":"Other","lastName":"Driver",
        "personType":["DELIVERY_ASSOCIATE"]}),
    );
    let revision = db
        .connection_revision(&dsp, dispatch_cortex::PROVIDER)
        .unwrap()
        .unwrap();
    crate::keep_live(&state, &dsp, read.clone(), revision)
        .await
        .unwrap();
    let named = db
        .routes_live_db(&dsp)
        .unwrap()
        .count(
            "SELECT count(*) FROM drivers WHERE transporter_id='driver-9'",
            [],
        )
        .unwrap();
    assert_eq!(named, 0);

    assert_eq!(offered.live_day(&db, &dsp).unwrap(), Some(today.clone()));
    let driven = offered.day(&db, &dsp).unwrap();
    let days: Vec<_> = driven
        .days
        .iter()
        .map(|d| (d.day.as_str(), d.complete))
        .collect();
    assert_eq!(days, [(today.as_str(), false)]);
    let routes: Vec<_> = driven
        .routes
        .iter()
        .map(|r| {
            (
                r.driver_id.as_str(),
                r.route_code.as_deref(),
                r.departed_at.is_some(),
            )
        })
        .collect();
    assert_eq!(
        routes,
        [
            ("driver-1", Some("CX101"), true),
            ("driver-2", Some("CX102"), false)
        ]
    );
    let planned = offered
        .routes(&db, &dsp, &ids(&["driver-2"]))
        .unwrap()
        .routes;
    assert_eq!(planned.len(), 1);
    assert_eq!((planned[0].stops, planned[0].delivered), (2, 0));
    assert!(planned[0].planned_departure_at.is_some());
    assert_eq!(
        offered.work_phones(&db, &dsp).unwrap(),
        [WorkPhone {
            driver_id: "driver-1".into(),
            phone: "2065550190".into(),
            since: read.finished_at,
        }]
    );
    // Live route data is no historical route data.
    let historical = dispatch_routes_contract::for_dsp(&db, &dsp)
        .unwrap()
        .unwrap();
    assert!(nothing(historical.day(&db, &dsp, &today).unwrap()));
    let seventeen: Vec<String> = (0..17).map(|n| format!("driver-{n}")).collect();
    let refused = offered.routes(&db, &dsp, &seventeen).unwrap_err();
    assert_eq!(refused.code, "invalid_input");

    // Once today has passed, the day it holds isn't live.
    let yesterday = (routes_today(&db, &dsp).unwrap() - Duration::days(1)).to_string();
    db.routes_live_db(&dsp)
        .unwrap()
        .exec("UPDATE route_publications SET day=?", [&yesterday])
        .unwrap();
    assert_eq!(offered.live_day(&db, &dsp).unwrap(), None);
    assert!(nothing(
        offered.routes(&db, &dsp, &ids(&["driver-1"])).unwrap()
    ));
    assert!(offered.work_phones(&db, &dsp).unwrap().is_empty());
    assert!(offered.work_phone_holds(&db, &dsp).unwrap().is_empty());

    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "routes", false, &owner).unwrap();
    assert!(
        dispatch_routes_contract::live_for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}

/// `read` with its list showing no driver's phone, so each itinerary shows its own response's.
fn unlisted(read: &mut Capture) {
    for driver in read.summaries["transporters"].as_array_mut().unwrap() {
        driver.as_object_mut().unwrap().remove("workPhoneNumber");
    }
}
/// `read` with another itinerary of `driver`'s, begun an hour after the day's first, whose response
/// shows them on `phone`.
fn later(read: &mut Capture, driver: &str, phone: &str) {
    let id = format!("itinerary-{}", read.itineraries.len() + 1);
    let mut summary = read.summaries["itinerarySummaries"][0].clone();
    let began = summary["itineraryStartTime"].as_i64().unwrap() + 3_600_000;
    summary["itineraryId"] = json!(id);
    summary["transporterId"] = json!(driver);
    summary["itineraryStartTime"] = json!(began);
    let summaries = read.summaries["itinerarySummaries"].as_array_mut();
    summaries.unwrap().push(summary);
    let mut response: Value = serde_json::from_str(&read.itineraries[0].detail).unwrap();
    response["itineraryDetails"]["itineraryId"] = json!(id);
    response["itineraryDetails"]["transporterId"] = json!(driver);
    response["transporters"] = json!([{"transporterId":driver,"workPhoneNumber":phone}]);
    read.itineraries.push(ItineraryCapture {
        id,
        transporter_id: driver.into(),
        detail: response.to_string(),
    });
}

#[tokio::test]
async fn a_days_work_phones_are_each_drivers_latest_saying_when_two_share_one() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    let state = dispatch_core::State::new(db.config.clone()).unwrap();
    let routes = dispatch_routes_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Routes is on");
    let today = routes_today(&db, &dsp).unwrap();
    let day = |back: i64| (today - Duration::days(back)).to_string();
    let phones = |back| routes.day_phones(&db, &dsp, &day(back)).unwrap();
    let phone = |driver: &str, phone: &str, shared| DriverPhone {
        driver_id: driver.into(),
        phone: phone.into(),
        shared,
    };
    let (first, second) = ("2065550190", "2065550191");

    // The fixture's day: each driver on a phone of their own all day.
    publish(&db, &dsp, &day(1), Mode::Final);
    let own = vec![
        phone("driver-1", first, false),
        phone("driver-2", second, false),
    ];
    assert_eq!(phones(1), DayPhones::Read(own.clone()));
    // Swapped later in the day, each driver's next itinerary showing the other's phone: each has
    // the one they swapped to.
    publish_edited(&db, &dsp, &day(2), Mode::Final, |read| {
        unlisted(read);
        later(read, "driver-1", second);
        later(read, "driver-2", first);
    });
    let swapped = vec![
        phone("driver-1", second, false),
        phone("driver-2", first, false),
    ];
    assert_eq!(phones(2), DayPhones::Read(swapped));
    // The first driver's next showing the second's phone, who has it still: it's both drivers'
    // latest, and neither's alone.
    publish_edited(&db, &dsp, &day(3), Mode::Final, |read| {
        unlisted(read);
        later(read, "driver-1", second);
    });
    let both = vec![
        phone("driver-1", second, true),
        phone("driver-2", second, true),
    ];
    assert_eq!(phones(3), DayPhones::Read(both));
    // A day whose records show no phone is read, with none; one not collected isn't.
    publish_edited(&db, &dsp, &day(4), Mode::Final, |read| {
        unlisted(read);
        for itinerary in &mut read.itineraries {
            let mut response: Value = serde_json::from_str(&itinerary.detail).unwrap();
            response["transporters"] = json!([]);
            itinerary.detail = response.to_string();
        }
    });
    assert_eq!(phones(4), DayPhones::Read(Vec::new()));
    assert_eq!(phones(5), DayPhones::Uncollected);
    let refused = routes.day_phones(&db, &dsp, "2026-9-25").unwrap_err();
    assert_eq!(refused.code, "invalid_date");

    // Live: nothing without a live day; then today as its last read left it, the first driver on a
    // phone no saved day shows, which is a work phone from then on.
    let live = dispatch_routes_contract::live_for_dsp(&db, &dsp)
        .unwrap()
        .expect("Routes is on");
    assert_eq!(live.day_phones(&db, &dsp).unwrap(), DayPhones::Uncollected);
    let request = day_request(&db, &dsp, &today.to_string(), Mode::Snapshot).unwrap();
    let mut read = dispatch_cortex::routes::fixture(&request).unwrap();
    read.summaries["transporters"][0]["workPhoneNumber"] = json!("+1 555 010 0009");
    let ever = |phone: &str| routes.ever_work_phone(&db, &dsp, phone).unwrap();
    assert!(!ever("5550100009"));
    let revision = db
        .connection_revision(&dsp, dispatch_cortex::PROVIDER)
        .unwrap()
        .unwrap();
    crate::keep_live(&state, &dsp, read, revision)
        .await
        .unwrap();
    assert_eq!(
        live.day_phones(&db, &dsp).unwrap(),
        DayPhones::Read(vec![
            phone("driver-1", "5550100009", false),
            phone("driver-2", second, false),
        ])
    );
    assert!(ever("(555) 010-0009") && ever(first), "live and saved");
    assert!(!ever("5550100008") && !ever("0100009"));
}
