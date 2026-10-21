//! The driver tool: whom a name finds, the days a lookup covers, what each feature is asked about
//! them, how a long answer is cut to fit, and a call as an agent makes it.
use super::*;
use crate::{
    api::types::ToolLevel,
    testing::{self, agent, call_tool, serving},
};
use dispatch_core::{
    manifest::people::{PersonRole, SourceInfo},
    testing::{self as common, bootstrapped},
};
use serde_json::{Value, json};
use std::sync::Mutex;

static AMAZON: SourceInfo = source("amazon");
static PAYCOM: SourceInfo = source("paycom");
static SLACK: SourceInfo = source("slack");
const fn source(id: &'static str) -> SourceInfo {
    SourceInfo {
        id,
        label: id,
        order: 10,
    }
}
/// The first day and the last every stand-in feature has collected.
const FIRST: &str = "2026-08-01";
const LAST: &str = "2026-09-18";

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| (*item).to_owned()).collect()
}
fn span(from: &str, to: &str) -> Span {
    let (from, to) = (parse(from).unwrap(), parse(to).unwrap());
    Span { from, to }
}
/// The days from `from` to `to` every stand-in feature has collected.
fn known(from: &str, to: &str) -> Vec<String> {
    span(from, to)
        .each()
        .filter(|day| (FIRST..=LAST).contains(&day.as_str()))
        .collect()
}
/// Whether `labels`, of what a cut of 30 kept, are all of the newest day's.
fn newest_kept<'a>(labels: impl Iterator<Item = &'a str>) -> bool {
    let labels: Vec<&str> = labels.collect();
    !labels.is_empty() && labels.len() <= 30 && labels.iter().all(|label| label.contains(LAST))
}
/// Each day of `from` to `to` collected, under the first of `ids`; none for no IDs.
fn rows_of(ids: &[String], from: &str, to: &str) -> Vec<(String, String)> {
    let days = known(from, to);
    let first = ids.first().into_iter();
    first
        .flat_map(|id| days.iter().map(|day| (id.clone(), day.clone())))
        .collect()
}
/// `day` at `hour`:`minute` UTC, in epoch milliseconds: an hour past 23 is on a later day.
fn at(day: &str, hour: i64, minute: i64) -> i64 {
    let midnight = parse(day).unwrap().and_hms_opt(0, 0, 0).unwrap();
    (midnight + Duration::minutes(hour * 60 + minute))
        .and_utc()
        .timestamp_millis()
}
fn person(code: &str, name: &str, ids: &[(&'static SourceInfo, &[&str])]) -> Person {
    Person {
        code: code.into(),
        name: name.into(),
        names: vec![name.into()],
        role: PersonRole::Driver,
        ids: ids
            .iter()
            .map(|(source, ids)| (Source::new(source), strings(ids)))
            .collect(),
        emails: vec![],
        phones: vec![],
        confirmed: false,
        photo: None,
    }
}
/// A DSP on Los Angeles time.
fn at_a_dsp() -> (tempfile::TempDir, Store, Dsp) {
    testing::install();
    let (root, db, id) = bootstrapped();
    common::set_dsp(&db, &id, "Fixture Delivery", "America/Los_Angeles").unwrap();
    let dsp = db.find_dsp(&id).unwrap();
    (root, db, dsp)
}

#[test]
fn finds_one_person_by_code_or_a_choice_and_asks_which_of_several() {
    let mut people = vec![
        person("K7QH3M", "Cam Hayes", &[]),
        person("R2VX8N", "Avery Morgan", &[]),
        person("T5WB9C", "Avery Mendez", &[]),
    ];
    let jordans = (b'A'..b'M').map(|last| format!("Jordan {}ates", last as char));
    people.extend(jordans.map(|name| person(&name[7..].to_ascii_uppercase(), &name, &[])));
    let everyone = Everyone::new(people, BTreeMap::new());
    let found = |asked: &str| {
        find(&everyone, "Fixture Delivery", asked)
            .map(|person| person.code.clone())
            .map_err(|refusal| (refusal.code, refusal.choices))
    };

    for asked in ["K7QH3M", " k7qh3m "] {
        assert_eq!(found(asked), Ok("K7QH3M".to_owned()), "{asked}");
    }
    let (code, choices) = found("Avery").unwrap_err();
    assert_eq!(code, "driver_ambiguous");
    assert_eq!(choices, ["Avery Mendez (T5WB9C)", "Avery Morgan (R2VX8N)"]);
    // A choice, given back as it is, is the one it names.
    for (choice, code) in choices.iter().zip(["T5WB9C", "R2VX8N"]) {
        assert_eq!(found(choice), Ok(code.to_owned()), "{choice}");
    }
    // Twelve to choose from are offered ten.
    assert_eq!(found("Jordan").unwrap_err().1.len(), 10);
    assert_eq!(found("Nobody Here").unwrap_err().0, "driver_not_found");
    assert_eq!(found(" ").unwrap_err().0, "invalid_input");
}

#[test]
fn a_lookup_names_31_days_at_most_one_way() {
    let today = "2026-10-07";
    let ranged =
        |from: Option<&str>, to: Option<&str>, last_days| -> std::result::Result<_, String> {
            let (from, to) = (from.map(Into::into), to.map(Into::into));
            let lookup = Lookup {
                driver: "Cam".into(),
                from,
                to,
                last_days,
                sections: None,
            };
            let span = range(&lookup, parse(today).unwrap()).map_err(|refusal| refusal.message)?;
            Ok((span.from.to_string(), span.to.to_string()))
        };
    let days = |from: &str| Ok((from.to_owned(), today.to_owned()));
    assert_eq!(ranged(None, None, None), days("2026-10-01"));
    assert_eq!(ranged(None, None, Some(31)), days("2026-09-07"));
    assert_eq!(ranged(Some("2026-09-07"), None, None), days("2026-09-07"));
    for (from, to, last_days, refused) in [
        (Some("2026-09-06"), Some(today), None, "31 days at most"),
        (None, None, Some(32), "1 to 31"),
        (Some("2026-10-01"), Some("2026-10-08"), None, "after today"),
        (None, Some("2026-10-01"), None, "`to` ends a range"),
        (Some("2026-10-01"), None, Some(3), "not both"),
    ] {
        let message = ranged(from, to, last_days).unwrap_err();
        assert!(message.contains(refused), "{refused}: {message}");
    }
}

/// Every feature at once, answering for the first ID it's asked about: on each day collected,
/// `rows` of each long list, each labelled with its day. It notes the IDs each reader was asked
/// about.
struct Fake {
    rows: usize,
    asked: Mutex<Vec<(&'static str, Vec<String>)>>,
}
impl Fake {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            asked: Mutex::default(),
        }
    }
    /// Notes that `reader` was asked about `ids`.
    fn note(&self, reader: &'static str, ids: &[String]) {
        self.asked.lock().unwrap().push((reader, ids.to_vec()));
    }
    /// Who was asked what, by reader.
    fn asked(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut asked = self.asked.lock().unwrap().clone();
        asked.sort();
        asked
    }
    /// The first day collected and the last, as `make` makes them.
    fn first_to_last<T>(&self, make: fn(String, String) -> T) -> Result<Option<T>> {
        Ok(Some(make(FIRST.into(), LAST.into())))
    }
}
/// Every section, read from `fake`.
fn every(fake: &Fake) -> Readers<'_> {
    Readers {
        scorecards: Some(fake),
        daily: Some(fake),
        routes: Some(fake),
        timecards: Some(fake),
        roster: Some(fake),
        chat: Some(fake),
        inspections: Some(fake),
    }
}
impl DriverScorecards for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    /// Every week but 2026-W37, September 6 to 12.
    fn collected(&self, _: &Store, _: &str) -> Result<Option<scorecard::Collected>> {
        let weeks = strings(&["2026-W36", "2026-W38"]);
        let (from, through) = ("2026-08-30".into(), "2026-09-19".into());
        Ok(Some(scorecard::Collected {
            from,
            through,
            weeks,
        }))
    }
    fn scorecards(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        _: &str,
        _: &str,
    ) -> Result<Vec<scorecard::Scorecard>> {
        self.note("scorecards", ids);
        Ok(vec![])
    }
    fn packages(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        _: &str,
        _: &str,
    ) -> Result<Vec<scorecard::Package>> {
        self.note("packages", ids);
        let Some(id) = ids.first() else {
            return Ok(vec![]);
        };
        let package = |n| scorecard::Package {
            kind: scorecard::PackageKind::Return,
            week: "2026-W38".into(),
            date: None,
            tracking_id: format!("TBA{n:012}"),
            driver_id: id.clone(),
            impacting: Some(true),
            reasons: strings(&["BUSINESS CLOSED"]),
        };
        Ok((0..self.rows).map(package).collect())
    }
}
impl DailyMetrics for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<performance::Collected>> {
        self.first_to_last(|from, through| performance::Collected { from, through })
    }
    fn days(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<performance::Days> {
        self.note("daily", ids);
        let rows = rows_of(ids, from, to);
        let returned = |date: &str, n| performance::Returned {
            tracking_id: Some(format!("R {date} {n:02}")),
            reason: Some("BUSINESS CLOSED".into()),
            impacting: Some(false),
            exempted: Some(true),
            exemption_reason: None,
            coaching: None,
        };
        let event = |date: &str, n| performance::SafetyEvent {
            event_id: None,
            kind: Some("Speeding".into()),
            subtype: None,
            severity: Some("LOW".into()),
            started: Some(format!("{date} 10:{n:02}:00")),
            impacting: Some(true),
            assessed: true,
        };
        let day = |(id, date): &(String, String)| performance::Day {
            date: date.clone(),
            id: id.clone(),
            quality: None,
            safety: None,
            feedback: None,
            behaviors: None,
            returns: None,
            returned: (0..self.rows).map(|n| returned(date, n)).collect(),
            events: (0..self.rows).map(|n| event(date, n)).collect(),
        };
        Ok(performance::Days {
            days: rows.iter().map(day).collect(),
            collected: known(from, to),
        })
    }
}
impl DriverRoutes for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<route::Collected>> {
        self.first_to_last(|from, through| route::Collected { from, through })
    }
    /// A route each day collected, ending after midnight UTC; the last day still under way.
    fn routes(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<route::Driven> {
        self.note("routes", ids);
        let rows = rows_of(ids, from, to);
        let undelivered = |day: &str, n| route::Undelivered {
            tracking_id: Some(format!("U {day} {n:02}")),
            state: Some("UNDELIVERABLE".into()),
            reason: Some("BUSINESS_CLOSED".into()),
            attempted_at: Some(at(day, 20, 5)),
        };
        let driven = |(id, day): &(String, String)| route::RouteDay {
            day: day.clone(),
            driver_id: id.clone(),
            route_code: Some("CX12".into()),
            status: Some("COMPLETE".into()),
            progress: None,
            vin: None,
            wave_start_at: None,
            planned_departure_at: None,
            departed_at: Some(at(day, 15, 30)),
            first_delivery_at: None,
            last_delivery_at: None,
            session_end_at: Some(at(day, 31, 40)),
            stops: 120,
            stops_completed: 118,
            delivered: 200,
            not_delivered: 0,
            removed: 0,
            breaks_secs: None,
            overtime_secs: None,
            undelivered: (0..self.rows).map(|n| undelivered(day, n)).collect(),
        };
        let days = known(from, to).into_iter();
        let days = days
            .map(|day| route::CollectedDay {
                complete: day != LAST,
                day,
            })
            .collect();
        Ok(route::Driven {
            days,
            routes: rows.iter().map(driven).collect(),
        })
    }
}
impl timecard::Timecards for Fake {
    fn source(&self) -> Source {
        Source::new(&PAYCOM)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<timecard::Collected>> {
        self.first_to_last(|from, through| timecard::Collected { from, through })
    }
    /// A timecard each day collected; the last day still under way.
    fn timecards(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<timecard::Days> {
        self.note("timecards", ids);
        let rows = rows_of(ids, from, to);
        let card = |(id, date): &(String, String)| timecard::Day {
            date: date.clone(),
            id: id.clone(),
            status: "Complete".into(),
            hours: 9.5,
            worked_minutes: Some(570),
            punches: vec![],
            meals: vec![],
            review: false,
        };
        let unknown = span(from, to)
            .each()
            .filter(|day| day.as_str() > LAST)
            .collect();
        let partial = known(from, to)
            .into_iter()
            .filter(|day| day == LAST)
            .collect();
        Ok(timecard::Days {
            days: rows.iter().map(card).collect(),
            unknown,
            partial,
        })
    }
}
impl AmazonRoster for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<String>> {
        Ok(Some("2026-09-18T12:00:00Z".into()))
    }
    fn associates(&self, _: &Store, _: &str, ids: &[String]) -> Result<Vec<profiles::Associate>> {
        self.note("roster", ids);
        let associate = |id: &String| profiles::Associate {
            id: id.clone(),
            listed: true,
            status: "ACTIVE".into(),
            roles: strings(&["Driver"]),
            qualifications: vec![],
            license_expires: None,
            last_listed: "2026-09-18T12:00:00Z".into(),
        };
        Ok(ids.iter().map(associate).collect())
    }
}
impl ChatAccounts for Fake {
    fn source(&self) -> Source {
        Source::new(&SLACK)
    }
    fn accounts(&self, _: &Store, _: &str, ids: &[String]) -> Result<Vec<slack::ChatAccount>> {
        self.note("chat", ids);
        let account = |id: &String| slack::ChatAccount {
            id: id.clone(),
            workspace: "Fixture".into(),
            user: "U0FIXTURE".into(),
            display_name: None,
            active: true,
            guest: false,
            broken: false,
        };
        Ok(ids.iter().map(account).collect())
    }
}
impl ShortInspections for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<dvic::Collected>> {
        self.first_to_last(|from, through| dvic::Collected { from, through })
    }
    fn drivers(&self, _: &Store, _: &str) -> Result<Vec<dvic::Driver>> {
        Ok(vec![])
    }
    /// `rows` short inspections on the last day collected, a minute apart.
    fn inspections(&self, _: &Store, _: &str, asked: &Asked<'_>) -> Result<dvic::Page> {
        let id = asked.driver.unwrap_or_default().to_owned();
        self.note("inspections", std::slice::from_ref(&id));
        let inspection = |n| dvic::Inspection {
            date: LAST.into(),
            start_time: format!("{LAST} 06:{n:02}:00"),
            driver_id: id.clone(),
            driver_name: "Cam Hayes".into(),
            vehicle_type: "CDV".into(),
            vin: "1FIXTURE000000001".into(),
            duration_seconds: 60.0,
            minimum_seconds: 90,
            short_by_seconds: 30.0,
        };
        Ok(dvic::Page {
            inspections: (0..self.rows).map(inspection).collect(),
            next: None,
        })
    }
}

