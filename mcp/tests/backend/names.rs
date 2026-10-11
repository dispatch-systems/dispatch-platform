use super::*;

#[test]
fn names_are_trimmed_and_cut_short() {
    assert_eq!(shown("  Codex\n "), Some("Codex".into()));
    assert_eq!(shown(&"a".repeat(100)).unwrap().len(), 80);
    assert_eq!(shown(" \u{7} "), None);
    // Invisible and reordering characters cannot hide or disguise a name.
    assert_eq!(
        shown("\u{202E}edoC \u{200B}edualC\u{2066}\u{FEFF}"),
        Some("edoC edualC".into())
    );
    assert_eq!(shown("\u{200F}\u{2069}\u{E0041}"), None);
}
