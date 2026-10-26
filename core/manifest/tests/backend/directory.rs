use super::*;
use crate::manifest::people::SourceInfo;

const PAYROLL: Source = Source::new(&SourceInfo {
    id: "payroll",
    label: "Payroll",
    order: 10,
});
const ROUTES: Source = Source::new(&SourceInfo {
    id: "routes",
    label: "Routes",
    order: 20,
});

fn person(code: &str, names: &[&str], ids: &[(Source, &str)], email: &str, phone: &str) -> Person {
    let mut held: BTreeMap<Source, Vec<String>> = BTreeMap::new();
    for (source, id) in ids {
        held.entry(*source).or_default().push((*id).to_owned());
    }
    Person {
        code: code.into(),
        name: names[0].into(),
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        role: PersonRole::Driver,
        title: None,
        ids: held,
        emails: vec![email.into()],
        phones: vec![phone.into()],
        confirmed: false,
        photo: None,
    }
}
fn codes(people: Vec<&Person>) -> Vec<&str> {
    people.iter().map(|person| person.code.as_str()).collect()
}

#[test]
fn a_person_is_found_by_their_code_any_id_contact_or_name() {
    let everyone = Everyone::new(
        vec![
            person(
                "AVERYM",
                &["Avery Morgan"],
                &[(PAYROLL, "E002"), (ROUTES, "A1B2C3")],
                "avery@example.com",
                "(555) 010-2030",
            ),
            person(
                "CAMHAY",
                &["Cameron Hayes", "Cam Hayes"],
                &[(PAYROLL, "E001")],
                "cam@example.com",
                "555-010-9999",
            ),
            person(
                "AVERYZ",
                &["Avery Morganson"],
                &[],
                "averyz@example.com",
                "+1 (555) 010-9999",
            ),
        ],
        // A code merged into one since merged goes on to whoever kept their IDs.
        BTreeMap::from([
            ("OLDONE".to_owned(), "OLDTWO".to_owned()),
            ("OLDTWO".to_owned(), "CAMHAY".to_owned()),
        ]),
    );
    assert_eq!(
        codes(everyone.iter().collect()),
        ["AVERYM", "AVERYZ", "CAMHAY"],
        "by name"
    );
    assert_eq!(everyone.person("OLDONE").unwrap().code, "CAMHAY");
    assert!(everyone.person("NOBODY").is_none());
    assert_eq!(everyone.holding(ROUTES, "A1B2C3").unwrap().code, "AVERYM");
    assert!(everyone.holding(PAYROLL, "A1B2C3").is_none());

    assert_eq!(codes(everyone.find("a1b2c3")), ["AVERYM"]);
    assert_eq!(codes(everyone.find("CAM@example.com")), ["CAMHAY"]);
    assert_eq!(codes(everyone.find("+1 555 010 2030")), ["AVERYM"]);
    // A number however it's written: everyone who has it, by name.
    assert_eq!(codes(everyone.by_phone("5550109999")), ["AVERYZ", "CAMHAY"]);
    // Any way their sources write their name, in part.
    assert_eq!(codes(everyone.find("cam h")), ["CAMHAY"]);
    assert_eq!(codes(everyone.find("Avery Morg")), ["AVERYM", "AVERYZ"]);
    // A name exactly as someone's sources write it is them alone.
    assert_eq!(codes(everyone.find("avery morgan")), ["AVERYM"]);
    assert!(everyone.find("  ").is_empty());
    assert!(everyone.find("nobody@example.com").is_empty());
}
