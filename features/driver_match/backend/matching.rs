//! Who a new ID belongs to. Pure: it reads what is known and decides, and the store writes
//! the decisions. Only certain answers link an ID to someone, tried in the order `decide`
//! lists them; anything less gives the ID a person of its own, for a person to review.
use super::contacts::Contact;
use super::names::{agree, exact, variant};
use super::sources::{Identity, Key};
use crate::api::types::IdLink;
use dispatch_core::manifest::people::DriverSource;
use std::collections::{BTreeMap, BTreeSet};

/// Whom an ID goes to: someone who holds IDs already, by their code, or a new person, the
/// plan's `n`th, whom the store gives a code.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    Person(String),
    New(usize),
}
/// Where one new ID goes, and how: `index` is its place among the new IDs.
#[derive(Debug)]
pub(crate) struct Decision {
    pub index: usize,
    pub target: Target,
    pub link: IdLink,
}

/// The people of each source who carry each contact, by the person their source says each ID
/// is, among every ID held or new: a contact two people of one source share says nothing of
/// either.
type Carried = BTreeMap<(Contact, DriverSource, String), BTreeSet<String>>;
fn carried(all: &[Identity]) -> Carried {
    let mut out = Carried::new();
    for x in all {
        for (kind, value) in x.contacts.all() {
            out.entry((kind, x.source, value.to_owned()))
                .or_default()
                .insert(x.one().to_owned());
        }
    }
    out
}

struct Plan {
    /// Every person and the IDs they hold, as they are known now.
    holders: BTreeMap<Target, Vec<Identity>>,
    owner: BTreeMap<Key, Target>,
    /// Every ID of every source, held or new: a name is unique only among all of them.
    all: Vec<Identity>,
    carried: Carried,
}
impl Plan {
    /// Where `x` goes, by the first rule that is certain:
    /// 1. Another ID its source says is the same person's, when one person holds those and
    ///    their names agree (`account`).
    /// 2. A contact of each kind in turn, that only the two of them have, when their names
    ///    agree (`email`, `phone`).
    /// 3. A name nobody else carries, written alike (`name`), or else written another way
    ///    (`variant`).
    fn decide(&self, x: &Identity) -> Option<(Target, IdLink)> {
        if let Some(person) = self.account(x) {
            return Some((person, IdLink::Account));
        }
        for kind in Contact::ALL {
            if let Some(person) = self.contact(x, kind) {
                return Some((person, kind.link()));
            }
        }
        self.by_name(x)
    }

    /// The names everyone holding `person`'s IDs is written by.
    fn names(&self, person: &Target) -> Vec<String> {
        self.holders[person]
            .iter()
            .flat_map(|held| held.names.iter().cloned())
            .collect()
    }
    /// Whether `person` may take `x` by a contact or a name: one who has an ID of its source
    /// already takes no second that way, as two people may share both.
    fn open(&self, person: &Target, x: &Identity) -> bool {
        !self.holders[person]
            .iter()
            .any(|held| held.source == x.source)
    }

