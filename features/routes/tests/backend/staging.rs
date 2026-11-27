use super::*;
use crate::backend::tests::{later, request};
use dispatch_core::{State, db::Store, testing};
use dispatch_cortex::routes::{Mode, fixture};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn a_day_staged_in_batches_is_staged_whole_and_stops_once_its_work_is_not_the_callers() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    let state = State::new(store.config.clone()).unwrap();
    let asked = request(Mode::Final);
    // Two whole batches and part of a third.
    let mut capture = fixture(&asked).unwrap();
    while capture.itineraries.len() < 2 * BATCH_ITINERARIES + 3 {
        later(&mut capture, "driver-1", "2065550190");
    }
    let listed = capture.itineraries.len() as i64;
    let db = store.routes_db(&dsp).unwrap();
    // What a key's publication holds: its itineraries, and their responses.
    let staged = |key: &str| {
        let count = |sql: &str| db.count(sql, [key]).unwrap();
        (
            count(
                "SELECT count(*) FROM itineraries i JOIN route_publications p \
                 ON p.id=i.publication_id WHERE p.job_id=?",
            ),
            count(
                "SELECT count(*) FROM route_raw r JOIN route_publications p \
                 ON p.id=r.publication_id WHERE p.job_id=? AND r.name LIKE 'itinerary:%'",
            ),
        )
    };

    // Every itinerary is staged with its response, and the publication counts every stop and
    // task its rows hold.
    let whole = stage_capture(
        &state,
        &dsp,
        "whole",
        asked.clone(),
        capture.clone(),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(staged("whole"), (listed, listed));
    let count = |sql: &str| db.count(sql, [&whole.publication]).unwrap();
    assert_eq!(
        (
            count("SELECT itinerary_count FROM route_publications WHERE id=?"),
            count("SELECT stop_count FROM route_publications WHERE id=?"),
            count("SELECT task_count FROM route_publications WHERE id=?"),
        ),
        (
            listed,
            count("SELECT count(*) FROM stops WHERE publication_id=?"),
            count("SELECT count(*) FROM tasks WHERE publication_id=? AND active=1"),
        )
    );

    // Each write asks first whether the work is still the caller's: once it isn't, nothing more
    // is written, here after the day's start and its first batch.
    let asked_mine = Arc::new(AtomicUsize::new(0));
    let mine = {
        let asked_mine = asked_mine.clone();
        move |_: &Store| {
            let asked = asked_mine.fetch_add(1, Ordering::SeqCst) + 1;
            dispatch_core::ensure(asked <= 2, "not_found", 404)
        }
    };
    let refused = stage_capture(&state, &dsp, "refused", asked, capture, mine).await;
    assert_eq!(refused.unwrap_err().code, "not_found");
    assert_eq!(asked_mine.load(Ordering::SeqCst), 3);
    let batch = BATCH_ITINERARIES as i64;
    assert_eq!(staged("refused"), (batch, batch));
}
