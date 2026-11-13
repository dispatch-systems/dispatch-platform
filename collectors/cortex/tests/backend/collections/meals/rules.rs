//! The meal rules read a day's list and an itinerary's response as `meal_rules.js` and the hook
//! read them: the cases here are those of `tests/native/cortex-meal-script.test.ts`, which
//! holds the scripts to the same answers.
use super::*;
use serde_json::json;

const START: i64 = 1_789_498_800_000; // 2026-09-15T19:00:00Z

fn scope() -> Scope {
    Scope {
        date: "2026-09-15".into(),
        station: "TST1".into(),
        service_area_id: "area-1".into(),
        provider: "provider-1".into(),
        timezone: "America/Los_Angeles".into(),
    }
}
fn on() -> Value {
    json!({"punchId":"punch-1","breakId":"meal-1","type":"MEAL","state":"ON",
        "timeStampOn":START,"timeStampOff":null,"sequenceNumber":1})
}
fn off() -> Value {
    with(
        on(),
        json!({"punchId":"punch-2","state":"OFF","timeStampOff":START + 1_800_000}),
    )
}
/// `base` with `fields` written over it.
fn with(mut base: Value, fields: Value) -> Value {
    for (key, value) in fields.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}
/// The day's list of one route whose punches are `breaks`.
fn list(breaks: Value) -> Value {
    json!({
        "itinerarySummaries":[{"itineraryId":"itinerary-1","transporterId":"driver-1","routeCode":"CX1",
            "companyId":"provider-1","executionStatus":"IN_PROGRESS","breaks":breaks}],
        "transporters":[{"transporterId":"driver-1","firstName":"Fixture","lastName":"Driver"}],
    })
}
fn route(breaks: Value) -> Candidate {
    candidates(&list(breaks), &scope()).unwrap().remove(0)
}
fn punches(candidate: &Candidate) -> Vec<(&str, i64, Option<i64>)> {
    (candidate.meals.iter())
        .map(|meal| (meal.id.as_str(), meal.start, meal.end))
        .collect()
}

#[test]
fn a_route_whose_punches_break_a_rule_keeps_its_route_its_meals_unknown() {
    let cases = [
        ("meal_break_id", json!([with(on(), json!({"breakId":""}))])),
        (
            "meal_punch_id",
            json!([with(on(), json!({"punchId":null}))]),
        ),
        ("meal_start", json!([with(on(), json!({"timeStampOn":0}))])),
        ("meal_state", json!([with(on(), json!({"state":"PAUSED"}))])),
        (
            "meal_off_without_end",
            json!([with(off(), json!({"timeStampOff":null}))]),
        ),
        (
            "meal_end_before_start",
            json!([with(off(), json!({"timeStampOff":START - 1000}))]),
        ),
        (
            "meal_on_with_end",
            json!([with(on(), json!({"timeStampOff":START + 1000}))]),
        ),
        (
            "meal_repeat_sequence",
            json!([on(), with(off(), json!({"sequenceNumber":2}))]),
        ),
        (
            "meal_repeat_conflict",
            json!([off(), with(off(), json!({"timeStampOff":START + 900_000}))]),
        ),
        (
            "meal_repeat_outside",
            json!([with(on(), json!({"timeStampOn":START + 3_600_000})), off()]),
        ),
    ];
    for (rule, breaks) in cases {
        let route = route(breaks);
        assert!(route.meals.is_empty(), "{rule}");
        assert_eq!(route.unreadable.as_deref(), Some(rule));
    }
}

