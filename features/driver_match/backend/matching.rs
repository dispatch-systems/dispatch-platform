//! Who a new ID belongs to. Pure: it reads what is known and decides, and the store writes
//! the decisions. Only certain answers link an ID to someone: an email address or phone
//! number only the two of them have, with names that agree, or a name nobody else carries.
//! Anything less gives the ID a person of its own, for a person to review.
use super::legacy::{AMAZON, PAYCOM, Saved};
use super::names::{Name, name_key};
use super::sources::{Identity, Key};
use crate::api::types::DriverLink;
use dispatch_core::manifest::people::DriverSource;
use std::collections::{BTreeMap, BTreeSet};

/// An ID someone already holds, with what its sources say of it today.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub source: DriverSource,
    pub id: String,
    pub names: Vec<String>,
    pub emails: BTreeSet<String>,
    pub phones: BTreeSet<String>,
}
impl Member {
    pub fn of(x: &Identity) -> Self {
        Self {
            source: x.source,
            id: x.id.clone(),
            names: x.names.clone(),
            emails: x.emails.clone(),
            phones: x.phones.clone(),
        }
    }
}
/// Where a new ID goes: to a person by code, or to a new person, `+0`, `+1` and on, that
/// the store then gives a code.
pub(crate) type Decision = (usize, String, DriverLink);

fn keys(names: &[String]) -> BTreeSet<String> {
    names
        .iter()
        .map(|n| name_key(n))
        .filter(|k| !k.is_empty())
        .collect()
}
fn exact(a: &[String], b: &[String]) -> bool {
    !keys(a).is_disjoint(&keys(b))
}
fn variant(a: &[String], b: &[String]) -> bool {
    a.iter()
        .any(|x| b.iter().any(|y| Name::new(x).matches(&Name::new(y))))
}
/// Whether two name lists could be one person's: one name, a variant of it, or a shared
/// first or last name. A contact two people share joins them only when their names agree.
pub(crate) fn agree(a: &[String], b: &[String]) -> bool {
    exact(a, b)
        || variant(a, b)
        || a.iter().any(|x| {
            let x = Name::new(x);
            b.iter().any(|y| {
                let y = Name::new(y);
                (!x.given.is_empty() && x.given == y.given)
                    || x.last()
                        .is_some_and(|last| last.chars().count() > 1 && y.last() == Some(last))
            })
        })
}

/// Which kind of contact a link came by.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Contact {
    Email,
    Phone,
}
impl Contact {
    const ALL: [Self; 2] = [Self::Email, Self::Phone];
    /// The contacts of this kind among an ID's.
    fn pick<'a>(
        self,
        emails: &'a BTreeSet<String>,
        phones: &'a BTreeSet<String>,
    ) -> &'a BTreeSet<String> {
        match self {
            Self::Email => emails,
            Self::Phone => phones,
        }
    }
    fn link(self) -> DriverLink {
        match self {
            Self::Email => DriverLink::Email,
            Self::Phone => DriverLink::Phone,
        }
    }
}
/// How many IDs of each source carry each contact, among every ID held or new: a contact two
/// IDs of one source share says nothing of either.
type Carried = BTreeMap<(Contact, DriverSource, String), usize>;
fn carried(all: &[Member]) -> Carried {
    let mut out = Carried::new();
    for m in all {
        for contact in Contact::ALL {
            for value in contact.pick(&m.emails, &m.phones) {
                *out.entry((contact, m.source, value.clone())).or_default() += 1;
            }
        }
    }
    out
}

