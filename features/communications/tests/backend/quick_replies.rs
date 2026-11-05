//! The DSP's quick replies: kept in their order, each trimmed; as many and as long as allowed and
//! no more; and a save over another's made since refused, as Settings refuses one.
use super::*;
use dispatch_core::testing;

#[test]
fn quick_replies_keep_their_order_within_their_limits_on_the_revision_read() {
    crate::backend::support::install();
    let (_root, db, dsp) = testing::bootstrapped();
    let none = quick_replies(&db, &dsp).unwrap();
    assert_eq!((none.replies.len(), none.revision), (0, 0));

    let saved = save_quick_replies(
        &db,
        &dsp,
        &[" On my way ".into(), "Call dispatch when you can".into()],
        0,
    )
    .unwrap();
    let read = quick_replies(&db, &dsp).unwrap();
    assert_eq!(read.replies, ["On my way", "Call dispatch when you can"]);
    assert_eq!((saved.revision, read.revision), (1, 1));

    // Another session saved since: the save on what it read first is refused, and nothing changes.
    let stale = save_quick_replies(&db, &dsp, &["Later".into()], 0).unwrap_err();
    assert_eq!(stale.code, "settings_changed_reload_before_saving");

    // Thirty, each of 500 characters, are kept; one more, one longer or one empty is refused.
    let longest = "a".repeat(LONGEST);
    let most = vec![longest.clone(); MOST];
    assert_eq!(save_quick_replies(&db, &dsp, &most, 1).unwrap().revision, 2);
    for refused in [
        vec![longest.clone(); MOST + 1],
        vec![format!("{longest}a")],
        vec!["  ".to_owned()],
    ] {
        let error = save_quick_replies(&db, &dsp, &refused, 2).unwrap_err();
        assert_eq!(error.code, "invalid_input");
    }
    assert_eq!(quick_replies(&db, &dsp).unwrap().replies, most);
}
