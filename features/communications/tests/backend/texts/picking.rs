//! Pick driver: a work phone nobody has, or two drivers have, filed under one of today's drivers for
//! the day, its texts of today theirs until the day's route data names one driver whose latest
//! phone it is, whose the whole day's are then.
use super::*;
use crate::backend::{
    support::{
        DRIVERS, USE, at, filed, member, phoned, phones, route, routed, saved, sent, started,
        texted, today,
    },
    texts::reply::send,
};
use std::sync::Arc;

/// Picks the driver `driver` for the work phone of the conversation `id`, as the member `me` at
/// `at`.
async fn picked(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    driver: &str,
    at: i64,
) -> Result<Conversation> {
    let (shared, dsp, id, driver) = (
        state.clone(),
        dsp.to_owned(),
        id.to_owned(),
        driver.to_owned(),
    );
    state
        .run(move |db| pick(db, &shared, &dsp, &id, &driver, "me", at))
        .await
}
/// Each pick the audit log keeps, oldest first: whom, and the work phone.
async fn logged(state: &Arc<State>, dsp: &str) -> Vec<(String, String)> {
    let dsp = dsp.to_owned();
    state
        .read(move |db| {
            db.platform.query_as(
                "SELECT json_extract(data,'$.target'),detail FROM audit \
                 WHERE dsp_id=? AND action='communications.driver_picked' ORDER BY id",
                [&dsp],
            )
        })
        .await
        .unwrap()
}
/// Where the answers queued so far went, oldest first.
async fn went(state: &Arc<State>, dsp: &str) -> Vec<String> {
    sent(state, dsp)
        .await
        .into_iter()
        .map(|(to, ..)| to)
        .collect()
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

    // Picked for it, Avery has its text, and what comes after, and is answered at it.
    let avery = picked(&state, &dsp, &phone.id, "A1", now(10, 0))
        .await
        .unwrap();
    assert_eq!(avery.driver_id.as_deref(), Some("A1"));
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("A1"));
    let pick = (avery.with.name.clone(), "(206) 555-0103".to_owned());
    assert_eq!(logged(&state, &dsp).await, [pick.clone()]);
    let later = texted(&state, &dsp, "+12065550103", "Found it", now(13, 0)).await;
    assert_eq!(whose(&state, &dsp, &later).await.as_deref(), Some("A1"));
    let c = member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let answer = || send(&state, &c, USE, &avery.id, "Thanks", (&[], None));
    answer().await.unwrap();
    assert_eq!(went(&state, &dsp).await, ["+12065550103"]);

    // A read shows 0103 as Blake's latest: the day's texts with it are Blake's, before the pick
    // too, and Avery has no work phone to be answered at.
    let blake = phones(&[("B1", "2065550103")]);
    phoned(&state, &dsp, Some(&today), blake, earlier).await;
    for text in [&asked, &later] {
        assert_eq!(whose(&state, &dsp, text).await.as_deref(), Some("B1"));
    }
    assert_eq!(
        answer().await.unwrap_err().code,
        "communications_no_work_phone"
    );

    // Only one of today's drivers is picked, and only for a work phone nobody has.
    let refused = picked(&state, &dsp, &phone.id, "Z9", now(13, 5))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_driver_not_found");
    let stranger = texted(&state, &dsp, "+14255550199", "Is this dispatch?", now(9, 5)).await;
    let number = filed(&state, &dsp, &stranger).await.id;
    let refused = picked(&state, &dsp, &number, "A1", now(13, 5))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "invalid_input");
    // The picks refused are none of the log's.
    assert_eq!(logged(&state, &dsp).await, [pick]);
}

#[tokio::test]
async fn a_driver_picked_for_a_work_phone_is_answered_at_it_rather_than_their_own_latest() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let today = day.to_string();
    // Avery drives CX14 with 0101 as their latest; 0103, a work phone yesterday, nobody's today,
    // texts.
    routed(
        &dsp,
        vec![route(
            &today,
            "A1",
            "CX14",
            "1FTVAN214",
            168,
            at(zone, day, 9, 20),
        )],
    );
    let avery = phones(&[("A1", "2065550101")]);
    let earlier = vec![saved(day.pred_opt().unwrap(), &[("C1", "2065550103")])];
    phoned(&state, &dsp, Some(&today), avery, earlier).await;
    let asked = texted(
        &state,
        &dsp,
        "+12065550103",
        "This is Avery, my phone died",
        at(zone, day, 9, 0),
    )
    .await;
    let phone = filed(&state, &dsp, &asked).await;

    // Picked for it, Avery is answered at 0103, where they asked from.
    let avery = picked(&state, &dsp, &phone.id, "A1", at(zone, day, 10, 0))
        .await
        .unwrap();
    let c = member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    send(&state, &c, USE, &avery.id, "Got it", (&[], None))
        .await
        .unwrap();
    assert_eq!(went(&state, &dsp).await, ["+12065550103"]);
}

#[tokio::test]
async fn a_driver_picked_for_more_than_one_work_phone_is_answered_at_the_one_picked_last() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    let now = |h, m| at(zone, day, h, m);
    let today = day.to_string();
    // Avery and Blake drive; 0102, 0103 and 0104 were work phones yesterday, nobody's today, and
    // each texts.
    routed(
        &dsp,
        vec![
            route(&today, "A1", "CX14", "1FTVAN214", 168, now(9, 20)),
            route(&today, "B1", "CX09", "1FTVAN207", 159, now(9, 40)),
        ],
    );
    let yesterday = day.pred_opt().unwrap();
    let earlier = vec![saved(
        yesterday,
        &[
            ("C1", "2065550102"),
            ("C2", "2065550103"),
            ("C3", "2065550104"),
        ],
    )];
    phoned(&state, &dsp, Some(&today), Vec::new(), earlier).await;
    let mut work = Vec::new();
    for number in ["+12065550102", "+12065550103", "+12065550104"] {
        let asked = texted(&state, &dsp, number, "Who has this one?", now(9, 0)).await;
        work.push(filed(&state, &dsp, &asked).await.id);
    }
    let c = member(&state, &dsp, "Test", "Owner", "owner@example.test").await;

    // 0103 is picked for Avery, then 0102 for Blake, then 0103's pick corrected to Blake: Blake
    // is answered at 0103, picked last.
    picked(&state, &dsp, &work[1], "A1", now(10, 0))
        .await
        .unwrap();
    picked(&state, &dsp, &work[0], "B1", now(10, 5))
        .await
        .unwrap();
    let blake = picked(&state, &dsp, &work[1], "B1", now(10, 10))
        .await
        .unwrap();
    let answer = || send(&state, &c, USE, &blake.id, "Thanks", (&[], None));
    answer().await.unwrap();
    // Then 0104 is picked for Blake too: Blake is answered there.
    picked(&state, &dsp, &work[2], "B1", now(10, 15))
        .await
        .unwrap();
    answer().await.unwrap();
    assert_eq!(went(&state, &dsp).await, ["+12065550103", "+12065550104"]);
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
    picked(&state, &dsp, &phone.id, "B1", now(10, 0))
        .await
        .unwrap();
    assert_eq!(whose(&state, &dsp, &asked).await.as_deref(), Some("B1"));
}
