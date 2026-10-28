//! Queuing a text for a phone: the demo's stand-in delivers what Dispatch sends at once, and a text
//! past what one carries is refused before it's queued.
use super::*;
use crate::backend::phone::{LIMITS, model::Status, phones, support::store};
use dispatch_core::{db::now, testing};

/// A text `by` sends to `to`, from whichever phone it goes from.
fn to(to: &str, body: String, files: Vec<String>, by: &str) -> Outgoing {
    Outgoing {
        phone: None,
        to: to.into(),
        body,
        files,
        by: by.into(),
    }
}

#[tokio::test]
async fn a_text_sent_from_the_demos_stand_in_phone_is_delivered_at_once() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    crate::backend::phone::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let paired = phones(&db, &dsp).unwrap();
    assert_eq!(paired.len(), 1);
    assert!(paired[0].online);
    let received = db.phones_changes(&dsp, 0).unwrap();
    assert_eq!(received.texts.len(), 3);
    let sent = send(
        &db,
        &dsp,
        &to(
            "(425) 555-0199",
            "Come to the station first".into(),
            Vec::new(),
            &owner,
        ),
        now(),
    )
    .unwrap();
    assert_eq!(sent.phone, paired[0].id);
    assert_eq!(sent.number, "+14255550199");
    assert_eq!(
        sent.status,
        Some(Status::Delivered),
        "the demo's stand-in delivers at once"
    );
    let changed = db.phones_changes(&dsp, received.next).unwrap();
    assert_eq!(changed.texts, [sent]);
}

#[test]
fn a_text_past_what_one_carries_is_refused_before_it_is_queued() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    crate::backend::phone::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    // A text with nothing in it, too long, or with too many pictures is never queued.
    let many = (0..=LIMITS.files).map(|n| format!("file_{n}")).collect();
    let before = db.phones_changes(&dsp, 0).unwrap().next;
    for (outgoing, why) in [
        (
            to("(425) 555-0199", " ".into(), Vec::new(), &owner),
            "nothing in it",
        ),
        (
            to(
                "(425) 555-0199",
                "a".repeat(LIMITS.body + 1),
                Vec::new(),
                &owner,
            ),
            "too long",
        ),
        (
            to("(425) 555-0199", "The side door".into(), many, &owner),
            "too many pictures",
        ),
    ] {
        let refused = send(&db, &dsp, &outgoing, now());
        assert_eq!(refused.unwrap_err().code.as_str(), "invalid_input", "{why}");
    }
    assert_eq!(db.phones_changes(&dsp, before).unwrap().texts, []);
}
