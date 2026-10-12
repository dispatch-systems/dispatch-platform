use super::*;
use axum::{body::Body, extract::ConnectInfo, http::Request, response::IntoResponse};
use std::{
    net::SocketAddr,
    os::unix::fs::{PermissionsExt, symlink},
};

// Protects the signed credential boundary: a copied or altered grant cannot outlive its
// audience or account. Existing support tests cover job leases, not host-issued claims.
// Runs real mint/authenticate entry points; no test-only product API is introduced.
#[test]
fn observation_credentials_expire_and_cannot_change_audience_or_account() {
    crate::testing::install(&[], &[]);
    let (root, mut db, _) = crate::testing::bootstrapped();
    let output = root.path().join("observation.json");
    let metadata = mint(&db.config, &output, None).unwrap();
    let credential: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    let raw = credential["token"].as_str().unwrap();
    assert!(metadata.get("token").is_none());
    assert_eq!(fs::metadata(&output).unwrap().mode() & 0o777, 0o600);
    assert!(mint(&db.config, &output, None).is_err());
    let key_path = db.config.platform().join("platform.key");
    let rejected_output = root.path().join("unsafe.json");
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o604)).unwrap();
    assert_eq!(
        mint(&db.config, &rejected_output, None).unwrap_err().code,
        "private_observation_path_required"
    );
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
    let held_key = root.path().join("held.key");
    fs::rename(&key_path, &held_key).unwrap();
    symlink(&held_key, &key_path).unwrap();
    assert_eq!(
        mint(&db.config, &rejected_output, None).unwrap_err().code,
        "private_observation_path_required"
    );
    fs::remove_file(&key_path).unwrap();
    fs::rename(&held_key, &key_path).unwrap();
    assert!(!rejected_output.exists());
    let auth = db.authenticate(raw, &Site::Admin).unwrap();
    assert!(read_only(&auth));
    assert_eq!(
        db.ensure_recent(&auth).unwrap_err().code,
        "observation_read_only"
    );
    let status = db.security_status(&auth).unwrap();
    assert!(status.verified && !status.required && !status.recent);
    assert!(db.authenticate(raw, &Site::Dsp("nll".into())).is_err());

    let (encoded, signature) = raw.strip_prefix(PREFIX).unwrap().split_once(':').unwrap();
    let original: Value = serde_json::from_slice(&B64.decode(encoded).unwrap()).unwrap();
    let signed = |value: &Value| {
        let unsigned = format!("{PREFIX}{}", B64.encode(serde_json::to_vec(value).unwrap()));
        format!("{unsigned}:{}", crypto::sign(&db.key, &unsigned))
    };
    for (field, value) in [
        ("origin", json!("https://another.example")),
        ("environment", json!("production")),
        ("user", json!("usr_unknown")),
        ("version", json!(999)),
        ("nonce", json!("short")),
        ("expiresAt", json!(now() + 24 * 60 * 60 * 1000)),
    ] {
        let mut value_with_change = original.clone();
        value_with_change[field] = value;
        assert!(
            db.authenticate(&signed(&value_with_change), &Site::Admin)
                .is_err(),
            "{field}"
        );
    }
    let mut future = original.clone();
    let future_at = now() + 60_000;
    future["issuedAt"] = json!(future_at);
    future["expiresAt"] = json!(future_at + LIFETIME);
    assert!(db.authenticate(&signed(&future), &Site::Admin).is_err());
    let mut expired = original.clone();
    let expired_at = now() - 1;
    expired["issuedAt"] = json!(expired_at - LIFETIME);
    expired["expiresAt"] = json!(expired_at);
    assert!(db.authenticate(&signed(&expired), &Site::Admin).is_err());
    let mut tampered = original;
    tampered["version"] = json!(99);
    let changed_without_signature = format!(
        "{PREFIX}{}:{signature}",
        B64.encode(serde_json::to_vec(&tampered).unwrap())
    );
    assert!(
        db.authenticate(&changed_without_signature, &Site::Admin)
            .is_err()
    );

    db.config.origin = "https://admin.example.test".into();
    assert_eq!(cookie_name(&db.config), "__Host-dispatch_observation_read");
    assert!(db.authenticate(raw, &Site::Admin).is_err());
    let secure_output = root.path().join("secure.json");
    mint(&db.config, &secure_output, None).unwrap();
    let secure: Value = serde_json::from_slice(&fs::read(&secure_output).unwrap()).unwrap();
    let state = crate::State::new(db.config.clone()).unwrap();
    let mut request = Request::builder()
        .method("POST")
        .uri(EXCHANGE)
        .header("host", "admin.example.test")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(Site::Admin);
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
    ));
    let (parts, _) = request.into_parts();
    let input = crate::server::http::middleware::head(&state, &parts, EXCHANGE).unwrap();
    let before_exchange = now();
    let response = exchange(&db, &input, secure["token"].as_str().unwrap())
        .unwrap()
        .into_response();
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("__Host-dispatch_observation_read="));
    assert!(cookie.contains("; HttpOnly; SameSite=Strict;") && cookie.ends_with("; Secure"));
    assert!(!cookie.contains("Domain="));
    let seconds: i64 = cookie
        .split("Max-Age=")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        seconds > 0 && seconds * 1000 <= secure["expiresAt"].as_i64().unwrap() - before_exchange
    );
    drop(state);
    db.config.origin = credential["origin"].as_str().unwrap().into();
    db.platform
        .exec(
            "UPDATE users SET version=version+1 WHERE id=?",
            [&auth.user.id],
        )
        .unwrap();
    assert!(db.authenticate(raw, &Site::Admin).is_err());
    db.platform
        .exec(
            "UPDATE users SET version=version-1,platform_owner=0 WHERE id=?",
            [&auth.user.id],
        )
        .unwrap();
    assert!(db.authenticate(raw, &Site::Admin).is_err());
    db.platform
        .exec(
            "UPDATE users SET platform_owner=1,status='disabled' WHERE id=?",
            [&auth.user.id],
        )
        .unwrap();
    assert!(db.authenticate(raw, &Site::Admin).is_err());
}
