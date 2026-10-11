//! A name as the owner reads it: trimmed, without control or format characters (which could
//! hide or reorder what they read), at most 80 characters. The same for a key's name, a
//! connection's, and the name an app gives itself.
/// `name` made fit to show, or None when nothing is left.
pub(crate) fn shown(name: &str) -> Option<String> {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !format_character(*c))
        .collect();
    let name: String = clean.trim().chars().take(80).collect();
    let name = name.trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Unicode's format characters (general category Cf): invisible, and some reorder text.
fn format_character(c: char) -> bool {
    matches!(
        u32::from(c),
        0xAD | 0x600..=0x605
            | 0x61C
            | 0x6DD
            | 0x70F
            | 0x890..=0x891
            | 0x8E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

#[cfg(test)]
#[path = "../tests/backend/names.rs"]
mod tests;
