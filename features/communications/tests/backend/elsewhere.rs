//! A person's conversations of each kind: their work one follows the ID holding a work phone now,
//! whichever of their IDs their earlier texts were under.
use super::*;
use crate::backend::support::{self, DRIVERS, at, held, hold, texted, today};
use dispatch_core::manifest::people::PersonRole;
use std::collections::BTreeMap;

#[tokio::test]
async fn the_work_chip_goes_to_the_id_holding_a_work_phone_now_not_an_earlier_aliases() {
    let (_root, state, dsp) = support::started();
    let (zone, day) = today(&state, &dsp).await;
    let yesterday = day.pred_opt().unwrap();
    // Avery, one person to Driver Match, drove as A1 yesterday and as A2 today.
    let avery = Person {
        code: "avery".into(),
        name: "Avery Diaz".into(),
        names: vec!["Avery Diaz".into()],
        role: PersonRole::Driver,
        title: None,
        ids: BTreeMap::from([(DRIVERS, vec!["A1".into(), "A2".into()])]),
        emails: Vec::new(),
        phones: vec!["(253) 555-0177".into()],
        confirmed: true,
        photo: None,
    };
    support::know(&state, &dsp, vec![avery.clone()], Vec::new(), Vec::new());
    // Yesterday, as A1, they texted from 0101; today, as A2, they hold 0202 and haven't texted.
    let then = at(zone, yesterday, 9, 0);
    let earlier = (yesterday.to_string(), hold("A1", "2065550101", then, None));
    let now_held = vec![hold("A2", "2065550202", at(zone, day, 0, 0), None)];
    held(
        &state,
        &dsp,
        Some(&day.to_string()),
        now_held,
        vec![earlier],
    )
    .await;
    texted(&state, &dsp, "+12065550101", "Leaving the station", then).await;

    let at_dsp = dsp.clone();
    let found = state.read(move |db| of(db, &at_dsp, &avery)).await.unwrap();
    let work = found
        .iter()
        .find(|each| each.kind == ConversationKind::Work)
        .unwrap();
    assert_eq!(
        (
            work.conversation_id.as_deref(),
            work.driver_id.as_deref(),
            work.number.as_deref()
        ),
        (None, Some("A2"), Some("+12065550202"))
    );
}
