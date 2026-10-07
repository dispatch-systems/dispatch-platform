use super::*;

#[test]
fn manual_links_reserve_targets_and_unrecognized_names_are_not_guessed() {
    let mut drivers: BTreeMap<_, _> = [
        ("one".to_owned(), json!({"name":"Jamie Reed"})),
        ("two".to_owned(), json!({"name":"Different Name"})),
        ("three".to_owned(), json!({"name":"Al Jones"})),
        ("empty".to_owned(), json!({"name":"---"})),
    ]
    .into();
    let roster = vec![
        json!({"code":"E1","name":"REED, JAMIE"}),
        json!({"code":"E2","name":"JONES, ALEXANDER"}),
        json!({"code":"E3","name":""}),
    ];
    match_drivers(
        &mut drivers,
        &roster,
        &json!({"links":[{"cortexId":"two","paycomCode":"E1"}]}),
    );
    assert_eq!(drivers["one"]["matchType"], "unmatched");
    assert_eq!(drivers["two"]["matchType"], "saved");
    assert_eq!(drivers["three"]["matchType"], "unmatched");
    assert_eq!(drivers["empty"]["matchType"], "unmatched");
    match_drivers(
        &mut drivers,
        &roster,
        &json!({"links":[],"separate":["one"]}),
    );
    assert_eq!(drivers["one"]["matchType"], "separate");
    match_drivers(&mut drivers, &roster, &json!({"links":[]}));
    assert_eq!(drivers["one"]["paycomCode"], "E1");
}

fn matches(names: &[&str], employees: &[&str], settings: Value) -> Vec<Value> {
    let mut drivers = names
        .iter()
        .enumerate()
        .map(|(i, name)| (format!("D{i}"), json!({"name":name})))
        .collect();
    let roster = employees
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"code":format!("E{i}"),"name":name}))
        .collect::<Vec<_>>();
    match_drivers(&mut drivers, &roster, &settings);
    drivers.into_values().collect()
}

#[test]
fn unique_name_variants_match_in_both_directions() {
    for (left, right) in [
        ("Jamie Reed Vega", "REED, JAMIE"),
        ("Casey Molina Solis", "MOLINA, CASEY"),
        ("Alexander Stone", "STONE, ALEX"),
        ("Taylor Hart Jr.", "HART, TAYLOR"),
        ("Morgan Hill III", "HILL, MORGAN"),
        ("René O’Neill Cruz", "O'NEILL, RENÉ"),
        ("Taylor MolinaReed Vega", "MOLINA REED, TAYLOR"),
    ] {
        for (driver, employee) in [(left, right), (right, left)] {
            let result = matches(&[driver], &[employee], json!({}));
            assert_eq!(result[0]["paycomCode"], "E0", "{driver} / {employee}");
            assert_eq!(result[0]["matchType"], "name");
        }
    }
}

#[test]
fn variants_require_complete_name_components_and_compatible_suffixes() {
    for (driver, employee) in [
        ("Alex Stone", "Alexis Stone"),
        ("Alex Stone", "Alexandra Stone"),
        ("Jamie Reed", "Jamie Reeder"),
        ("Jamie Reed Vega", "Jamie Reed Cruz"),
        ("Jamie Reed Jr", "Jamie Reed Sr"),
        ("Jamie Reed II", "Jamie Reed III"),
        ("Jamie Reed", "J Reed"),
        ("Jamie", "Jamie Reed"),
        ("Reed", "Reed Jamie"),
    ] {
        assert_eq!(
            matches(&[driver], &[employee], json!({}))[0]["paycomCode"],
            Value::Null,
            "{driver} / {employee}"
        );
    }
}

#[test]
fn variants_must_be_unique_across_both_complete_rosters() {
    let drivers = ["Jamie Reed Vega", "Jamie Reed Cruz"];
    let employees = ["REED, JAMIE"];
    for settings in [
        json!({}),
        json!({"separate":["D1"]}),
        json!({"links":[{"cortexId":"D1","paycomCode":"E1"}]}),
    ] {
        let result = matches(&drivers, &employees, settings);
        assert_eq!(result[0]["matchType"], "unmatched");
    }
    let result = matches(
        &["Jamie Reed"],
        &["REED VEGA, JAMIE", "REED CRUZ, JAMIE"],
        json!({}),
    );
    assert_eq!(result[0]["matchType"], "unmatched");
    let result = matches(
        &["Alexander Stone"],
        &["STONE, ALEX", "STONE, ALEX"],
        json!({}),
    );
    assert_eq!(result[0]["matchType"], "unmatched");
}

#[test]
fn exact_matches_and_saved_choices_take_priority_over_variants() {
    let result = matches(
        &["Jamie Reed", "Jamie Reed Vega"],
        &["REED, JAMIE"],
        json!({}),
    );
    assert_eq!(result[0]["paycomCode"], "E0");
    assert_eq!(result[1]["matchType"], "unmatched");
    let result = matches(
        &["Jamie Reed Vega"],
        &["REED, JAMIE", "REED VEGA, JAMIE"],
        json!({}),
    );
    assert_eq!(result[0]["paycomCode"], "E1");
    let drivers = ["Jamie Reed Vega", "Different Person"];
    let employees = ["REED, JAMIE"];
    let result = matches(
        &drivers,
        &employees,
        json!({"links":[{"cortexId":"D1","paycomCode":"E0"}]}),
    );
    assert_eq!(result[0]["matchType"], "unmatched");
    assert_eq!(result[1]["matchType"], "saved");
    assert_eq!(
        matches(&drivers, &employees, json!({"separate":["D0"]}))[0]["matchType"],
        "separate"
    );
    assert_eq!(
        matches(&drivers, &employees, json!({}))[0]["paycomCode"],
        "E0"
    );
}
