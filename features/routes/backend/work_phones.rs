//! Each itinerary's work phone, kept in its row in both route databases: the company phone its
//! driver's records show with it, by its last ten digits.

/// A phone number as its last ten digits, which is how two records are compared.
pub(super) fn digits(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 10).then(|| digits[digits.len() - 10..].to_owned())
}
