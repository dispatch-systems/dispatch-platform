//! Web Push as push services and browsers read it: a message encrypted as RFC 8291's own example
//! is, and a push's authorization signed by the server's key for the push service it goes to,
//! signed again for each no more than hourly.
use super::*;
use p256::ecdsa::{VerifyingKey, signature::Verifier};

fn bytes(text: &str) -> Vec<u8> {
    B64.decode(text).unwrap()
}

#[test]
fn a_message_is_encrypted_for_its_browser_as_rfc_8291_says() {
    // RFC 8291's Appendix A: the browser's keys, the server's key pair and salt for this message,
    // and the push it makes, which a browser holding its private key decrypts.
    let ephemeral =
        SecretKey::from_slice(&bytes("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
    let salt: [u8; 16] = bytes("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
    let pushed = encrypt_with(
        b"When I grow up, I want to be a watermelon",
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        "BTBZMqHH6r4Tts7J_aSIgg",
        &ephemeral,
        &salt,
    )
    .unwrap();
    assert_eq!(
        B64.encode(pushed),
        concat!(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInm",
            "YWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQ",
            "exSgSxsj_Qulcy4a-fN"
        )
    );
}

#[test]
fn a_push_is_authorized_by_the_servers_key_for_the_service_it_goes_to() {
    let vapid = Vapid::new(&secret().unwrap(), "https://admin.example.test");
    let endpoint = url::Url::parse("https://fcm.googleapis.com/fcm/send/browser-1").unwrap();
    let authorization = vapid.authorization(&endpoint, 1_800_000_000);
    let (token, key) = authorization
        .strip_prefix("vapid t=")
        .and_then(|rest| rest.split_once(", k="))
        .unwrap();
    assert_eq!(key, vapid.public);
    // Signed by the key it names, for the push service's origin, by whom, until when.
    let (signed, signature) = token.rsplit_once('.').unwrap();
    let signature = Signature::from_slice(&bytes(signature)).unwrap();
    VerifyingKey::from_sec1_bytes(&bytes(key))
        .unwrap()
        .verify(signed.as_bytes(), &signature)
        .unwrap();
    let (header, claims) = signed.split_once('.').unwrap();
    let read = |part: &str| serde_json::from_slice::<serde_json::Value>(&bytes(part)).unwrap();
    assert_eq!(
        read(header),
        serde_json::json!({"typ": "JWT", "alg": "ES256"})
    );
    assert_eq!(
        read(claims),
        serde_json::json!({
            "aud": "https://fcm.googleapis.com",
            "exp": 1_800_000_000 + 12 * 60 * 60,
            "sub": "https://admin.example.test",
        })
    );
}

#[test]
fn each_push_service_has_one_authorization_an_hour() {
    let vapid = Vapid::new(&secret().unwrap(), "https://admin.example.test");
    let at = |address: &str| url::Url::parse(address).unwrap();
    let (chrome, firefox) = (
        at("https://fcm.googleapis.com/fcm/send/browser-1"),
        at("https://updates.push.services.mozilla.com/wpush/v2/browser-2"),
    );
    let first = vapid.authorization(&chrome, 1_800_000_000);
    // Another browser at the same push service, within the hour: the same one.
    let other = at("https://fcm.googleapis.com/fcm/send/browser-3");
    assert_eq!(vapid.authorization(&other, 1_800_000_000 + 3599), first);
    // Another push service has its own; an hour on, the first is signed again.
    assert_ne!(vapid.authorization(&firefox, 1_800_000_000 + 10), first);
    let later = vapid.authorization(&chrome, 1_800_000_000 + 3600);
    assert_ne!(later, first);
    assert_eq!(vapid.authorization(&chrome, 1_800_000_000 + 3700), later);
}