#[test]
fn a_meal_start_pressed_twice_is_one_meal_that_began_at_the_first_press() {
    let again = |seconds: i64| {
        with(
            on(),
            json!({"punchId":"punch-2","timeStampOn":START + seconds * 1000}),
        )
    };
    // While the meal runs, Cortex holds both starts open.
    for breaks in [json!([on(), again(4)]), json!([again(64), on()])] {
        let route = route(breaks);
        assert_eq!(punches(&route), [("meal-1", START, None)]);
        assert_eq!(route.unreadable, None);
    }
    // Once it ends, the first start is the completed meal and the second lies inside it.
    let ended = with(
        on(),
        json!({"state":"OFF","timeStampOff":START + 1_800_000}),
    );
    assert_eq!(
        punches(&route(json!([ended, again(4)]))),
        [("meal-1", START, Some(START + 1_800_000))]
    );
    // Starts further apart are not one press: which began the meal is unknown.
    assert_eq!(
        route(json!([on(), again(301)])).unreadable.as_deref(),
        Some("meal_repeat_conflict")
    );
}

#[test]
fn the_list_is_read_for_its_scope_alone() {
    // A meal swipe is published evidence, so it versions the route; a time in seconds reads
    // as one in milliseconds.
    let before = route(json!([on()]));
    assert_ne!(route(json!([on(), off()])).revision, before.revision);
    let seconds = route(json!([with(on(), json!({"timeStampOn":START / 1000}))]));
    assert_eq!(punches(&seconds), punches(&before));
    // Another company's routes are not the scope's, and a driver the list does not name is
    // no one's.
    let mut other = list(json!([on()]));
    other["itinerarySummaries"][0]["companyId"] = json!("provider-2");
    assert!(candidates(&other, &scope()).unwrap().is_empty());
    let mut unnamed = list(json!([on()]));
    unnamed["transporters"] = json!([]);
    assert_eq!(
        candidates(&unnamed, &scope()).err().map(|fault| fault.code),
        Some("cortex_invalid_identity")
    );
    assert_eq!(
        candidates(&json!({"itinerarySummaries":{}}), &scope())
            .err()
            .map(|fault| fault.code),
        Some("cortex_content_incomplete")
    );
}

/// A delivery made at `time`.
fn task(id: &str, time: i64) -> Value {
    json!({"taskId":id,"taskType":"DROP_OFF","taskState":"DELIVERED","executionStatus":"COMPLETE",
        "actualExecutionTime":time as f64 / 1000.0,"transporterId":null})
}
/// A finished route whose meal both deliveries bound, as its itinerary's response sends it,
/// and the route as the list names it.
fn finished() -> (Candidate, Value) {
    let ended = with(
        on(),
        json!({"state":"OFF","timeStampOff":START + 1_800_000}),
    );
    let mut day = list(json!([ended.clone()]));
    day["itinerarySummaries"][0]["executionStatus"] = json!("COMPLETE");
    let candidate = candidates(&day, &scope()).unwrap().remove(0);
    let response = json!({"itineraryDetails":{
        "itineraryId":"itinerary-1","transporterId":"driver-1","executionStatus":"COMPLETE",
        "breaks":[ended],"localDate":[2026,9,15],"serviceAreaId":"area-1",
        "stopProgress":{"total":2,"completed":2},
        "stops":[
            {"tasks":[task("task-1", START - 60000)]},
            {"tasks":[task("task-1", START - 60000), task("task-2", START + 1_860_000)]},
        ],
        "unknownStops":[],
        "inactiveTasks":[task("removed", START - 7_200_000)],
    }});
    (candidate, response)
}
fn read(candidate: &Candidate, response: &Value) -> std::result::Result<Itinerary, Fault> {
    detail(&response.to_string(), candidate, &scope())
}
fn bounds(itinerary: &Itinerary) -> Vec<[Option<i64>; 4]> {
    (itinerary.meals.iter())
        .map(|meal| {
            [
                meal.last_delivery,
                meal.first_delivery,
                meal.last_delivery_stop.map(i64::from),
                meal.first_delivery_stop.map(i64::from),
            ]
        })
        .collect()
}

