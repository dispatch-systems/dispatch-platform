//! Each number is kept one way, so a text and its reply meet in one conversation.
use super::normal;

#[test]
fn writes_each_number_one_way() {
    for (raw, kept) in [
        ("(206) 555-0148", Some("+12065550148")),
        ("206.555.0148", Some("+12065550148")),
        ("+1 206 555 0148", Some("+12065550148")),
        ("12065550148", Some("+12065550148")),
        ("+44 20 7946 0958", Some("+442079460958")),
        ("72166", Some("72166")),
        ("", None),
        ("call me", None),
        ("555-01", None),
    ] {
        assert_eq!(normal(raw).as_deref(), kept, "{raw}");
    }
}
