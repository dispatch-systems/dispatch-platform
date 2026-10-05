use super::*;

fn id(source: DriverSource, id: &str, names: &[&str]) -> Identity {
    Identity {
        source,
        id: id.into(),
        names: names.iter().map(|n| (*n).to_owned()).collect(),
        department: None,
        position: None,
        first_seen: None,
        last_seen: None,
        listed: false,
    }
}
fn paycom(code: &str, name: &str) -> Identity {
    id(DriverSource::Paycom, code, &[name])
}
fn amazon(transporter: &str, names: &[&str]) -> Identity {
    id(DriverSource::Amazon, transporter, names)
}
/// Plans from nobody, and answers who each ID ends up with, by how.
fn run(incoming: &[Identity], saved: &Saved) -> BTreeMap<String, (String, DriverLink)> {
    plan(BTreeMap::new(), incoming, saved)
        .into_iter()
        .map(|(i, person, link)| (incoming[i].id.clone(), (person, link)))
        .collect()
}
fn together(result: &BTreeMap<String, (String, DriverLink)>, a: &str, b: &str) -> bool {
    result[a].0 == result[b].0
}

#[test]
fn unique_names_join_across_formats_and_variants_need_the_whole_surname() {
    let result = run(
        &[
            paycom("A001", "REYES, ANTONIO"),
            paycom("A002", "HERNANDEZ ORTIZ, LUIS"),
            paycom("A003", "WHITFIELD, JAMES"),
            amazon("T1", &["Antonio Reyes"]),
            amazon("T2", &["Luis Hernandez"]),
            amazon("T3", &["James Whitfield Jr"]),
        ],
        &Saved::default(),
    );
    assert!(together(&result, "A001", "T1"));
    assert_eq!(result["T1"].1, DriverLink::Name);
    assert!(together(&result, "A002", "T2"));
    assert_eq!(result["T2"].1, DriverLink::Variant);
    assert!(together(&result, "A003", "T3"));
    assert_eq!(result["A001"].1, DriverLink::New);
}
#[test]
fn any_spelling_amazon_knows_can_match_and_middle_names_are_looked_past() {
    let result = run(
        &[
            paycom("A001", "OKAFOR, DANIEL"),
            // The scorecard adds a middle name; the routes name is plain.
            amazon("T1", &["Daniel Okafor", "Daniel Chukwuma Okafor"]),
            paycom("A002", "BELL, MARCUS"),
            amazon("T2", &["Marcus Andre Bell"]),
            // Paycom lists the middle name they go by.
            paycom("A003", "MORGAN, RUBEN"),
            amazon("T3", &["Martin Morgan", "Martin Ruben Morgan"]),
        ],
        &Saved::default(),
    );
    assert!(together(&result, "A001", "T1"));
    assert!(together(&result, "A002", "T2"));
    assert_eq!(result["T2"].1, DriverLink::Variant);
    assert!(together(&result, "A003", "T3"));
    assert_eq!(result["T3"].1, DriverLink::Variant);
}
#[test]
fn a_middle_name_two_people_could_go_by_is_never_guessed() {
    let result = run(
        &[
            paycom("A001", "GARCIA, JOSE"),
            paycom("A002", "GARCIA, JUAN"),
            amazon("T1", &["Juan Jose Garcia"]),
        ],
        &Saved::default(),
    );
    assert_eq!(result["T1"].1, DriverLink::New);
}
#[test]
fn shared_names_and_different_first_names_are_never_guessed() {
    let result = run(
        &[
            paycom("A001", "SMITH, JOHN"),
            paycom("A002", "SMITH, JOHN"),
            amazon("T1", &["John Smith"]),
            paycom("A003", "LEE, MARIA"),
            amazon("T2", &["Maria Lee"]),
            amazon("T3", &["Maria Lee"]),
            paycom("A004", "REYES, ANTONIO"),
            amazon("T4", &["Tony Reyes"]),
        ],
        &Saved::default(),
    );
    for (a, b) in [
        ("A001", "T1"),
        ("A002", "T1"),
        ("A003", "T2"),
        ("A003", "T3"),
        ("A004", "T4"),
    ] {
        assert!(!together(&result, a, b), "{a} and {b}");
    }
    assert_eq!(result["T4"].1, DriverLink::New);
}
#[test]
fn a_person_who_has_an_amazon_id_takes_no_second_by_name() {
    let mut people = BTreeMap::new();
    people.insert(
        "K7M2QX".to_owned(),
        vec![
            Member {
                source: DriverSource::Paycom,
                id: "A001".into(),
                names: vec!["CARTER, DEVON".into()],
            },
            Member {
                source: DriverSource::Amazon,
                id: "T1".into(),
                names: vec!["Devon Carter".into()],
            },
        ],
    );
    let incoming = [amazon("T9", &["Devon Carter"])];
    let decided = plan(people, &incoming, &Saved::default());
    assert_eq!(decided[0].1, "+0");
    assert_eq!(decided[0].2, DriverLink::New);
}
#[test]
fn amazon_seen_first_is_joined_by_paycom_later() {
    let mut people = BTreeMap::new();
    people.insert(
        "W5HC2K".to_owned(),
        vec![Member {
            source: DriverSource::Amazon,
            id: "T1".into(),
            names: vec!["Priya Natarajan".into()],
        }],
    );
    let incoming = [paycom("A001", "NATARAJAN, PRIYA")];
    let decided = plan(people, &incoming, &Saved::default());
    assert_eq!(
        (decided[0].1.as_str(), decided[0].2),
        ("W5HC2K", DriverLink::Name)
    );
}
#[test]
fn a_driver_kept_apart_from_paycom_stays_apart_when_paycom_lists_them_later() {
    let saved = Saved {
        separate: BTreeSet::from(["T5".to_owned()]),
        ..Saved::default()
    };
    let mut people = BTreeMap::new();
    people.insert(
        "W5HC2K".to_owned(),
        vec![Member {
            source: DriverSource::Amazon,
            id: "T5".into(),
            names: vec!["Maria Lee".into()],
        }],
    );
    let incoming = [paycom("A005", "LEE, MARIA")];
    let decided = plan(people, &incoming, &saved);
    assert_eq!(
        (decided[0].1.as_str(), decided[0].2),
        ("+0", DriverLink::New)
    );
}
#[test]
fn links_saved_on_the_meal_break_page_decide_first() {
    let saved = Saved {
        links: BTreeMap::from([("T4".to_owned(), "A004".to_owned())]),
        separate: BTreeSet::from(["T5".to_owned()]),
    };
    let result = run(
        &[
            paycom("A004", "REYES, ANTONIO"),
            amazon("T4", &["Tony Reyes"]),
            paycom("A005", "LEE, MARIA"),
            amazon("T5", &["Maria Lee"]),
        ],
        &saved,
    );
    assert!(together(&result, "A004", "T4"));
    assert_eq!(result["T4"].1, DriverLink::Saved);
    assert!(!together(&result, "A005", "T5"));
}
