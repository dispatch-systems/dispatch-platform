//! The contacts people are told apart by. Each kind is written one way to compare: an email
//! address in lower case, a phone number as its ten digits. A new kind of contact, as a badge
//! number, is one more `Contact`: how it is written, and how a match and a pair by it read.
use crate::api::types::{DriverEvidenceKind, DriverLink};
use std::collections::{BTreeMap, BTreeSet};

/// A kind of contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Contact {
    Email,
    Phone,
}
impl Contact {
    pub const ALL: [Self; 2] = [Self::Email, Self::Phone];
    /// The contact written the one way it is compared, or `None` for text that isn't one.
    fn compared(self, text: &str) -> Option<String> {
        match self {
            Self::Email => email(text),
            Self::Phone => phone(text),
        }
    }
    /// Whether text that isn't one is still shown as written: a number that isn't North
    /// American is still someone's phone, though nobody is matched by it.
    fn shown_unmatched(self) -> bool {
        self == Self::Phone
    }
    /// How an ID joined someone it shares this kind of contact with.
    pub fn link(self) -> DriverLink {
        match self {
            Self::Email => DriverLink::Email,
            Self::Phone => DriverLink::Phone,
        }
    }
    /// Why two people sharing this kind of contact might be one.
    pub fn evidence(self) -> DriverEvidenceKind {
        match self {
            Self::Email => DriverEvidenceKind::SameEmail,
            Self::Phone => DriverEvidenceKind::SamePhone,
        }
    }
}

/// The contacts of every kind an ID's sources have for them: each written the one way it is
/// compared, and as its source wrote it, to show.
#[derive(Clone, Debug, Default)]
pub(crate) struct Contacts {
    compared: BTreeMap<Contact, BTreeSet<String>>,
    written: BTreeMap<Contact, Vec<String>>,
}
impl Contacts {
    /// Adds a contact of `kind` as a source wrote it, once.
    pub fn add(&mut self, kind: Contact, text: &str) {
        let written = text.trim();
        let new = match kind.compared(written) {
            Some(compared) => self.compared.entry(kind).or_default().insert(compared),
            None => {
                kind.shown_unmatched()
                    && !written.is_empty()
                    && !self.written(kind).iter().any(|w| w == written)
            }
        };
        if new {
            self.written
                .entry(kind)
                .or_default()
                .push(written.to_owned());
        }
    }
    /// The contacts of `kind`, as they are compared.
    pub fn of(&self, kind: Contact) -> impl Iterator<Item = &str> {
        self.compared
            .get(&kind)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
    /// Whether one of the contacts of `kind` is `value`, as it is compared.
    pub fn has(&self, kind: Contact, value: &str) -> bool {
        self.compared
            .get(&kind)
            .is_some_and(|set| set.contains(value))
    }
    /// The contacts of `kind`, as their sources wrote them.
    pub fn written(&self, kind: Contact) -> &[String] {
        self.written.get(&kind).map_or(&[], Vec::as_slice)
    }
    /// Every contact, as it is compared, with its kind.
    pub fn all(&self) -> impl Iterator<Item = (Contact, &str)> {
        self.compared
            .iter()
            .flat_map(|(kind, set)| set.iter().map(move |value| (*kind, value.as_str())))
    }
}

/// An email address in lower case, or `None` for text that isn't one.
fn email(text: &str) -> Option<String> {
    let email = text.trim().to_lowercase();
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    (!local.is_empty()
        && domain.contains('.')
        && email.len() <= 254
        && !email.chars().any(char::is_whitespace))
    .then_some(email)
}
/// A North American phone number's ten digits, or `None` for any other number: one a person
/// could only share by chance, such as 000-000-0000, says nothing either.
fn phone(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let ten = match digits.len() {
        10 => digits,
        11 if digits.starts_with('1') => digits[1..].to_owned(),
        _ => return None,
    };
    (ten.chars().collect::<BTreeSet<_>>().len() > 2).then_some(ten)
}

#[cfg(test)]
#[path = "../tests/backend/contacts.rs"]
mod tests;
