//! Today's Routes: while live route data holds today, its drivers by name, each with their route,
//! the work phone they hold now and their work conversation, and the work phones nobody held that
//! texted today; nothing once it holds no day.
use super::*;
use crate::backend::support::{
    self, at, driver, held, hold, route, routed, started, texted, today, van,
};
use crate::{ConversationState, Pace};
use dispatch_routes_contract::RouteDay;
use std::sync::Arc;

/// Today's Routes, as the page reads it.
async fn listed(state: &Arc<State>, dsp: &str) -> TodaysRoutes {
    let (shared, dsp) = (state.clone(), dsp.to_owned());
    state.read(move |db| read(db, &shared, &dsp)).await.unwrap()
}

#[tokio::test]
async fn todays_routes_lists_the_live_days_drivers_with_their_route_phone_and_texts_until_it_holds_no_day()
 {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let (now, then) = (|h, m| at(zone, day, h, m), |h, m| at(zone, yesterday, h, m));
    let (today, yesterday) = (day.to_string(), yesterday.to_string());
    support::know(
        &state,
        &dsp,
        vec![
            driver("avery", "Avery Diaz", "A1"),
            driver("blake", "Blake Moss", "B1"),
            driver("casey", "Casey Ruiz", "C1"),
        ],
        Vec::new(),
        vec![van("1FTVAN214", "214")],
    );
    // Avery drives CX14 behind, beside a rescue with fewer stops; Blake drove every stop of
    // CX09. Casey holds a work phone today but is gone from the day's list.
    routed(
        &dsp,
        vec![
            RouteDay {
                progress: Some("BEHIND".into()),
                ..route(&today, "A1", "CX14", "1FTVAN214", 168, now(9, 20))
            },
            route(&today, "A1", "CX40", "1FTVAN040", 12, now(11, 0)),
            RouteDay {
                stops_completed: 159,
                ..route(&today, "B1", "CX09", "1FTVAN207", 159, now(9, 40))
            },
        ],
    );
    let saved = vec![(yesterday, hold("D1", "2065550103", then(8, 0), None))];
    held(
        &state,
        &dsp,
        Some(&today),
        vec![
            hold("A1", "2065550101", now(8, 0), None),
            hold("C1", "2065550104", now(8, 0), None),
        ],
        saved.clone(),
    )
    .await;
    // Avery and Casey text; so does 0103, a work phone yesterday that nobody holds today.
    texted(&state, &dsp, "+12065550101", "Tire light came on", now(9, 12)).await;
    texted(&state, &dsp, "+12065550104", "Done, heading back", now(9, 30)).await;
    texted(&state, &dsp, "+12065550103", "Who has the charger?", now(10, 0)).await;

    let shown = listed(&state, &dsp).await;
    assert_eq!(shown.day.as_deref(), Some(today.as_str()));
    let drivers: Vec<_> = shown
        .drivers
        .iter()
        .map(|each| {
            (
                each.person.name.as_str(),
                each.today.route.as_deref(),
                each.today.van.as_deref(),
                (each.today.stops, each.today.stops_done, each.today.pace),
                each.today.phone.as_deref(),
                each.conversation.as_ref().map(|conversation| conversation.state),
            )
        })
        .collect();
    use ConversationState::Waiting;
    assert_eq!(
        drivers,
        [
            (
                "Avery Diaz",
                Some("CX14"),
                Some("214"),
                (Some(168), Some(84), Some(Pace::Behind)),
                Some("+12065550101"),
                Some(Waiting),
            ),
            (
                "Blake Moss",
                Some("CX09"),
                None,
                (Some(159), Some(159), Some(Pace::Done)),
                None,
                None,
            ),
            (
                "Casey Ruiz",
                None,
                None,
                (None, None, None),
                Some("+12065550104"),
                Some(Waiting),
            ),
        ]
    );
    let phones: Vec<_> = shown
        .phones
        .iter()
        .map(|each| (each.with.name.as_str(), each.state))
        .collect();
    assert_eq!(phones, [("Work phone 0103", Waiting)]);

    // Once live route data holds no day, as after the stop, it lists nothing.
    held(&state, &dsp, None, Vec::new(), saved).await;
    let shown = listed(&state, &dsp).await;
    assert_eq!(
        (shown.day, shown.drivers.len(), shown.phones.len()),
        (None, 0, 0)
    );
}