    /// The one person holding `x`'s source's other IDs for the same person, when their names
    /// agree: with those IDs split among people, or names a shared address can't vouch for,
    /// a person decides.
    fn account(&self, x: &Identity) -> Option<Target> {
        let holders: BTreeSet<&Target> = x
            .same_as
            .iter()
            .filter_map(|id| self.owner.get(&(x.source, id.clone())))
            .collect();
        let [person] = holders.into_iter().collect::<Vec<_>>()[..] else {
            return None;
        };
        agree(&self.names(person), &x.names).then(|| person.clone())
    }
    /// Whether `value` is a contact of `kind` on exactly one person of `source`.
    fn alone(&self, kind: Contact, source: DriverSource, value: &str) -> bool {
        self.carried
            .get(&(kind, source, value.to_owned()))
            .is_some_and(|people| people.len() == 1)
    }
    /// The one person `x` shares a contact of `kind` with that no other person of either
    /// source has.
    fn contact(&self, x: &Identity, kind: Contact) -> Option<Target> {
        let mine: Vec<&str> = x
            .contacts
            .of(kind)
            .filter(|value| self.alone(kind, x.source, value))
            .collect();
        if mine.is_empty() {
            return None;
        }
        let shares = |held: &Identity| {
            held.source != x.source
                && mine.iter().any(|value| {
                    held.contacts.has(kind, value) && self.alone(kind, held.source, value)
                })
        };
        let mut found = self
            .holders
            .iter()
            .filter(|(_, ids)| ids.iter().any(shares))
            .map(|(person, _)| person);
        let person = found.next()?;
        (found.next().is_none() && self.open(person, x) && agree(&self.names(person), &x.names))
            .then(|| person.clone())
    }
    /// The one person `x` belongs to by name. A name written alike somewhere decides before a
    /// variant could: when the exact spelling is shared, a variant would only guess between
    /// the people sharing it.
    fn by_name(&self, x: &Identity) -> Option<(Target, IdLink)> {
        let written_alike = self
            .holders
            .values()
            .flatten()
            .any(|held| held.source != x.source && exact(&held.names, &x.names));
        if written_alike {
            self.unique(x, exact).map(|p| (p, IdLink::Name))
        } else {
            self.unique(x, variant).map(|p| (p, IdLink::Variant))
        }
    }
    /// The one person whose IDs of other sources carry `x`'s name, as `alike` compares names,
    /// when no other ID of `x`'s source, but its own other IDs, carries that person's name.
    fn unique(&self, x: &Identity, alike: fn(&[String], &[String]) -> bool) -> Option<Target> {
        let carries = |held: &Identity| held.source != x.source && alike(&held.names, &x.names);
        let mut matched = self
            .holders
            .iter()
            .filter(|(_, ids)| ids.iter().any(carries))
            .map(|(person, _)| person);
        let person = matched.next()?;
        if matched.next().is_some() || !self.open(person, x) {
            return None;
        }
        let theirs: Vec<&Identity> = self.holders[person]
            .iter()
            .filter(|held| held.source != x.source)
            .collect();
        let rival = self.all.iter().any(|other| {
            other.source == x.source
                && other.id != x.id
                && !x.same_as.contains(&other.id)
                && theirs.iter().any(|t| alike(&other.names, &t.names))
        });
        (!rival).then(|| person.clone())
    }
}

/// Decides where each new ID goes, source by source in their order, so the first source's IDs
/// are there for the others to join. `holders` maps each code to the IDs it holds.
pub(crate) fn plan(
    holders: BTreeMap<String, Vec<Identity>>,
    incoming: &[Identity],
) -> Vec<Decision> {
    let holders: BTreeMap<Target, Vec<Identity>> = holders
        .into_iter()
        .map(|(code, ids)| (Target::Person(code), ids))
        .collect();
    let mut owner = BTreeMap::new();
    for (person, ids) in &holders {
        for held in ids {
            owner.insert((held.source, held.id.clone()), person.clone());
        }
    }
    let mut all: Vec<Identity> = holders.values().flatten().cloned().collect();
    all.extend(incoming.iter().cloned());
    let mut plan = Plan {
        holders,
        owner,
        carried: carried(&all),
        all,
    };
    let mut order: Vec<usize> = (0..incoming.len()).collect();
    order.sort_by_key(|&i| (incoming[i].source, incoming[i].id.clone()));
    let mut decisions = Vec::new();
    let mut new = 0;
    for index in order {
        let x = &incoming[index];
        let (target, link) = plan.decide(x).unwrap_or_else(|| {
            new += 1;
            (Target::New(new - 1), IdLink::New)
        });
        plan.holders
            .entry(target.clone())
            .or_default()
            .push(x.clone());
        plan.owner.insert((x.source, x.id.clone()), target.clone());
        decisions.push(Decision {
            index,
            target,
            link,
        });
    }
    decisions
}

#[cfg(test)]
#[path = "../tests/backend/matching.rs"]
mod tests;
