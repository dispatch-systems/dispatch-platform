//! A person's conversations of each kind: their work one follows the ID with a work phone now,
//! whichever of their IDs their earlier texts were under; of two IDs with one, the one on the
//! busier route today.
use super::*;
use crate::backend::support::{
    self, DRIVERS, at, phoned, phones, route, routed, saved, texted, today,
};
use dispatch_core::manifest::people::PersonRole;
use std::collections::BTreeMap;

#[tokio::test]
async fn the_work_chip_goes_to_the_id_with_a_work_phone_now_not_an_earlier_aliases() {
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
    // Yesterday, as A1, they texted from 0101; today, as A2, they have 0202 and haven't texted.
    let then = at(zone, yesterday, 9, 0);
    let today = day.to_string();
    let earlier = saved(yesterday, &[("A1", "2065550101")]);
    let now = phones(&[("A2", "2065550202")]);
    phoned(&state, &dsp, Some(&today), now, vec![earlier.clone()]).await;
    texted(&state, &dsp, "+12065550101", "Leaving the station", then).await;
    let work = || {
        let (at_dsp, avery) = (dsp.clone(), avery.clone());
        let found = state.read(move |db| of(db, &at_dsp, &avery));
        async move {
            let found = found.await.unwrap();
            let work = found
                .into_iter()
                .find(|each| each.kind == ConversationKind::Work);
            let work = work.unwrap();
            (work.conversation_id, work.driver_id, work.number)
        }
    };
    let a2 = (None, Some("A2".into()), Some("+12065550202".into()));
    assert_eq!(work().await, a2);

    // As A1 too they have a phone of their own today, 0303, on a route with fewer stops than A2's:
    // the chip goes to A2's, on the route their day shows.
    let wave = at(zone, day, 9, 0);
    routed(
        &dsp,
        vec![
            route(&today, "A1", "CX40", "1FTVAN040", 12, wave),
            route(&today, "A2", "CX14", "1FTVAN214", 168, wave),
        ],
    );
    let both = phones(&[("A1", "2065550303"), ("A2", "2065550202")]);
    phoned(&state, &dsp, Some(&today), both, vec![earlier]).await;
    assert_eq!(work().await, a2);
}
