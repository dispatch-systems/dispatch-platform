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

/// A passkey as Dispatch stored it before webauthn-rs 0.6, with the credential ID 0 to 31.
const STORED_PASSKEY: &str = r#"{"cred":{"cred_id":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8","cred":{"type_":"ES256","key":{"EC_EC2":{"curve":"SECP256R1","x":"H77oDThr2F0L6okUynF10nYeqrVsT2fJ95f-cBpfqxw","y":"7mHiT1_v2OGeonqMqaukozmFsrvIIPO0suWT4xnpYxM"}}},"counter":7,"transports":null,"user_verified":true,"backup_eligible":true,"backup_state":true,"registration_policy":"required","extensions":{"cred_protect":"NotRequested","hmac_create_secret":"NotRequested","appid":"NotRequested","cred_props":"NotRequested"},"attestation":{"data":"None","metadata":"None"},"attestation_format":"none"}}"#;

#[test]
fn a_stored_passkey_still_reads_under_the_id_it_was_stored_with() {
    let key: Passkey = serde_json::from_str(STORED_PASSKEY).unwrap();
    assert_eq!(key.cred_id(), &(0..32).collect::<Vec<u8>>());
    assert_eq!(
        passkey_id(&key).unwrap(),
        crypto::sha("\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8\"")
    );
}