#[test]
fn asks_each_feature_about_the_persons_ids_of_its_source_and_says_what_it_hasnt_collected() {
    let (_root, db, dsp) = at_a_dsp();
    let fake = Fake::new(1);
    // The roster's feature is off at the DSP.
    let readers = Readers {
        roster: None,
        ..every(&fake)
    };
    // No Paycom ID. DVIC takes only IDs of letters, digits, `_`, `-` or `.`; the rest take any.
    let amazon = ["A1", "A2", "A 3"];
    let cam = person(
        "K7QH3M",
        "Cam Hayes",
        &[(&AMAZON, &amazon), (&SLACK, &["S1"])],
    );
    let found = answer(&readers, &db, &dsp, &cam, span("2026-09-06", "2026-09-19")).unwrap();

    let amazon = strings(&amazon);
    let asked = [("chat", strings(&["S1"])), ("daily", amazon.clone())]
        .into_iter()
        .chain(["A1", "A2"].map(|id| ("inspections", strings(&[id]))))
        .chain(["packages", "routes", "scorecards"].map(|reader| (reader, amazon.clone())))
        .chain([("timecards", vec![])]);
    assert_eq!(fake.asked(), asked.collect::<Vec<_>>());
    assert_eq!(found.inspections.as_ref().unwrap().ids, ["A1", "A2"]);
    assert!(found.roster.is_none());

    // Known by no Paycom ID, they have no timecards, though which days are known still shows.
    let timecards = found.timecards.unwrap();
    assert!(timecards.ids.is_empty() && timecards.days.is_empty());
    assert_eq!(
        (timecards.unknown_days, timecards.partial_days),
        (strings(&["2026-09-19"]), strings(&[LAST]))
    );
    // What isn't collected is unknown: the week between two collected, and the day after the last.
    assert_eq!(found.scorecards.unwrap().unknown_weeks, ["2026-W37"]);
    assert_eq!(found.daily.unwrap().unknown_days, ["2026-09-19"]);
    assert_eq!(found.inspections.unwrap().unknown_days, ["2026-09-19"]);
    let routes = found.routes.unwrap();
    assert_eq!(
        (routes.unknown_days, routes.partial_days),
        (strings(&["2026-09-19"]), strings(&[LAST]))
    );
    // Read on the DSP's clock, 15:30 UTC being 08:30 in Los Angeles; one past midnight has its date.
    let first = &routes.routes[0];
    assert_eq!(
        (first.departed.as_deref(), first.ended.as_deref()),
        (Some("08:30"), Some("2026-09-07 00:40"))
    );
}

