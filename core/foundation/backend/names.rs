//! How names are shown and sorted. Sources format one name differently, "REYES, ANTONIO" in
//! one and "Antonio Reyes" in another; each is shown as people read it.
use std::cmp::Ordering;

/// A name as people read it: "First Last". A source that writes only capitals is put in
/// ordinary case; any other spelling is kept as written.
pub fn display(name: &str) -> String {
    let ordered = name
        .split_once(',')
        .map(|(last, first)| format!("{} {}", first.trim(), last.trim()))
        .unwrap_or_else(|| name.trim().to_owned());
    let words: Vec<&str> = ordered.split_whitespace().collect();
    if ordered.chars().any(char::is_lowercase) {
        return words.join(" ");
    }
    words
        .iter()
        .map(|word| {
            let mut out = String::with_capacity(word.len());
            let mut start = true;
            for c in word.chars() {
                if start {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                start = matches!(c, '-' | '\'' | '’');
            }
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn display_name(name: &str, order: &str) -> String {
    let parts: Vec<_> = name
        .split(|c: char| c.is_whitespace() && c != '\u{0085}' || c == '\u{feff}')
        .filter(|s| !s.is_empty())
        .collect();
    if order == "last_first" && parts.len() > 1 {
        format!(
            "{}, {}",
            parts.last().unwrap(),
            parts[..parts.len() - 1].join(" ")
        )
    } else {
        name.into()
    }
}
pub fn compare(a: &str, b: &str) -> Ordering {
    static COLLATOR: std::sync::OnceLock<icu_collator::CollatorBorrowed<'static>> =
        std::sync::OnceLock::new();
    COLLATOR
        .get_or_init(|| {
            icu_collator::Collator::try_new(Default::default(), Default::default())
                .expect("compiled Unicode collation data")
        })
        .compare(a, b)
}

#[cfg(test)]
#[path = "../tests/backend/names.rs"]
mod tests;
