use super::*;

fn kinds(evidence: &[DriverEvidence]) -> Vec<DriverEvidenceKind> {
    evidence.iter().map(|e| e.kind).collect()
}
fn pairs(
    paycom: &[(&'static str, &'static str)],
    amazon: &[(&'static str, &'static str)],
) -> Vec<Suggestion> {
    let side = |list: &[(&'static str, &'static str)]| {
        list.iter()
            .map(|&(code, name)| Side {
                code,
                names: vec![name],
            })
            .collect::<Vec<_>>()
    };
    suggest(&side(paycom), &side(amazon), &BTreeSet::new())
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
    let between = |p: &str, a: &str| found.iter().find(|s| s.paycom == p && s.amazon == a);
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
    let paycom = [Side {
        code: "P1",
        names: vec!["REYES, ANTONIO"],
    }];
    let amazon = [Side {
        code: "A1",
        names: vec!["Tony Reyes"],
    }];
    let apart = BTreeSet::from([("A1".to_owned(), "P1".to_owned())]);
    assert!(suggest(&paycom, &amazon, &apart).is_empty());
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