#[test]
fn cuts_long_lists_to_fit_keeping_the_newest_and_counting_the_rest() {
    let (_root, db, dsp) = at_a_dsp();
    let cam = person(
        "K7QH3M",
        "Cam Hayes",
        &[(&AMAZON, &["A1"]), (&PAYCOM, &["P1"]), (&SLACK, &["S1"])],
    );
    let looked_up =
        |readers: Readers, from: &str| answer(&readers, &db, &dsp, &cam, span(from, LAST)).unwrap();

    // A week of a little is whole.
    let fake = Fake::new(2);
    assert!(!looked_up(every(&fake), "2026-09-12").trimmed);

    // A cap alone trims: of 35 short inspections on one day, the newest 30 are kept.
    let fake = Fake::new(35);
    let day = looked_up(
        Readers {
            inspections: Some(&fake),
            ..Readers::default()
        },
        LAST,
    );
    let short = day.inspections.unwrap();
    assert!(day.trimmed);
    assert_eq!((short.inspections.len(), short.more_inspections), (30, 5));
    assert_eq!(short.inspections[0].start_time, format!("{LAST} 06:34:00"));

    // A month of a lot: each list is cut to its cap, then the longest further, until it fits.
    let fake = Fake::new(60);
    let month = looked_up(every(&fake), "2026-08-20");
    assert!(month.trimmed && json_len(&month) <= ROOM);
    let cards = month.scorecards.unwrap();
    assert!(cards.packages.len() <= 40);
    assert_eq!(cards.packages.len() + cards.more_packages, 60);
    assert_eq!(
        cards.packages.last().unwrap().tracking_id,
        "TBA000000000059"
    );
    let daily = month.daily.unwrap();
    assert_eq!(daily.days.len() + daily.more_days, 30);
    assert_eq!(daily.days.last().unwrap().date, LAST);
    let routes = month.routes.unwrap();
    assert_eq!(routes.routes.len() + routes.more_routes, 30);
    assert_eq!(routes.routes.last().unwrap().day, LAST);
    let timecards = month.timecards.unwrap();
    assert_eq!(timecards.days.len() + timecards.more_days, 30);
    assert_eq!(timecards.days.last().unwrap().date, LAST);
    // Of the lists within days, only the newest day's survive.
    let returned = daily.days.iter().flat_map(|day| &day.returned);
    assert!(newest_kept(
        returned.filter_map(|it| it.tracking_id.as_deref())
    ));
    let events = daily.days.iter().flat_map(|day| &day.events);
    assert!(newest_kept(events.filter_map(|it| it.started.as_deref())));
    let undelivered = routes.routes.iter().flat_map(|route| &route.undelivered);
    assert!(newest_kept(
        undelivered.filter_map(|it| it.tracking_id.as_deref())
    ));
}

