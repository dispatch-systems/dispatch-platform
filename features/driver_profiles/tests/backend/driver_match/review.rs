use super::*;
use dispatch_core::manifest::people::SourceData;

static PAYCOM_IDS: SourceData = SourceData {
    id: "paycom",
    label: "Paycom",
    order: 10,
};
static AMAZON_IDS: SourceData = SourceData {
    id: "amazon",
    label: "Amazon",
    order: 20,
};

fn kinds(evidence: &[DriverEvidence]) -> Vec<DriverEvidenceKind> {
    evidence.iter().map(|e| e.kind).collect()
}
/// Someone one source knows by a name.
fn side(source: &'static SourceData, code: &'static str, name: &'static str) -> Side<'static> {
    Side {
        code,
        sources: BTreeSet::from([DriverSource::new(source)]),
        names: vec![name],
        emails: BTreeSet::new(),
        phones: BTreeSet::new(),
    }
}
fn pairs(
    paycom: &[(&'static str, &'static str)],
    amazon: &[(&'static str, &'static str)],
) -> Vec<Suggestion> {
    let people: Vec<Side> = paycom
        .iter()
        .map(|&(code, name)| side(&PAYCOM_IDS, code, name))
        .chain(
            amazon
                .iter()
                .map(|&(code, name)| side(&AMAZON_IDS, code, name)),
        )
        .collect();
    suggest(&people, &BTreeSet::new())
}

#[test]
fn preferred_names_and_last_initials_are_suggested_and_strangers_are_not() {
    let found = pairs(
        &[
            ("P1", "REYES, ANTONIO"),
            ("P2", "KOWALSKI, JORDAN"),
            ("P3", "BELL, MARCUS"),
        ],
        &[
            ("A1", "Tony Reyes"),
            ("A2", "Jordan K."),
            ("A3", "Devon Carter"),
        ],
    );
    let between = |p: &str, a: &str| found.iter().find(|s| s.first == p && s.second == a);
    let reyes = between("P1", "A1").unwrap();
    assert_eq!(
        kinds(&reyes.evidence),
        [
            DriverEvidenceKind::SameLastName,
            DriverEvidenceKind::ShortForm
        ]
    );
    assert_eq!(reyes.evidence[1].short.as_deref(), Some("Tony"));
    assert_eq!(
        kinds(&between("P2", "A2").unwrap().evidence),
        [
            DriverEvidenceKind::LastInitial,
            DriverEvidenceKind::SameFirstName
        ]
    );
    assert!(between("P3", "A3").is_none());
    assert_eq!(found.len(), 2);
}
#[test]
fn pairs_kept_apart_are_not_suggested_again() {
    let people = [
        side(&PAYCOM_IDS, "P1", "REYES, ANTONIO"),
        side(&AMAZON_IDS, "A1", "Tony Reyes"),
    ];
    let apart = BTreeSet::from([("A1".to_owned(), "P1".to_owned())]);
    assert!(suggest(&people, &apart).is_empty());
}
#[test]
fn a_contact_only_two_share_pairs_them_even_within_one_source() {
    let reached = |mut side: Side<'static>, phone: &'static str| {
        side.phones.insert(phone);
        side
    };
    let people = [
        // A rehire, under a second transporter ID.
        reached(side(&AMAZON_IDS, "A1", "Sam Rivera"), "6125550123"),
        reached(side(&AMAZON_IDS, "A2", "Samuel Rivera"), "6125550123"),
        // A phone three people carry joins none of them.
        reached(side(&AMAZON_IDS, "A3", "Lee Park"), "6125550100"),
        reached(side(&AMAZON_IDS, "A4", "Kim Park"), "6125550100"),
        reached(side(&PAYCOM_IDS, "P1", "PARK, JO"), "6125550100"),
    ];
    let found = suggest(&people, &BTreeSet::new());
    let rehire = found
        .iter()
        .find(|s| s.first == "A1" && s.second == "A2")
        .unwrap();
    assert_eq!(
        kinds(&rehire.evidence),
        [
            DriverEvidenceKind::SamePhone,
            DriverEvidenceKind::SameLastName,
            DriverEvidenceKind::ShortForm
        ]
    );
    assert_eq!(strength(&rehire.evidence), DriverStrength::Strong);
    // The Parks are tied by their name across sources, never by the phone.
    assert!(
        found.iter().all(
            |s| !kinds(&s.evidence).contains(&DriverEvidenceKind::SamePhone) || s.first == "A1"
        )
    );
    assert!(!found.iter().any(|s| s.first == "A3" && s.second == "A4"));
}
#[test]
fn days_worked_count_only_where_both_sources_were_collected() {
    let set = |days: &[&str]| {
        days.iter()
            .map(|d| (*d).to_owned())
            .collect::<BTreeSet<_>>()
    };
    let clocked = set(&["2026-09-01", "2026-09-02", "2026-09-04"]);
    let drove = set(&[
        "2026-09-01",
        "2026-09-02",
        "2026-09-03",
        "2026-09-04",
        "2026-09-09",
    ]);
    let paycom = set(&["2026-09-01", "2026-09-02", "2026-09-03", "2026-09-04"]);
    let amazon = set(&[
        "2026-09-01",
        "2026-09-02",
        "2026-09-03",
        "2026-09-04",
        "2026-09-09",
    ]);
    let days = worked_days(Some(&clocked), Some(&drove), &paycom, &amazon).unwrap();
    assert_eq!((days.same, days.of), (Some(3), Some(4)));
    assert!(worked_days(Some(&clocked), None, &paycom, &amazon).is_none());
}
#[test]
fn strong_needs_the_last_name_and_a_short_form_or_every_day() {
    let e = |kind| evidence(kind);
    let days = |same, of| DriverEvidence {
        same: Some(same),
        of: Some(of),
        ..e(DriverEvidenceKind::WorkedDays)
    };
    let last = e(DriverEvidenceKind::SameLastName);
    let short = e(DriverEvidenceKind::ShortForm);
    assert_eq!(
        strength(&[last.clone(), short.clone()]),
        DriverStrength::Strong
    );
    assert_eq!(
        strength(&[last.clone(), days(5, 5)]),
        DriverStrength::Strong
    );
    assert_eq!(
        strength(&[last.clone(), days(2, 2)]),
        DriverStrength::Possible
    );
    assert_eq!(
        strength(&[last.clone(), short, days(1, 5)]),
        DriverStrength::Possible
    );
    assert_eq!(
        strength(&[e(DriverEvidenceKind::LastInitial), days(4, 4)]),
        DriverStrength::Possible
    );
}
