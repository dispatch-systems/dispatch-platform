//! An ID the phone side gives is opaque: its own are such IDs, as the app's may be, and nothing
//! longer or stranger is.
use super::{ID_MOST, identifier};
use dispatch_core::foundation::crypto;

#[test]
fn an_id_is_opaque_and_bounded() {
    for kind in ["phone", "text", "file"] {
        assert!(identifier(&crypto::id(kind).unwrap()), "its own {kind}");
    }
    let longest = "a".repeat(ID_MOST);
    assert!(identifier("sms-8812") && identifier(&longest));
    let longer = "a".repeat(ID_MOST + 1);
    for refused in ["", longer.as_str(), "../text", "text 1", "téxt"] {
        assert!(!identifier(refused), "{refused}");
    }
}
