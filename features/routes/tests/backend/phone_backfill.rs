//! The saved days' work phones filled in from their stored responses: a few days a pass, newest
//! first, those it can't read among them.
use super::*;
use crate::backend::{routes_today, tests::publish};
use chrono::Duration;
use dispatch_core::testing;
use dispatch_routes_contract::{DayPhones, DriverPhone};

#[tokio::test]
async fn a_pass_tries_at_most_its_batch_of_days_counting_those_it_cant_read() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, &dsp, dispatch_cortex::PROVIDER).unwrap();
    let state = State::new(db.config.clone()).unwrap();
    let routes = dispatch_routes_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Routes is on");
    let today = routes_today(&db, &dsp).unwrap();
    let day = |back| (today - Duration::days(back)).to_string();
    let phones = |back| routes.day_phones(&db, &dsp, &day(back)).unwrap();

    // Days kept by the version before: the newest, then more than a pass tries whose stored
    // responses can't be read, then the oldest.
    let oldest = EXTRACTED_PER_PASS + 3;
    for back in 1..=oldest {
        // Its job done, as the executor leaves one it published, so the queue takes the next.
        let job = publish(&db, &dsp, &day(back), Mode::Final);
        testing::set_job_status(&db, &job, "succeeded").unwrap();
    }
    let historical = db.routes_db(&dsp).unwrap();
    let shaped = "UPDATE route_publications SET adapter_version=?";
    historical.exec(shaped, [PHONES_SINCE - 1]).unwrap();
    historical
        .exec("UPDATE itineraries SET work_phone=NULL", [])
        .unwrap();
    let unreadable = "UPDATE route_raw SET body=x'00' WHERE publication_id IN \
        (SELECT id FROM route_publications WHERE day<?1 AND day>?2)";
    historical
        .exec(unreadable, params![day(1), day(oldest)])
        .unwrap();
    drop(historical);

    // A pass extracts the newest's, then tries no more than so many, leaving the oldest for later.
    extract(&state, &dsp).await.unwrap();
    let phone = |driver: &str, phone: &str| DriverPhone {
        driver_id: driver.into(),
        phone: phone.into(),
        shared: false,
    };
    assert_eq!(
        phones(1),
        DayPhones::Read(vec![
            phone("driver-1", "2065550190"),
            phone("driver-2", "2065550191"),
        ])
    );
    assert_eq!(phones(oldest), DayPhones::Unextracted);
}
