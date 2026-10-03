use super::*;
use serde_json::json;

// Written by argon2 0.5, aes-gcm 0.10, hmac 0.12 and sha2 0.10. Stored passwords,
// sealed credentials and signed values must keep working across library updates.
const PASSWORD: &str = "$argon2id$v=19$m=19456,t=2,p=1$C6TJINtK+QXKynKYz7+0yw$\
    yaNW7UfSOeGStcAIvDqRZmRukfwf9cJZmg80zWOqwMs";
const SEALED: &str =
    "-jO4ORfuqy5LGd8k.Cwg6GUBJeaWuE_rJf8QCFxRxd6FMav27VPtiLKJ4xAte6VN8vzVoHrSI1QGVyrnGsixXMpgjZA";

#[test]
fn values_written_by_earlier_libraries_still_verify() {
    let key = [7u8; 32];
    assert!(check_password("correct horse battery", PASSWORD));
    assert!(!check_password("correct horse batterz", PASSWORD));
    assert_eq!(
        decrypt(&key, "dsp_vector:paycom:2", SEALED).unwrap(),
        json!({"username":"vector","answers":[1,2,3]})
    );
    assert!(decrypt(&key, "dsp_other:paycom:2", SEALED).is_err());
    assert!(decrypt(&[8u8; 32], "dsp_vector:paycom:2", SEALED).is_err());
    assert_eq!(
        sign(&key, "session:vector"),
        "1E5nOJSJz28c6CcrnFuGjlPdjVDlS7Kvti2nyJJN8lA"
    );
    assert_eq!(
        sha("dispatch"),
        "db8d1b6d64e4ec90ceb335fc4344e75799fdc20cd236d27cc85ed4783a33d3fa"
    );
}

#[test]
fn new_values_keep_the_same_parameters_and_round_trip() {
    let key = [7u8; 32];
    let hash = hash_password("correct horse battery").unwrap();
    assert!(
        hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "{hash}"
    );
    assert!(check_password("correct horse battery", &hash));
    assert!(!check_password("wrong", &hash));
    assert_ne!(hash, hash_password("correct horse battery").unwrap());
    let sealed = encrypt(&key, "binding", &json!({"a":1})).unwrap();
    assert_eq!(decrypt(&key, "binding", &sealed).unwrap(), json!({"a":1}));
    let secret =
        json!({"password":"private","securityAnswers":["00123","two","three","four","five"]});
    let sealed_secret = encrypt(&key, "dsp-one", &secret).unwrap();
    assert_eq!(decrypt(&key, "dsp-one", &sealed_secret).unwrap(), secret);
    assert!(decrypt(&key, "dsp-two", &sealed_secret).is_err());
    assert!(decrypt(&key, "dsp-one", &format!("{sealed_secret}x")).is_err());
    assert!(!sealed_secret.contains("private"));
    let (nonce, body) = sealed.split_once('.').unwrap();
    assert!(decrypt(&key, "binding", &format!("{nonce}A.{body}")).is_err());
}

#[test]
fn new_passwords_are_long_and_not_common() {
    for value in [
        "short-password",
        "passwordpassword",
        " CorrectHorseBatteryStaple ",
    ] {
        assert_eq!(hash_password(value).unwrap_err().code, "invalid_password");
    }
    assert!(hash_password("a unique dispatch password").is_ok());
}

#[test]
fn recovery_codes_are_short_grouped_and_random() {
    let first = recovery_code().unwrap();
    let second = recovery_code().unwrap();
    assert_ne!(first, second);
    assert_eq!(first.len(), 19);
    let groups = first.split('-').collect::<Vec<_>>();
    assert_eq!(groups.len(), 4);
    assert!(groups.iter().all(|group| group.len() == 4));
    assert!(groups.iter().all(|group| {
        group
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
    }));
}