struct Plan<'a> {
    people: BTreeMap<String, Vec<Member>>,
    owner: BTreeMap<Key, String>,
    /// Every ID of every source, held or new: a name is unique only among all of them.
    all: Vec<Member>,
    carried: Carried,
    saved: &'a Saved,
}
impl Plan<'_> {
    fn holds(&self, person: &str, source: DriverSource) -> bool {
        self.people[person].iter().any(|m| m.source == source)
    }
    /// The one person a saved link joins this ID to.
    fn saved(&self, x: &Identity) -> Option<String> {
        let held = |source: &str, id: &str| {
            self.owner
                .iter()
                .find(|((s, i), _)| s.as_str() == source && i == id)
                .map(|(_, person)| person.clone())
        };
        match x.source.as_str() {
            AMAZON => held(PAYCOM, self.saved.links.get(&x.id)?),
            PAYCOM => self
                .saved
                .links
                .iter()
                .filter(|(_, code)| **code == x.id)
                .find_map(|(id, _)| held(AMAZON, id))
                .filter(|person| {
                    !self.people[person]
                        .iter()
                        .any(|m| m.source.as_str() == PAYCOM)
                }),
            _ => None,
        }
    }
    /// Whether `value` is on exactly one ID of `source`.
    fn alone(&self, contact: Contact, source: DriverSource, value: &str) -> bool {
        self.carried.get(&(contact, source, value.to_owned())) == Some(&1)
    }
    /// The one person `x` shares a contact of one kind with that no other ID of either
    /// source has, when their names agree and they lack an ID of `x`'s source.
    fn contact(&self, x: &Identity, contact: Contact) -> Option<String> {
        let mine: Vec<&String> = contact
            .pick(&x.emails, &x.phones)
            .iter()
            .filter(|value| self.alone(contact, x.source, value))
            .collect();
        if mine.is_empty() {
            return None;
        }
        let mut found = self.people.iter().filter(|(_, members)| {
            members.iter().any(|m| {
                m.source != x.source
                    && mine.iter().any(|value| {
                        contact.pick(&m.emails, &m.phones).contains(*value)
                            && self.alone(contact, m.source, value)
                    })
            })
        });
        let (person, members) = found.next()?;
        if found.next().is_some() || self.holds(person, x.source) {
            return None;
        }
        let names: Vec<String> = members
            .iter()
            .flat_map(|m| m.names.iter().cloned())
            .collect();
        agree(&names, &x.names).then(|| person.clone())
    }
    /// The one person `x` belongs to by name, when nothing else could: `same` says whether
    /// two name lists are one name. The person must lack an ID of `x`'s source, no other
    /// person may carry the name in another source, and no other ID of `x`'s source may
    /// carry the person's name.
    fn unique(&self, x: &Identity, same: impl Fn(&[String], &[String]) -> bool) -> Option<String> {
        let carries = |m: &Member| m.source != x.source && same(&m.names, &x.names);
        // Someone kept apart from Paycom on the meal-break page takes no Paycom ID by name,
        // in this pass or any later one.
        let kept = |members: &[Member]| {
            x.source.as_str() == PAYCOM
                && members
                    .iter()
                    .any(|m| m.source.as_str() == AMAZON && self.saved.separate.contains(&m.id))
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
            .filter(|m| m.source != x.source)
            .collect();
        let rival = self.all.iter().any(|m| {
            m.source == x.source && m.id != x.id && theirs.iter().any(|t| same(&m.names, &t.names))
        });
        (!rival).then_some(person)
    }
    fn decide(&self, x: &Identity) -> Option<(String, DriverLink)> {
        if x.source.as_str() == AMAZON && self.saved.separate.contains(&x.id) {
            return None;
        }
        if let Some(person) = self.saved(x) {
            return Some((person, DriverLink::Saved));
        }
        for contact in Contact::ALL {
            if let Some(person) = self.contact(x, contact) {
                return Some((person, contact.link()));
            }
        }
        // A name written alike somewhere decides before a variant could: when the exact
        // spelling is shared, a variant would only guess between the people sharing it.
        if self
            .people
            .values()
            .flatten()
            .any(|m| m.source != x.source && exact(&m.names, &x.names))
        {
            return self.unique(x, exact).map(|p| (p, DriverLink::Name));
        }
        self.unique(x, variant).map(|p| (p, DriverLink::Variant))
    }
}

/// Decides where each new ID goes, source by source in their order: Paycom's first, as its
/// roster lists who works for the DSP; the others then join them. `people` maps each code
/// to the IDs it holds.
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
    all.extend(incoming.iter().map(Member::of));
    let mut plan = Plan {
        people,
        owner,
        carried: carried(&all),
        all,
        saved,
    };
    let mut order: Vec<usize> = (0..incoming.len()).collect();
    order.sort_by_key(|&i| (incoming[i].source, incoming[i].id.clone()));
    let mut out = Vec::new();
    for index in order {
        let x = &incoming[index];
        let (person, link) = plan.decide(x).unwrap_or_else(|| {
            let fresh = format!("+{}", out.len());
            plan.people.insert(fresh.clone(), vec![]);
            (fresh, DriverLink::New)
        });
        plan.people.get_mut(&person).unwrap().push(Member::of(x));
        plan.owner.insert((x.source, x.id.clone()), person.clone());
        out.push((index, person, link));
    }
    out
}

#[cfg(test)]
#[path = "../tests/backend/matching.rs"]
mod tests;
