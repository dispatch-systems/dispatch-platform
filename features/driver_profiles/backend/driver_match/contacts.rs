//! The contacts people are told apart by, each written one way: an email address in lower
//! case, a phone number as its ten digits.
use std::collections::BTreeSet;

/// An email address, or `None` for text that isn't one.
pub(crate) fn email(text: &str) -> Option<String> {
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
pub(crate) fn phone(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let ten = match digits.len() {
        10 => digits,
        11 if digits.starts_with('1') => digits[1..].to_owned(),
        _ => return None,
    };
    (ten.chars().collect::<BTreeSet<_>>().len() > 2).then_some(ten)
}

#[cfg(test)]
#[path = "../../tests/backend/driver_match/contacts.rs"]
mod tests;
