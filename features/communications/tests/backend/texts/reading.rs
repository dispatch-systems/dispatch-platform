//! Reading texts: a text's picture goes only to a member whose access still stands once the phone
//! has read it; a driver's work conversation is found by each work phone they texted on.
use super::*;
use crate::backend::support::{self, at, held, hold, texted, today};
use dispatch_core::testing;

#[tokio::test]
async fn a_texts_picture_is_refused_once_the_members_access_ended_while_the_phone_read_it() {
    support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let photo = "file_photo";
    assert!(file(&state, &c, support::USE, photo).await.is_ok());

    // Communications goes off for the DSP as the phone reads it, and with it their permission.
    support::off_while_reading(&dsp);
    let refused = file(&state, &c, support::USE, photo).await.err();
    assert_eq!(
        refused.map(|error| error.code).as_deref(),
        Some("permission_denied")
    );
}

#[tokio::test]
async fn a_drivers_work_conversation_is_found_by_the_digits_of_each_work_phone_they_texted_on() {
    let (_root, state, dsp) = support::started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    // Yesterday Avery held 0101, and today 0202, texting on each.
    let (midnight, then) = (at(zone, day, 0, 0), at(zone, yesterday, 9, 0));
    let earlier = (yesterday.to_string(), hold("A1", "2065550101", then, None));
    let now_held = vec![hold("A1", "2065550202", midnight, None)];
    held(
        &state,
        &dsp,
        Some(&day.to_string()),
        now_held,
        vec![earlier],
    )
    .await;
    texted(&state, &dsp, "+12065550101", "Leaving the station", then).await;
    texted(&state, &dsp, "+12065550202", "Which van today?", midnight).await;

    let found = |typed: &'static str| {
        let (shared, dsp) = (state.clone(), dsp.clone());
        state.read(move |db| {
            let found = search(db, &shared, &dsp, ConversationKind::Work, typed)?;
            Ok(found
                .conversations
                .into_iter()
                .filter_map(|each| each.driver_id)
                .collect::<Vec<_>>())
        })
    };
    assert_eq!(found("555-0202").await.unwrap(), ["A1"]);
    assert_eq!(found("0101").await.unwrap(), ["A1"]);
    assert!(found("0303").await.unwrap().is_empty());
}