#[cfg(feature = "dvic")]
#[tokio::test]
async fn an_agent_looks_up_a_driver_with_what_the_features_on_know_of_them() {
    use super::super::dvic_short_inspections::tests::{publish, ready, request};
    use dispatch_driver_match::DriverMatchStore;
    let (_root, db, id) = ready();
    publish(
        &db,
        &id,
        "older",
        &dispatch_cortex::dvic::fixture(&request(&["2026-W38"])).unwrap(),
    );
    // Driver Match gives the driver DVIC names a code, as after a collection.
    db.write_pass(&id, db.read_pass(&id).unwrap()).unwrap();
    let caller = agent(&db, &[&id], &[("driver", ToolLevel::Read)]);
    let state = serving(db);
    let call = |arguments: Value| {
        let (state, caller) = (state.clone(), caller.clone());
        async move {
            let answered = call_tool(&state, &caller, "driver", arguments)
                .await
                .unwrap();
            (answered.data, answered.text.unwrap_or_default())
        }
    };

    let (found, text) =
        call(json!({"driver": "fixture driver", "from": "2026-09-13", "to": "2026-09-19"})).await;
    assert_eq!(found["person"]["ids"], json!({"amazon": ["driver-1"]}));
    let short = &found["inspections"];
    assert_eq!(
        (&short["ids"], &short["unknown_days"]),
        (&json!(["driver-1"]), &json!([]))
    );
    assert_eq!(short["inspections"][0]["date"], "2026-09-19");
    assert!(text.contains("1 short DVIC inspection"), "{text}");
    // Each other section is null where nothing at the DSP offers its feature's contract.
    let at = id.clone();
    let offered = state.read(move |db| {
        Ok([
            ("scorecards", scorecard::for_dsp(db, &at)?.is_some()),
            ("daily", performance::for_dsp(db, &at)?.is_some()),
            ("routes", route::for_dsp(db, &at)?.is_some()),
            ("timecards", timecard::for_dsp(db, &at)?.is_some()),
            ("roster", profiles::for_dsp(db, &at)?.is_some()),
            ("chat", slack::for_dsp(db, &at)?.is_some()),
        ])
    });
    for (section, offered) in offered.await.unwrap() {
        assert_eq!(found[section].is_null(), !offered, "{section}");
    }

    // By their code, the last 7 days, and only the sections asked for.
    let code = found["person"]["code"].as_str().unwrap();
    let (week, _) = call(json!({"driver": code, "sections": ["roster"]})).await;
    let today = clock::today(chrono_tz::America::Los_Angeles);
    let days = [today - Duration::days(6), today].map(|day| json!(day.to_string()));
    assert_eq!([&week["from"], &week["to"]], [&days[0], &days[1]]);
    assert_eq!(week["inspections"], Value::Null);
}
