//! The DSP's quick replies: kept in their order, each trimmed; as many and as long as allowed and
//! no more; a save over another's made since refused, as Settings refuses one; and each save
//! that changed them in the audit log, with the replies it added and removed.
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
        "me",
    )
    .unwrap();
    let read = quick_replies(&db, &dsp).unwrap();
    assert_eq!(read.replies, ["On my way", "Call dispatch when you can"]);
    assert_eq!((saved.revision, read.revision), (1, 1));

    // Another session saved since: the save on what it read first is refused, and nothing changes.
    let stale = save_quick_replies(&db, &dsp, &["Later".into()], 0, "me").unwrap_err();
    assert_eq!(stale.code, "settings_changed_reload_before_saving");

    // Thirty, each of 500 characters, are kept; one more, one longer or one empty is refused.
    let longest = "a".repeat(LONGEST);
    let most = vec![longest.clone(); MOST];
    assert_eq!(
        save_quick_replies(&db, &dsp, &most, 1, "me")
            .unwrap()
            .revision,
        2
    );
    for refused in [
        vec![longest.clone(); MOST + 1],
        vec![format!("{longest}a")],
        vec!["  ".to_owned()],
    ] {
        let error = save_quick_replies(&db, &dsp, &refused, 2, "me").unwrap_err();
        assert_eq!(error.code, "invalid_input");
    }
    assert_eq!(quick_replies(&db, &dsp).unwrap().replies, most);

    // The same list saved again changes nothing; each save that changed them is logged, with what
    // it added and removed, each by its first hundred characters, and none refused is.
    save_quick_replies(&db, &dsp, &most, 2, "me").unwrap();
    let events = testing::audits(&db, None).unwrap();
    let changes: Vec<serde_json::Value> = events
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .filter(|event| event["action"] == "communications.quick_replies_saved")
        .map(|event| event["changes"].clone())
        .collect();
    let change = |from: Option<&str>, to: Option<&str>| serde_json::json!({"field": "reply", "from": from, "to": to});
    assert_eq!(
        changes,
        [
            serde_json::json!([
                change(None, Some("On my way")),
                change(None, Some("Call dispatch when you can")),
            ]),
            serde_json::json!([
                change(Some("On my way"), None),
                change(Some("Call dispatch when you can"), None),
                change(None, Some(&format!("{}…", "a".repeat(100)))),
            ]),
        ]
    );
}
