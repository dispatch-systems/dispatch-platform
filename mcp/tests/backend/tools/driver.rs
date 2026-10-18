//! The driver tool: whom a name finds, what each feature is asked about them, how a long answer
//! is cut to fit, and a call as an agent makes it.
use super::*;
use crate::{
    api::types::ToolLevel,
    testing::{self, agent, call_tool, serving},
    toolbox::Answered,
};
use dispatch_core::{
    manifest::people::{PersonRole, SourceInfo},
    testing::{self as common, bootstrapped},
};
use serde_json::{Value, json};
use std::sync::Mutex;

static AMAZON: SourceInfo = SourceInfo {
    id: "amazon",
    label: "Amazon",
    order: 10,
};
static PAYCOM: SourceInfo = SourceInfo {
    id: "paycom",
    label: "Paycom",
    order: 20,
};
static SLACK: SourceInfo = SourceInfo {
    id: "slack",
    label: "Slack",
    order: 30,
};
/// The first day and the last every stand-in feature has collected.
const FIRST: &str = "2026-08-01";
const LAST: &str = "2026-09-18";

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| (*item).to_owned()).collect()
}
fn span(from: &str, to: &str) -> Span {
    Span {
        from: parse(from).unwrap(),
        to: parse(to).unwrap(),
    }
}
/// The days from `from` to `to` collected.
fn known(from: &str, to: &str) -> Vec<String> {
    let collected = |day: &String| (FIRST..=LAST).contains(&day.as_str());
    span(from, to).each().filter(collected).collect()
}
/// `day` at `hour`:`minute` UTC, in epoch milliseconds: an hour past 23 is on a later day.
fn at(day: &str, hour: i64, minute: i64) -> i64 {
    let midnight = parse(day).unwrap().and_hms_opt(0, 0, 0).unwrap();
    (midnight + Duration::minutes(hour * 60 + minute))
        .and_utc()
        .timestamp_millis()
}
/// Each of `days` under the first of `ids`, as a stand-in feature has rows: none for no IDs.
fn rows_of(ids: &[String], days: &[String]) -> Vec<(String, String)> {
    let first = ids.first().into_iter();
    first
        .flat_map(|id| days.iter().map(|day| (id.clone(), day.clone())))
        .collect()
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
    let (code, choices) = found("Jordan").unwrap_err();
    assert_eq!((code.as_str(), choices.len()), ("driver_ambiguous", 10));
    assert_eq!(found("Nobody Here").unwrap_err().0, "driver_not_found");
    assert_eq!(found(" ").unwrap_err().0, "invalid_input");
}

