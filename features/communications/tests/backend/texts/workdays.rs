//! The line beginning each workday in a driver's work conversation: the route and van of each day
//! its texts were written, however far apart, and however many days the driver drove between.
use crate::backend::{
    support::{self, at, driver, filed, held, hold, route, started, texted, today, van},
    texts::reading,
};
use chrono::{Days, NaiveDate};

#[tokio::test]
async fn each_workday_shows_its_route_and_van_however_far_apart_and_however_many_days_between() {
    let (_root, state, dsp) = started();
    let (zone, day) = today(&state, &dsp).await;
    // Avery texted on 0101 four hundred days ago, a hundred and fifty days ago and yesterday, and
    // drove every day from the first, each in a van of its own, named by how many days in it is.
    let ago = |days| day.checked_sub_days(Days::new(days)).unwrap();
    let texted_on = [ago(400), ago(150), ago(1)];
    let driven: Vec<_> = ago(400)
        .iter_days()
        .take_while(|each| *each < day)
        .enumerate()
        .map(|(n, each)| {
            let wave = at(zone, each, 9, 0);
            route(
                &each.to_string(),
                "A1",
                &format!("CX{n}"),
                &format!("VIN{n}"),
                10,
                wave,
            )
        })
        .collect();
    let vans = ["0", "250", "399"].map(|n| van(&format!("VIN{n}"), n));
    support::know(
        &state,
        &dsp,
        vec![driver("avery", "Avery Diaz", "A1")],
        driven,
        vans.to_vec(),
    );
    let saved = texted_on.map(|each| {
        (
            each.to_string(),
            hold("A1", "2065550101", at(zone, each, 8, 0), None),
        )
    });
    held(&state, &dsp, None, Vec::new(), saved.to_vec()).await;
    let mut text = String::new();
    for each in texted_on {
        text = texted(
            &state,
            &dsp,
            "+12065550101",
            "At the station",
            at(zone, each, 9, 0),
        )
        .await;
    }
    let id = filed(&state, &dsp, &text).await.id;

    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let opened = state
        .read(move |db| reading::opened(db, &shared, &at_dsp, &id, None))
        .await
        .unwrap();
    let workdays: Vec<_> = opened
        .workdays
        .into_iter()
        .map(|each| (each.day, each.route, each.van))
        .collect();
    let shown =
        |each: NaiveDate, n: &str| (each.to_string(), Some(format!("CX{n}")), Some(n.to_owned()));
    assert_eq!(
        workdays,
        [
            shown(ago(400), "0"),
            shown(ago(150), "250"),
            shown(ago(1), "399")
        ]
    );
}
