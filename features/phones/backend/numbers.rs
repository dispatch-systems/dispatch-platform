//! Phone numbers as Phones keeps them, so one number is always written one way: `+1` and ten
//! digits for one in North America, `+` and its digits for one dialed with its country code,
//! and the digits alone for a short code, as a carrier's or a business's.

/// `raw` as kept, or none for something that isn't a number.
pub fn normal(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 32 {
        return None;
    }
    let plus = raw.starts_with('+');
    if !raw
        .chars()
        .all(|c| c.is_ascii_digit() || " +-().".contains(c))
    {
        return None;
    }
    let bare = raw.chars().all(|c| c.is_ascii_digit());
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 if !plus => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        8..=15 if plus => Some(format!("+{digits}")),
        3..=7 if bare => Some(digits),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/backend/numbers.rs"]
mod tests;
