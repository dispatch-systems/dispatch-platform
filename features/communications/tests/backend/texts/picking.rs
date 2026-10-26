//! Pick driver: a work phone nobody held, filed under one of today's drivers for the day, its texts
//! of today theirs but where a hold covers one, and kept so as the holds settle.
use super::*;
use crate::backend::support::{
    DRIVERS, at, filed, held, hold, route, routed, started, texted, today,
};
use std::sync::Arc;

/// Picks the driver `driver` for the work phone of the conversation `id`, as the member `me`.
async fn picked(state: &Arc<State>, dsp: &str, id: &str, driver: &str) -> Result<Conversation> {
    let (shared, dsp, id, driver) = (state.clone(), dsp.to_owned(), id.to_owned(), driver.to_owned());
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
async fn a_driver_picked_for_a_work_phone_nobody_held_keeps_its_texts_of_today_as_the_holds_settle()
 {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    let (now, then) = (|h, m| at(zone, day, h, m), |h, m| at(zone, yesterday, h, m));
    let today = day.to_string();
    // Today Avery drives CX14 and Blake holds 0102; 0103 was a work phone yesterday, nobody's
    // today.
    routed(
        &dsp,
        vec![
            route(&today, "A1", "CX14", "1FTVAN214", 168, now(9, 20)),
            route(&today, "B1", "CX09", "1FTVAN207", 159, now(9, 40)),
        ],
    );
    let saved = vec![(
        yesterday.to_string(),
        hold("C1", "2065550103", then(8, 0), None),
    )];
    let blake = hold("B1", "2065550102", now(8, 0), None);
    held(&state, &dsp, Some(&today), vec![blake.clone()], saved.clone()).await;
    let asked = texted(&state, &dsp, "+12065550103", "Who has the charger?", now(9, 0)).await;
    let phone = filed(&state, &dsp, &asked).await;
    assert_eq!((phone.kind, phone.driver.clone()), (ConversationKind::Work, None));

    // Picked for it, Avery has its text.
    let avery = picked(&state, &dsp, &phone.id, "A1").await.unwrap();
    assert_eq!(avery.driver_id.as_deref(), Some("A1"));
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("A1"));

    // As the holds settle, Blake claims 0103 at noon: what came before stays Avery's, and what
    // comes once Blake holds it is Blake's.
    let claimed = vec![
        hold("B1", "2065550102", now(8, 0), Some(now(12, 0))),
        hold("B1", "2065550103", now(12, 0), None),
    ];
    held(&state, &dsp, Some(&today), claimed, saved).await;
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("A1"));
    let later = texted(&state, &dsp, "+12065550103", "Found it", now(13, 0)).await;
    assert_eq!(whose(&state, &dsp, &later).await.as_deref(), Some("B1"));

    // Only one of today's drivers is picked, and only for a work phone nobody held.
    let refused = picked(&state, &dsp, &phone.id, "Z9").await.unwrap_err();
    assert_eq!(refused.code, "communications_driver_not_found");
    let stranger = texted(&state, &dsp, "+14255550199", "Is this dispatch?", now(9, 5)).await;
    let number = filed(&state, &dsp, &stranger).await.id;
    let refused = picked(&state, &dsp, &number, "A1").await.unwrap_err();
    assert_eq!(refused.code, "invalid_input");
}