#[test]
fn a_lookup_names_31_days_at_most_one_way() {
    let today = "2026-10-07";
    let ranged = |from: Option<&str>, to: Option<&str>, last_days: Option<i64>| {
        let lookup = Lookup {
            driver: "Cam".into(),
            from: from.map(Into::into),
            to: to.map(Into::into),
            last_days,
            sections: None,
        };
        range(&lookup, parse(today).unwrap())
            .map(|span| (span.from.to_string(), span.to.to_string()))
            .map_err(|refusal| refusal.message)
    };
    let days = |from: &str, to: &str| Ok((from.to_owned(), to.to_owned()));
    assert_eq!(ranged(None, None, None), days("2026-10-01", today));
    assert_eq!(ranged(None, None, Some(31)), days("2026-09-07", today));
    assert_eq!(
        ranged(Some("2026-09-07"), None, None),
        days("2026-09-07", today)
    );
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

/// Every feature at once, as each would answer for the IDs it's asked about: `rows` of each
/// long list, on each day collected. It notes the IDs each reader was asked about.
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
    fn ask(&self, reader: &'static str, ids: &[String]) {
        self.asked.lock().unwrap().push((reader, ids.to_vec()));
    }
    fn asked(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut asked = self.asked.lock().unwrap().clone();
        asked.sort();
        asked
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
        Ok(Some(scorecard::Collected {
            from: "2026-08-30".into(),
            through: "2026-09-19".into(),
            weeks: strings(&["2026-W36", "2026-W38"]),
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
        self.ask("scorecards", ids);
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
        self.ask("packages", ids);
        let package = |(id, n): (String, usize)| scorecard::Package {
            kind: scorecard::PackageKind::Return,
            week: "2026-W38".into(),
            date: None,
            tracking_id: format!("TBA{n:012}"),
            driver_id: id,
            impacting: Some(true),
            reasons: strings(&["BUSINESS CLOSED"]),
        };
        let first = ids.first().into_iter();
        Ok(
            (first.flat_map(|id| (0..self.rows).map(|n| (id.clone(), n))))
                .map(package)
                .collect(),
        )
    }
}
impl DailyMetrics for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<performance::Collected>> {
        Ok(Some(performance::Collected {
            from: FIRST.into(),
            through: LAST.into(),
        }))
    }
    fn days(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<performance::Days> {
        self.ask("daily", ids);
        let collected = known(from, to);
        let day = |(id, date): (String, String)| {
            let started = Some(format!("{date} 10:00:00"));
            performance::Day {
                date,
                id,
                quality: None,
                safety: None,
                feedback: None,
                behaviors: None,
                returns: None,
                returned: (0..self.rows)
                    .map(|n| performance::Returned {
                        tracking_id: Some(format!("TBA{n:012}")),
                        reason: Some("BUSINESS CLOSED".into()),
                        impacting: Some(false),
                        exempted: Some(true),
                        exemption_reason: None,
                        coaching: None,
                    })
                    .collect(),
                events: (0..self.rows)
                    .map(|n| performance::SafetyEvent {
                        event_id: Some(format!("event-{n}")),
                        kind: Some("Speeding".into()),
                        subtype: None,
                        severity: Some("LOW".into()),
                        started: started.clone(),
                        impacting: Some(true),
                        assessed: true,
                    })
                    .collect(),
            }
        };
        Ok(performance::Days {
            days: rows_of(ids, &collected).into_iter().map(day).collect(),
            collected,
        })
    }
}
impl DriverRoutes for Fake {
    fn source(&self) -> Source {
        Source::new(&AMAZON)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<route::Collected>> {
        Ok(Some(route::Collected {
            from: FIRST.into(),
            through: LAST.into(),
        }))
    }
    /// A route each day collected, the last still under way.
    fn routes(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<route::Driven> {
        self.ask("routes", ids);
        let days = known(from, to);
        let driven = |(driver_id, day): (String, String)| route::RouteDay {
            departed_at: Some(at(&day, 15, 30)),
            session_end_at: Some(at(&day, 31, 40)),
            undelivered: (0..self.rows)
                .map(|n| route::Undelivered {
                    tracking_id: Some(format!("TBA{n:012}")),
                    state: Some("UNDELIVERABLE".into()),
                    reason: Some("BUSINESS_CLOSED".into()),
                    attempted_at: Some(at(&day, 20, 5)),
                })
                .collect(),
            day,
            driver_id,
            route_code: Some("CX12".into()),
            status: Some("COMPLETE".into()),
            progress: Some("ON_TIME".into()),
            vin: None,
            planned_departure_at: None,
            first_delivery_at: None,
            last_delivery_at: None,
            stops: 120,
            stops_completed: 118,
            delivered: 200,
            not_delivered: self.rows as i64,
            removed: 0,
            breaks_secs: Some(1800),
            overtime_secs: None,
        };
        Ok(route::Driven {
            days: (days.iter())
                .map(|day| route::CollectedDay {
                    day: day.clone(),
                    complete: day != LAST,
                })
                .collect(),
            routes: rows_of(ids, &days).into_iter().map(driven).collect(),
        })
    }
}
impl timecard::Timecards for Fake {
    fn source(&self) -> Source {
        Source::new(&PAYCOM)
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<timecard::Collected>> {
        Ok(Some(timecard::Collected {
            from: FIRST.into(),
            through: LAST.into(),
        }))
    }
    fn timecards(
        &self,
        _: &Store,
        _: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<timecard::Days> {
        self.ask("timecards", ids);
        let days = known(from, to);
        let punches = [
            (timecard::PunchKind::InDay, "07:00"),
            (timecard::PunchKind::OutLunch, "12:00"),
            (timecard::PunchKind::InLunch, "12:30"),
            (timecard::PunchKind::OutDay, "17:00"),
        ];
        let card = |(id, date): (String, String)| timecard::Day {
            date,
            id,
            status: "Complete".into(),
            hours: 9.5,
            worked_minutes: Some(570),
            punches: (punches.iter())
                .map(|(kind, time)| timecard::Punch {
                    kind: *kind,
                    time: Some((*time).into()),
                    next_day: false,
                })
                .collect(),
            meals: vec![timecard::Meal {
                out: Some("12:00".into()),
                back: Some("12:30".into()),
                minutes: Some(30),
            }],
            review: false,
        };
        Ok(timecard::Days {
            unknown: span(from, to)
                .each()
                .filter(|day| !days.contains(day))
                .collect(),
            partial: days.iter().filter(|day| *day == LAST).cloned().collect(),
            days: rows_of(ids, &days).into_iter().map(card).collect(),
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
        self.ask("roster", ids);
        let associate = |id: &String| profiles::Associate {
            id: id.clone(),
            listed: true,
            status: "ACTIVE".into(),
            roles: strings(&["Driver"]),
            qualifications: strings(&["CDV"]),
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
        self.ask("chat", ids);
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
        Ok(Some(dvic::Collected {
            from: FIRST.into(),
            through: LAST.into(),
        }))
    }
    fn drivers(&self, _: &Store, _: &str) -> Result<Vec<dvic::Driver>> {
        Ok(vec![])
    }
    fn inspections(&self, _: &Store, _: &str, asked: &Asked<'_>) -> Result<dvic::Page> {
        let id = asked.driver.unwrap_or_default().to_owned();
        self.ask("inspections", std::slice::from_ref(&id));
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
    let cam = person(
        "K7QH3M",
        "Cam Hayes",
        &[(&AMAZON, &["A1", "A2", "A 3"]), (&SLACK, &["S1"])],
    );
    let found = answer(&readers, &db, &dsp, &cam, span("2026-09-06", "2026-09-19")).unwrap();

    let amazon = strings(&["A1", "A2", "A 3"]);
    assert_eq!(
        fake.asked(),
        [
            ("chat", strings(&["S1"])),
            ("daily", amazon.clone()),
            ("inspections", strings(&["A1"])),
            ("inspections", strings(&["A2"])),
            ("packages", amazon.clone()),
            ("routes", amazon.clone()),
            ("scorecards", amazon.clone()),
            ("timecards", vec![]),
        ]
    );
    assert_eq!(found.scorecards.unwrap().ids, amazon);
    assert_eq!(found.inspections.as_ref().unwrap().ids, ["A1", "A2"]);
    assert_eq!(found.chat.unwrap().accounts.len(), 1);
    assert!(found.roster.is_none());

    // Known by no Paycom ID, they have no timecards; the days are still known or not, and so far.
    let timecards = found.timecards.unwrap();
    assert!(timecards.ids.is_empty() && timecards.days.is_empty());
    assert_eq!(timecards.unknown_days, ["2026-09-19"]);
    assert_eq!(timecards.partial_days, [LAST]);

    let routes = found.routes.unwrap();
    assert_eq!(routes.partial_days, [LAST]);
    // Read on the DSP's clock, 15:30 UTC being 08:30 in Los Angeles; one past midnight has its date.
    let first = &routes.routes[0];
    assert_eq!(
        (first.departed.as_deref(), first.ended.as_deref()),
        (Some("08:30"), Some("2026-09-07 00:40"))
    );
    for unknown in [
        routes.unknown_days,
        found.daily.unwrap().unknown_days,
        found.inspections.unwrap().unknown_days,
    ] {
        assert_eq!(unknown, ["2026-09-19"]);
    }
}

/// How many `list` holds within all of `items`.
fn within<T, U>(items: &[T], list: impl Fn(&T) -> &Vec<U>) -> usize {
    items.iter().map(|item| list(item).len()).sum()
}

#[test]
fn cuts_long_lists_to_fit_keeping_the_newest_and_counting_the_rest() {
    let (_root, db, dsp) = at_a_dsp();
    let cam = person(
        "K7QH3M",
        "Cam Hayes",
        &[(&AMAZON, &["A1"]), (&PAYCOM, &["P1"]), (&SLACK, &["S1"])],
    );
    let fake = Fake::new(2);
    let mut week = answer(&every(&fake), &db, &dsp, &cam, span("2026-09-12", LAST)).unwrap();
    fit(&mut week);
    assert!(!week.trimmed);
    assert_eq!(week.scorecards.unwrap().more_packages, 0);
    assert_eq!(week.routes.unwrap().routes.len(), 7);

    // A month of a lot: each long list is cut to its cap, then the longest further until it fits.
    let fake = Fake::new(60);
    let mut month = answer(&every(&fake), &db, &dsp, &cam, span("2026-08-20", LAST)).unwrap();
    fit(&mut month);
    assert!(month.trimmed);
    assert!(json_len(&month) <= ROOM);
    let cards = month.scorecards.unwrap();
    assert!(cards.packages.len() <= 40);
    assert_eq!(cards.packages.len() + cards.more_packages, 60);
    assert_eq!(
        cards.packages.last().unwrap().tracking_id,
        "TBA000000000059"
    );
    let short = month.inspections.unwrap();
    assert!(short.inspections.len() <= 30);
    assert_eq!(short.inspections.len() + short.more_inspections, 60);
    let daily = month.daily.unwrap();
    assert_eq!(daily.days.len() + daily.more_days, 30);
    assert_eq!(daily.days.last().unwrap().date, LAST);
    assert!(within(&daily.days, |day| &day.returned) <= 30);
    assert!(within(&daily.days, |day| &day.events) <= 30);
    let routes = month.routes.unwrap();
    assert_eq!(routes.routes.len() + routes.more_routes, 30);
    assert_eq!(routes.routes.last().unwrap().day, LAST);
    assert!(within(&routes.routes, |route| &route.undelivered) <= 30);
    let timecards = month.timecards.unwrap();
    assert_eq!(timecards.days.len() + timecards.more_days, 30);

    // A cap alone trims too: of 35 short inspections on one day, the newest 30 are kept.
    let fake = Fake::new(35);
    let readers = Readers {
        inspections: Some(&fake),
        ..Readers::default()
    };
    let mut day = answer(&readers, &db, &dsp, &cam, span(LAST, LAST)).unwrap();
    fit(&mut day);
    assert!(day.trimmed);
    let short = day.inspections.unwrap();
    assert_eq!((short.inspections.len(), short.more_inspections), (30, 5));
    assert_eq!(short.inspections[0].start_time, format!("{LAST} 06:34:00"));
}

fn reply(answer: Answer<Answered>) -> (Value, String) {
    let answered = answer.unwrap();
    (answered.data, answered.text.unwrap_or_default())
}

#[cfg(feature = "dvic")]
#[tokio::test]
async fn an_agent_looks_up_a_driver_with_what_the_features_on_know_of_them() {
    use super::super::dvic_short_inspections::tests::{publish, ready, request};
    use dispatch_driver_match::DriverMatchStore;
    let (_root, db, id) = ready();
    let week = dispatch_cortex::dvic::fixture(&request(&["2026-W38"])).unwrap();
    publish(&db, &id, "older", &week);
    // Driver Match gives the driver DVIC names a code, as after a collection.
    db.write_pass(&id, db.read_pass(&id).unwrap()).unwrap();
    let caller = agent(&db, &[&id], &[("driver", ToolLevel::Read)]);
    let state = serving(db);
    let call = |arguments: Value| {
        let (state, caller) = (state.clone(), caller.clone());
        async move { call_tool(&state, &caller, "driver", arguments).await }
    };

    let (found, text) = reply(
        call(json!({"driver": "fixture driver", "from": "2026-09-13", "to": "2026-09-19"})).await,
    );
    assert_eq!(found["person"]["name"], "Fixture Driver");
    assert_eq!(found["person"]["ids"], json!({"amazon": ["driver-1"]}));
    let short = &found["inspections"];
    assert_eq!(short["ids"], json!(["driver-1"]));
    assert_eq!(short["unknown_days"], json!([]));
    assert_eq!(short["inspections"][0]["date"], "2026-09-19");
    assert!(text.contains("1 short DVIC inspection"), "{text}");
    // Each other section is null where nothing at the DSP offers its feature's contract.
    let at = id.clone();
    let offered = (state.read(move |db| {
        Ok([
            ("scorecards", scorecard::for_dsp(db, &at)?.is_some()),
            ("daily", performance::for_dsp(db, &at)?.is_some()),
            ("routes", route::for_dsp(db, &at)?.is_some()),
            ("timecards", timecard::for_dsp(db, &at)?.is_some()),
            ("roster", profiles::for_dsp(db, &at)?.is_some()),
            ("chat", slack::for_dsp(db, &at)?.is_some()),
        ])
    }))
    .await
    .unwrap();
    for (section, offered) in offered {
        assert_eq!(found[section].is_null(), !offered, "{section}");
    }
    let code = found["person"]["code"].as_str().unwrap().to_owned();

    // By their code, the last 7 days, and only the sections asked for.
    let (week, _) = reply(call(json!({"driver": code, "sections": ["roster"]})).await);
    let today = dispatch_core::foundation::clock::today(chrono_tz::America::Los_Angeles);
    let first = today - Duration::days(6);
    assert_eq!(
        (&week["from"], &week["to"]),
        (&json!(first.to_string()), &json!(today.to_string()))
    );
    assert_eq!(week["inspections"], Value::Null);
}
