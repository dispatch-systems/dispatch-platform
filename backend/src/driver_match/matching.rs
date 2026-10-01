//! Who a new ID belongs to. Pure: it reads what is known and decides, and the store
//! writes the decisions. Only certain answers link an ID to someone; anything less gives
//! the ID a person of its own, for a person to review.
use super::names::{Name, name_key};
use super::sources::{Identity, Key};
use crate::contracts::{DriverLink, DriverSource};
use std::collections::{BTreeMap, BTreeSet};

/// An ID someone already holds, with the names its sources write today.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub source: DriverSource,
    pub id: String,
    pub names: Vec<String>,
}
/// Links saved on the meal-break page before Driver Match: an Amazon ID with the Paycom
/// code someone linked it to, and Amazon IDs someone kept apart from Paycom.
#[derive(Clone, Debug, Default)]
pub(crate) struct Saved {
    pub links: BTreeMap<String, String>,
    pub separate: BTreeSet<String>,
}
/// Where a new ID goes: to a person by code, or to a new person, `+0`, `+1` and on, that
/// the store then gives a code.
pub(crate) type Decision = (usize, String, DriverLink);

fn other(source: DriverSource) -> DriverSource {
    match source {
        DriverSource::Paycom => DriverSource::Amazon,
        DriverSource::Amazon => DriverSource::Paycom,
    }
}
fn keys(names: &[String]) -> BTreeSet<String> {
    names
        .iter()
        .map(|n| name_key(n))
        .filter(|k| !k.is_empty())
        .collect()
}
fn variant(a: &[String], b: &[String]) -> bool {
    a.iter()
        .any(|x| b.iter().any(|y| Name::new(x).matches(&Name::new(y))))
}

struct Plan<'a> {
    people: BTreeMap<String, Vec<Member>>,
    owner: BTreeMap<Key, String>,
    /// Every ID of either source, held or new: a name is unique only among all of them.
    all: Vec<Member>,
    saved: &'a Saved,
}
impl Plan<'_> {
    fn holds(&self, person: &str, source: DriverSource) -> bool {
        self.people[person].iter().any(|m| m.source == source)
    }
    /// The one person a saved link joins this ID to.
    fn saved(&self, x: &Identity) -> Option<String> {
        match x.source {
            DriverSource::Amazon => {
                let code = self.saved.links.get(&x.id)?;
                self.owner
                    .get(&(DriverSource::Paycom, code.clone()))
                    .cloned()
            }
            DriverSource::Paycom => self
                .saved
                .links
                .iter()
                .filter(|(_, code)| **code == x.id)
                .find_map(|(amazon, _)| self.owner.get(&(DriverSource::Amazon, amazon.clone())))
                .filter(|person| !self.holds(person, DriverSource::Paycom))
                .cloned(),
        }
    }
    /// The one person `x` belongs to by name, when nothing else could: `same` says whether
    /// two name lists are one name. The person must lack an ID of `x`'s source, no other
    /// person of the other source may carry the name, and no other ID of `x`'s source may
    /// carry the person's name.
    fn unique(&self, x: &Identity, same: impl Fn(&[String], &[String]) -> bool) -> Option<String> {
        let side = other(x.source);
        let carries = |m: &Member| m.source == side && same(&m.names, &x.names);
        // Someone kept apart from Paycom on the meal-break page takes no Paycom ID by name,
        // in this pass or any later one.
        let kept = |members: &[Member]| {
            x.source == DriverSource::Paycom
                && members.iter().any(|m| {
                    m.source == DriverSource::Amazon && self.saved.separate.contains(&m.id)
                })
        };
        let mut matched = self
            .people
            .iter()
            .filter(|(_, members)| !kept(members) && members.iter().any(carries))
            .map(|(person, _)| person.clone());
        let person = matched.next()?;
        if matched.next().is_some() || self.holds(&person, x.source) {
            return None;
        }
        let theirs: Vec<&Member> = self.people[&person]
            .iter()
            .filter(|m| m.source == side)
            .collect();
        let rival = self.all.iter().any(|m| {
            m.source == x.source && m.id != x.id && theirs.iter().any(|t| same(&m.names, &t.names))
        });
        (!rival).then_some(person)
    }
    fn decide(&self, x: &Identity) -> Option<(String, DriverLink)> {
        if x.source == DriverSource::Amazon && self.saved.separate.contains(&x.id) {
            return None;
        }
        if let Some(person) = self.saved(x) {
            return Some((person, DriverLink::Saved));
        }
        let exact = |a: &[String], b: &[String]| !keys(a).is_disjoint(&keys(b));
        // A name written alike somewhere decides before a variant could: when the exact
        // spelling is shared, a variant would only guess between the people sharing it.
        if self
            .people
            .values()
            .flatten()
            .any(|m| m.source == other(x.source) && exact(&m.names, &x.names))
        {
            return self.unique(x, exact).map(|p| (p, DriverLink::Name));
        }
        self.unique(x, variant).map(|p| (p, DriverLink::Variant))
    }
}

/// Decides where each new ID goes. Paycom's go first, as its roster lists who works for
/// the DSP; Amazon's then join them. `people` maps each code to the IDs it holds.
pub(crate) fn plan(
    people: BTreeMap<String, Vec<Member>>,
    incoming: &[Identity],
    saved: &Saved,
) -> Vec<Decision> {
    let mut owner = BTreeMap::new();
    for (person, members) in &people {
        for m in members {
            owner.insert((m.source, m.id.clone()), person.clone());
        }
    }
    let mut all: Vec<Member> = people.values().flatten().cloned().collect();
    all.extend(incoming.iter().map(|x| Member {
        source: x.source,
        id: x.id.clone(),
        names: x.names.clone(),
    }));
    let mut plan = Plan {
        people,
        owner,
        all,
        saved,
    };
    let mut order: Vec<usize> = (0..incoming.len()).collect();
    order.sort_by_key(|&i| {
        (
            incoming[i].source != DriverSource::Paycom,
            incoming[i].id.clone(),
        )
    });
    let mut out = Vec::new();
    for index in order {
        let x = &incoming[index];
        let (person, link) = plan.decide(x).unwrap_or_else(|| {
            let fresh = format!("+{}", out.len());
            plan.people.insert(fresh.clone(), vec![]);
            (fresh, DriverLink::New)
        });
        plan.people.get_mut(&person).unwrap().push(Member {
            source: x.source,
            id: x.id.clone(),
            names: x.names.clone(),
        });
        plan.owner.insert((x.source, x.id.clone()), person.clone());
        out.push((index, person, link));
    }
    out
}

#[cfg(test)]
mod tests {
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
    fn any_spelling_amazon_knows_can_match_and_a_middle_name_alone_does_not() {
        let result = run(
            &[
                paycom("A001", "OKAFOR, DANIEL"),
                // The scorecard adds a middle name; the routes name is plain.
                amazon("T1", &["Daniel Okafor", "Daniel Chukwuma Okafor"]),
                paycom("A002", "BELL, MARCUS"),
                amazon("T2", &["Marcus Andre Bell"]),
            ],
            &Saved::default(),
        );
        assert!(together(&result, "A001", "T1"));
        assert!(!together(&result, "A002", "T2"));
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
}
