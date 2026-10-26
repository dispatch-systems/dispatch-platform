//! An ID texting gives is opaque: Phones' own are such IDs, as another's may be, and nothing longer
//! or stranger is.
use dispatch_core::foundation::crypto;
use dispatch_phones_contract::{ID_MOST, identifier};

#[test]
fn an_id_is_opaque_and_bounded() {
    for kind in ["phone", "text", "file"] {
        assert!(identifier(&crypto::id(kind).unwrap()), "Phones' own {kind}");
    }
    let longest = "a".repeat(ID_MOST);
    assert!(identifier("sms-8812") && identifier(&longest));
    let longer = "a".repeat(ID_MOST + 1);
    for refused in ["", longer.as_str(), "../text", "text 1", "téxt"] {
        assert!(!identifier(refused), "{refused}");
    }
}
