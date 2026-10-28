//! Pick driver: a work phone nobody has, or two drivers have, filed under one of today's drivers for
//! the day, its texts of today theirs until the day's route data names one driver whose latest
//! phone it is, whose the whole day's are then.
use super::*;
use crate::backend::support::{
    DRIVERS, at, filed, phoned, phones, route, routed, saved, started, texted, today,
};
use std::sync::Arc;

/// Picks the driver `driver` for the work phone of the conversation `id`, as the member `me`.
async fn picked(state: &Arc<State>, dsp: &str, id: &str, driver: &str) -> Result<Conversation> {
    let (shared, dsp, id, driver) = (
        state.clone(),
        dsp.to_owned(),
        id.to_owned(),
        driver.to_owned(),
    );
    state
        .run(move |db| pick(db, &shared, &dsp, &id, &driver, "me"))
        .await
}
/// Whose driver's conversation the text `id` is filed in, by their ID.
async fn whose(state: &Arc<State>, dsp: &str, id: &str) -> Option<String> {
    let filed = filed(state, dsp, id).await;
    filed.driver.map(|(source, driver)| {
        assert_eq!(source, DRIVERS.as_str());
        driver
    })
}

#[tokio::test]
async fn a_driver_picked_for_a_work_phone_nobody_has_keeps_its_texts_of_today_until_route_data_names_one()
 {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let now = |h, m| at(zone, day, h, m);
    let today = day.to_string();
    // Today Avery drives CX14 and Blake has 0102; 0103 was a work phone yesterday, nobody's
    // today.
    routed(
        &dsp,
        vec![
            route(&today, "A1", "CX14", "1FTVAN214", 168, now(9, 20)),
            route(&today, "B1", "CX09", "1FTVAN207", 159, now(9, 40)),
        ],
    );
    let earlier = vec![saved(yesterday, &[("C1", "2065550103")])];
    let blake = phones(&[("B1", "2065550102")]);
    phoned(&state, &dsp, Some(&today), blake, earlier.clone()).await;
    let asked = texted(
        &state,
        &dsp,
        "+12065550103",
        "Who has the charger?",
        now(9, 0),
    )
    .await;
    let phone = filed(&state, &dsp, &asked).await;
    assert_eq!(
        (phone.kind, phone.driver.clone()),
        (ConversationKind::Work, None)
    );

    // Picked for it, Avery has its text, and what comes after.
    let avery = picked(&state, &dsp, &phone.id, "A1").await.unwrap();
    assert_eq!(avery.driver_id.as_deref(), Some("A1"));
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("A1"));
    let later = texted(&state, &dsp, "+12065550103", "Found it", now(13, 0)).await;
    assert_eq!(whose(&state, &dsp, &later).await.as_deref(), Some("A1"));

    // A read shows 0103 as Blake's latest: the day's texts with it are Blake's, before the pick
    // too.
    let blake = phones(&[("B1", "2065550103")]);
    phoned(&state, &dsp, Some(&today), blake, earlier).await;
    for text in [&asked, &later] {
        assert_eq!(whose(&state, &dsp, text).await.as_deref(), Some("B1"));
    }

    // Only one of today's drivers is picked, and only for a work phone nobody has.
    let refused = picked(&state, &dsp, &phone.id, "Z9").await.unwrap_err();
    assert_eq!(refused.code, "communications_driver_not_found");
    let stranger = texted(&state, &dsp, "+14255550199", "Is this dispatch?", now(9, 5)).await;
    let number = filed(&state, &dsp, &stranger).await.id;
    let refused = picked(&state, &dsp, &number, "A1").await.unwrap_err();
    assert_eq!(refused.code, "invalid_input");
}

#[tokio::test]
async fn a_work_phone_two_drivers_have_is_its_own_until_one_is_picked_for_it() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let now = |h, m| at(zone, day, h, m);
    let today = day.to_string();
    // Avery's and Blake's records both show 0102 as their latest: it's no one driver's.
    routed(
        &dsp,
        vec![
            route(&today, "A1", "CX14", "1FTVAN214", 168, now(9, 20)),
            route(&today, "B1", "CX09", "1FTVAN207", 159, now(9, 40)),
        ],
    );
    let both = phones(&[("A1", "2065550102"), ("B1", "2065550102")]);
    phoned(&state, &dsp, Some(&today), both, Vec::new()).await;
    let asked = texted(&state, &dsp, "+12065550102", "Which van?", now(9, 0)).await;
    let phone = filed(&state, &dsp, &asked).await;
    assert_eq!(
        (phone.kind, phone.driver.clone()),
        (ConversationKind::Work, None)
    );

    // Picked for it, Blake has it.
    picked(&state, &dsp, &phone.id, "B1").await.unwrap();
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("B1"));
}
