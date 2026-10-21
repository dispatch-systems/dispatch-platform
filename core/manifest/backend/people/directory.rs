//! The directory: everyone a DSP's features name, each told apart under a code of their own,
//! for any feature that shows people or uses what is known of them. A contract core keeps, as
//! its people helpers use it: the feature that tells people apart exposes it, and
//! `people::everyone` and `people::photo` ask it, answering nothing in a build without one. It
//! tells people apart again when it hears `events::PeopleChanged`.
use super::{Photo, Source};
use crate::{Result, db::Store, foundation::names, text_enum};
use serde::Serialize;
use std::collections::BTreeMap;

/// Who everyone is, as the feature that tells people apart answers.
pub trait Directory: Sync {
    /// Everyone the DSP's features name, read afresh: `people::everyone` keeps it.
    fn everyone(&self, store: &Store, dsp: &str) -> Result<Everyone>;
    /// The photo of the person `code` names: the one their `Person` names the version of. A
    /// merged person's code answers for whoever kept their IDs.
    fn photo(&self, store: &Store, dsp: &str, code: &str) -> Result<Option<Photo>>;
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/manifest/api/generated/"))]
    /// What a person does for the DSP: a driver or a helper, as their sources' roles say or
    /// their records show; management, outside the departments the DSP counts as drivers or
    /// given a title; or someone who has left, whom no source lists and nothing has seen
    /// lately.
    pub enum PersonRole {
        Driver => "driver",
        Helper => "helper",
        Office => "office",
        Former => "former",
    }
}
impl PersonRole {
    /// Whether they have left.
    pub fn left(self) -> bool {
        self == Self::Former
    }
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/manifest/api/generated/"))]
    /// What someone decided a person does in management, which no source says: whoever holds
    /// one is management, whatever their sources show.
    pub enum PersonTitle {
        Owner => "owner",
        OpsManager => "ops_manager",
        FleetManager => "fleet_manager",
        Dispatch => "dispatch",
        HrManager => "hr_manager",
        Manager => "manager",
    }
}
impl PersonTitle {
    /// Every title, in the order they are offered.
    pub const ALL: [Self; 6] = [
        Self::Owner,
        Self::OpsManager,
        Self::FleetManager,
        Self::Dispatch,
        Self::HrManager,
        Self::Manager,
    ];
    /// The title as people read it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Owner => "Owner",
            Self::OpsManager => "OPs Manager",
            Self::FleetManager => "Fleet Manager",
            Self::Dispatch => "Dispatch",
            Self::HrManager => "HR Manager",
            Self::Manager => "Manager",
        }
    }
}

/// One person, as the directory knows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/manifest/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Person {
    /// Theirs for good and the same in every feature: what a feature keeps to name them. A
    /// merged person's code still finds whoever kept their IDs.
    pub code: String,
    /// The name they are shown by, as the first source that knows them writes it.
    pub name: String,
    /// Every way their sources write their name, the one shown first.
    pub names: Vec<String>,
    pub role: PersonRole,
    /// Their title, while they are still there.
    pub title: Option<PersonTitle>,
    /// Every ID they hold, by its source, the first linked first.
    pub ids: BTreeMap<Source, Vec<String>>,
    /// The email addresses and phone numbers their sources have for them, as written.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// Whether someone confirmed who they are, rather than a match.
    pub confirmed: bool,
    /// The version of their photo, which changes when the photo does: `people::photo` serves
    /// it.
    pub photo: Option<String>,
}
impl Person {
    /// Their IDs of `source`, the first linked first.
    pub fn ids_of(&self, source: Source) -> &[String] {
        self.ids.get(&source).map_or(&[], Vec::as_slice)
    }
}

/// Everyone the DSP's features name, by name, and the ways to find each.
#[derive(Debug, Default, Serialize)]
pub struct Everyone {
    people: Vec<Person>,
    /// Each code, merged ones too, with whoever holds it now, by their place in `people`.
    #[serde(skip)]
    codes: BTreeMap<String, usize>,
    /// Each ID, with whoever holds it.
    #[serde(skip)]
    held: BTreeMap<(Source, String), usize>,
}
impl Everyone {
    /// Everyone, from each person and each code merged into another, with the code it went to.
    pub fn new(mut people: Vec<Person>, merged: BTreeMap<String, String>) -> Self {
        people.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
        let mut codes: BTreeMap<String, usize> = people
            .iter()
            .enumerate()
            .map(|(at, person)| (person.code.clone(), at))
            .collect();
        for gone in merged.keys() {
            // A merge into someone since merged goes on to whoever kept their IDs.
            let mut to = gone;
            for _ in 0..32 {
                match merged.get(to) {
                    Some(next) => to = next,
                    None => break,
                }
            }
            if let Some(&at) = codes.get(to) {
                codes.insert(gone.clone(), at);
            }
        }
        let held = people
            .iter()
            .enumerate()
            .flat_map(|(at, person)| {
                person.ids.iter().flat_map(move |(source, ids)| {
                    ids.iter().map(move |id| ((*source, id.clone()), at))
                })
            })
            .collect();
        Self {
            people,
            codes,
            held,
        }
    }
    /// Everyone, by name.
    pub fn iter(&self) -> std::slice::Iter<'_, Person> {
        self.people.iter()
    }
    /// The person `code` names; a merged person's code finds whoever kept their IDs.
    pub fn person(&self, code: &str) -> Option<&Person> {
        self.codes.get(code).map(|&at| &self.people[at])
    }
    /// Whoever holds `id` of `source`.
    pub fn holding(&self, source: Source, id: &str) -> Option<&Person> {
        self.held
            .get(&(source, id.to_owned()))
            .map(|&at| &self.people[at])
    }
    /// Whoever `text` names, by name: by one of their IDs, their email address or phone
    /// number, or else any way their sources write their name, in part or by an initial
    /// (`names::matches`). A name exactly as someone's sources write it is them alone, beside
    /// others it is part of. By name.
    pub fn find(&self, text: &str) -> Vec<&Person> {
        let text = text.trim();
        let by = |found: &dyn Fn(&Person) -> bool| -> Vec<&Person> {
            self.people.iter().filter(|person| found(person)).collect()
        };
        if text.is_empty() {
            return vec![];
        }
        let ids = by(&|person| {
            person
                .ids
                .values()
                .flatten()
                .any(|id| id.eq_ignore_ascii_case(text))
        });
        if !ids.is_empty() {
            return ids;
        }
        if text.contains('@') {
            return by(&|person| {
                person
                    .emails
                    .iter()
                    .any(|email| email.eq_ignore_ascii_case(text))
            });
        }
        let dialed = digits(text);
        if dialed.len() >= 7 && !text.chars().any(char::is_alphabetic) {
            return by(&|person| person.phones.iter().any(|phone| digits(phone) == dialed));
        }
        let named = by(&|person| person.names.iter().any(|name| names::matches(text, name)));
        let exact: Vec<&Person> = named
            .iter()
            .copied()
            .filter(|person| person.names.iter().any(|name| names::same(text, name)))
            .collect();
        if exact.len() == 1 { exact } else { named }
    }
}
/// A phone number's last ten digits, without its country code or punctuation.
fn digits(phone: &str) -> String {
    let all: Vec<char> = phone.chars().filter(char::is_ascii_digit).collect();
    all[all.len().saturating_sub(10)..].iter().collect()
}

#[cfg(test)]
#[path = "../../tests/backend/directory.rs"]
mod tests;
