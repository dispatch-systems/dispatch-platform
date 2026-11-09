//! The line beginning each workday in a driver's work conversation: the route and van of each day
//! its texts were written, however far apart, and however many days the driver drove between.
use crate::backend::{
    support::{self, at, driver, filed, phoned, route, saved, started, texted, today, van},
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
    let days = texted_on.map(|each| saved(each, &[("A1", "2065550101")]));
    phoned(&state, &dsp, None, Vec::new(), days.to_vec()).await;
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
    let read_id = id.clone();
    let opened = state
        .read(move |db| reading::opened(db, &shared, &at_dsp, &read_id, None))
        .await
        .unwrap();
    let anchor = opened.messages.first().unwrap().ts.clone();
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
    // A historical selection and its separate latest seed need their own exact local days.
    // Keep the original sparse-day/route/van assertions above, then enlarge this same fixture.
    let yesterday = at(zone, texted_on[2], 10, 0);
    let tail = at(zone, day, 10, 0);
    let (shared, at_dsp, conversation, template) =
        (state.clone(), dsp.clone(), id.clone(), text.clone());
    let page = state.run(move |db| {
        let connection = db.dsp(&at_dsp)?;
        connection.transaction(|| {
            for n in 0..220 {
                connection.exec("INSERT INTO phones_texts(id,device_id,client_id,direction,origin,number, \
                    body,at,change,conversation_id) SELECT ?1,device_id,?1,'in','phone',number, 'Window \
                    history',?2,?3,conversation_id FROM phones_texts WHERE id=?4",
                    rusqlite::params![format!("workday-window-{n:06}"), if n<160 { yesterday } else { tail }, n, template])?;
            }
            crate::backend::changes::changed_conversation(&connection, &conversation)
        })?;
        crate::backend::windows::read(db, &shared, &at_dsp,
            &crate::backend::model::Reference::Text(conversation),
            crate::backend::windows::Request::Anchor { place: anchor, leading: 0 })
    }).await.unwrap();
    assert_eq!((page.messages.len(), page.latest.len()), (150, 50));
    assert!(page.range.has_after);
    assert_eq!(
        page.workdays
            .iter()
            .map(|day| day.day.as_str())
            .collect::<Vec<_>>(),
        texted_on
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        page.latest_workdays
            .iter()
            .map(|day| (day.day.clone(), day.route.clone(), day.van.clone()))
            .collect::<Vec<_>>(),
        [(day.to_string(), None, None)]
    );
    assert!(page.context.previous.is_none());
    assert!(page.context.next.is_some());
    assert!(page.latest_context.previous.is_some());
    assert!(page.latest_context.next.is_none());
}
