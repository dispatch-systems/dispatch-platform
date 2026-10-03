use super::*;
#[test]
fn collected_clock_syntax_keeps_legacy_whitespace_and_hour_rules() {
    for (text, minutes) in [
        ("12:00 AM", Some(0)),
        ("12:00 pm", Some(720)),
        ("1:05 pm", Some(785)),
        ("\u{feff}08:00\u{00a0}", Some(480)),
        ("\u{0085}08:00", None),
        ("24:00", None),
        ("12:60", None),
        ("8:0", None),
        ("08:00:00", None),
        ("00:00 AM", None),
    ] {
        assert_eq!(parse_clock(text), minutes, "{text:?}");
    }
}
