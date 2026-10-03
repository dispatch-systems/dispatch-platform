//! Who might be one person listed twice: someone only Paycom knows beside someone only
//! Amazon knows, whose names come close without matching. A person decides; this only
//! gathers the reasons.
use crate::contracts::{DriverEvidence, DriverEvidenceKind, DriverStrength};
use crate::names::{Name, capitalized, short_form};
use std::collections::BTreeSet;

/// One side of a possible pair: a person's code and every name its IDs carry.
pub(crate) struct Side<'a> {
    pub code: &'a str,
    pub names: Vec<&'a str>,
}
pub(crate) struct Suggestion {
    pub paycom: String,
    pub amazon: String,
    pub evidence: Vec<DriverEvidence>,
}

fn evidence(kind: DriverEvidenceKind) -> DriverEvidence {
    DriverEvidence {
        kind,
        short: None,
        long: None,
        same: None,
        of: None,
    }
}

/// The reasons two names might be one person's, or none when nothing ties them: a shared
/// last name, or a first name and a last initial.
fn reasons(a: &Name, b: &Name) -> Vec<DriverEvidence> {
    let (Some(last_a), Some(last_b)) = (a.last(), b.last()) else {
        return vec![];
    };
    let same_last = last_a == last_b && last_a.chars().count() > 1;
    // Amazon sometimes writes only a last initial, as "Jordan K."
    let initial = !same_last
        && (last_a.chars().count() == 1 || last_b.chars().count() == 1)
        && last_a.chars().next() == last_b.chars().next();
    let first = if a.given == b.given && !a.given.is_empty() {
        Some(evidence(DriverEvidenceKind::SameFirstName))
    } else {
        short_form(&a.given, &b.given).map(|(short, long)| DriverEvidence {
            short: Some(capitalized(short)),
            long: Some(capitalized(long)),
            ..evidence(DriverEvidenceKind::ShortForm)
        })
    };
    let tied = same_last || (initial && first.is_some());
    if !tied {
        return vec![];
    }
    let mut out = vec![evidence(if same_last {
        DriverEvidenceKind::SameLastName
    } else {
        DriverEvidenceKind::LastInitial
    })];
    out.extend(first);
    out
}

/// Every pair worth a look, the strongest reasons each pair's names give. `apart` holds
/// the pairs someone already decided are different people, as (first, second) codes.
pub(crate) fn suggest(
    paycom: &[Side<'_>],
    amazon: &[Side<'_>],
    apart: &BTreeSet<(String, String)>,
) -> Vec<Suggestion> {
    let mut out = vec![];
    for a in amazon {
        let names_a: Vec<Name> = a.names.iter().map(|n| Name::new(n)).collect();
        for p in paycom {
            let key = if p.code < a.code {
                (p.code.to_owned(), a.code.to_owned())
            } else {
                (a.code.to_owned(), p.code.to_owned())
            };
            if apart.contains(&key) {
                continue;
            }
            let best = p
                .names
                .iter()
                .flat_map(|n| {
                    let name = Name::new(n);
                    names_a
                        .iter()
                        .map(|x| reasons(&name, x))
                        .collect::<Vec<_>>()
                })
                .max_by_key(|r| {
                    (
                        r.len(),
                        r.iter().any(|e| e.kind == DriverEvidenceKind::SameLastName),
                    )
                })
                .unwrap_or_default();
            if !best.is_empty() {
                out.push(Suggestion {
                    paycom: p.code.to_owned(),
                    amazon: a.code.to_owned(),
                    evidence: best,
                });
            }
        }
    }
    out
}

/// The days both sides worked out of the days the Amazon side drove, where both sources
/// were collected. `None` with fewer than two such days, which say nothing.
pub(crate) fn worked_days(
    clocked: Option<&BTreeSet<String>>,
    drove: Option<&BTreeSet<String>>,
    paycom_days: &BTreeSet<String>,
    amazon_days: &BTreeSet<String>,
) -> Option<DriverEvidence> {
    let drove = drove?;
    let window: BTreeSet<&String> = paycom_days.intersection(amazon_days).collect();
    let days: Vec<&String> = drove.iter().filter(|d| window.contains(d)).collect();
    if days.len() < 2 {
        return None;
    }
    let same = days
        .iter()
        .filter(|d| clocked.is_some_and(|c| c.contains(**d)))
        .count();
    Some(DriverEvidence {
        same: Some(same),
        of: Some(days.len()),
        ..evidence(DriverEvidenceKind::WorkedDays)
    })
}

/// Strong when the last names agree and either the first name is a short form of the
/// other or the two worked every one of at least three days together.
pub(crate) fn strength(evidence: &[DriverEvidence]) -> DriverStrength {
    let has = |kind| evidence.iter().any(|e| e.kind == kind);
    let all_days = evidence.iter().any(|e| {
        e.kind == DriverEvidenceKind::WorkedDays && e.of.unwrap_or(0) >= 3 && e.same == e.of
    });
    let short = has(DriverEvidenceKind::ShortForm);
    let partly = evidence.iter().any(|e| {
        e.kind == DriverEvidenceKind::WorkedDays && e.same.unwrap_or(0) * 2 < e.of.unwrap_or(0)
    });
    if has(DriverEvidenceKind::SameLastName) && (all_days || short) && !partly {
        DriverStrength::Strong
    } else {
        DriverStrength::Possible
    }
}

#[cfg(test)]
mod tests {
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
}