#[test]
fn an_itinerarys_response_bounds_each_meal_with_the_deliveries_around_it() {
    let (candidate, response) = finished();
    let itinerary = read(&candidate, &response).unwrap();
    assert_eq!(itinerary.delivery_coverage, Coverage::Complete);
    assert!(itinerary.route_complete);
    // Each delivery's stop by its place in the route. A delivery repeated in an overlapping
    // group stop keeps the first; a removed task hours away changes nothing.
    assert_eq!(
        bounds(&itinerary),
        [[
            Some(START - 60000),
            Some(START + 1_860_000),
            Some(0),
            Some(1)
        ]]
    );
    // Conflicting copies of one task cannot bound a meal, so coverage is lost, as it is when
    // a removed task may lie closer than the delivery chosen, another driver delivered, or
    // the route does not hold every stop it counts.
    let mut cases = Vec::new();
    let mut conflicting = response.clone();
    conflicting["itineraryDetails"]["stops"][1]["tasks"][0]["actualExecutionTime"] =
        json!((START - 61000) as f64 / 1000.0);
    cases.push(conflicting);
    let mut removed = response.clone();
    removed["itineraryDetails"]["inactiveTasks"] = json!([task("removed", START - 30000)]);
    cases.push(removed);
    let mut another = response.clone();
    another["itineraryDetails"]["stops"][0]["tasks"][0]["transporterId"] = json!("driver-2");
    another["itineraryDetails"]["stops"][1]["tasks"][0]["transporterId"] = json!("driver-2");
    cases.push(another);
    let mut short = response.clone();
    short["itineraryDetails"]["stopProgress"]["total"] = json!(3);
    cases.push(short);
    for (index, response) in cases.iter().enumerate() {
        let itinerary = read(&candidate, response).unwrap();
        assert_eq!(
            itinerary.delivery_coverage,
            Coverage::Unavailable,
            "{index}"
        );
        assert_eq!(bounds(&itinerary), [[None; 4]], "{index}");
    }
}

#[test]
fn an_itinerarys_response_that_is_not_the_route_listed_says_why() {
    let (candidate, response) = finished();
    let changed = |field: &str, value: Value| {
        let mut response = response.clone();
        *response
            .pointer_mut(&format!("/itineraryDetails/{field}"))
            .unwrap() = value;
        read(&candidate, &response).map_err(|fault| (fault.code, fault.reason))
    };
    let said = |code: &'static str, reason: &str| Some((code, reason.to_owned()));
    assert_eq!(
        changed("itineraryId", json!("itinerary-2")).err(),
        said("cortex_scope_mismatch", "detail_itinerary")
    );
    // Another driver's is a route that changed hands after the list was read, as is a meal
    // that ended since.
    assert_eq!(
        changed("transporterId", json!("driver-2")).err(),
        said("cortex_source_changed", "route_changed")
    );
    assert_eq!(
        changed("breaks", json!([on()])).err(),
        said("cortex_source_changed", "meal_changed")
    );
    assert_eq!(
        changed("executionStatus", json!("IN_PROGRESS")).err(),
        said("cortex_source_changed", "meal_changed")
    );
    assert_eq!(
        changed("localDate", json!([2026, 9, 18])).err(),
        said("cortex_scope_mismatch", "detail_date")
    );
    assert_eq!(
        changed("serviceAreaId", json!("area-2")).err(),
        said("cortex_scope_mismatch", "detail_area")
    );
    assert_eq!(
        changed("stops", json!({})).err(),
        said("cortex_content_incomplete", "stops")
    );
    assert_eq!(
        changed("stops/0/tasks", json!(null)).err(),
        said("cortex_content_incomplete", "stops")
    );
    // Punches the rules can't read leave the route's meals unknown, whatever the list said.
    let unknown = changed("breaks", json!([with(off(), json!({"timeStampOn":0}))])).unwrap();
    assert_eq!(unknown.unreadable.as_deref(), Some("meal_start"));
    assert!(unknown.meals.is_empty());
    assert_eq!(unknown.delivery_coverage, Coverage::Unavailable);
}
