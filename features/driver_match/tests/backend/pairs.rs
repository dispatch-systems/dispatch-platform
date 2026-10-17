use super::*;
use crate::api::types::EvidenceKind;

fn pair(person: &str, strength: Strength, reasons: usize) -> Pair {
    let reason = Evidence {
        kind: EvidenceKind::SameLastName,
        short: None,
        long: None,
        same: None,
        of: None,
        supports: true,
    };
    Pair {
        strength,
        person: person.to_owned(),
        candidate: format!("{person}-candidate"),
        evidence: vec![reason; reasons],
    }
}

#[test]
fn strong_pairs_come_first_then_more_reasons_then_names_and_three_at_most_for_anyone() {
    use Strength::{Possible, Strong};
    let names = BTreeMap::from([("BAYLEE", "Baylee Ortiz"), ("ADAM", "Adam Lee")]);
    let pairs = vec![
        pair("BAYLEE", Possible, 1),
        pair("ADAM", Possible, 1),
        pair("BAYLEE", Possible, 3),
        pair("BAYLEE", Strong, 1),
        pair("BAYLEE", Possible, 2),
    ];
    let asked: Vec<(&str, Strength, usize)> = ranked(pairs, |code| names[code])
        .iter()
        .map(|p| (names[p.person.as_str()], p.strength, p.evidence.len()))
        .collect::<Vec<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        asked,
        [
            ("Baylee Ortiz", Strong, 1),
            ("Baylee Ortiz", Possible, 3),
            ("Baylee Ortiz", Possible, 2),
            ("Adam Lee", Possible, 1),
        ]
    );
}
