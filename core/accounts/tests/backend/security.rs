use super::*;

#[test]
fn base32_and_totp_match_the_standard_vectors() {
    assert_eq!(
        base32(b"12345678901234567890"),
        "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
    );
    assert_eq!(
        decode_base32("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ").unwrap(),
        b"12345678901234567890"
    );
    assert!(decode_base32("NOT+A+SETUP+KEY").is_none());
    assert_eq!(totp(b"12345678901234567890", 1), "287082");
    assert_eq!(
        matching_totp(b"12345678901234567890", "287082", 30000),
        Some(1)
    );
    assert_eq!(
        matching_totp(b"12345678901234567890", "287082", 120000),
        None
    );
}
