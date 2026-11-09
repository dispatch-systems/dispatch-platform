//! Literal stored-format vectors: changing v1 would strand a shipped rollback reader.
use super::*;
use serde_json::json;

#[test]
fn v1_canonical_bytes_and_keys_preserve_payroll_distinctions() -> Result<()> {
    let empty = Payload::punches(&json!([]))?;
    assert_eq!(empty.body, "[]");
    assert_eq!(
        empty.digest,
        "7412eb7a35ec10359b6c19b0d76f8fdc458d976561d6e028e9a52186472cd74a"
    );
    let value: Value =
        serde_json::from_str(r#"[{"out":"16:30","inKind":null,"in":"08:00","hours":8}]"#)?;
    let payload = Payload::punches(&value)?;
    assert_eq!(
        payload.body,
        r#"[{"hours":8,"in":"08:00","inKind":null,"out":"16:30"}]"#
    );
    assert_eq!(
        payload.digest,
        "1e06f0a90f202b272317e9c8cf54275f58f6fcdf61227b0a7b97c2eb32be8d55"
    );
    let reordered: Value =
        serde_json::from_str(r#"[{"hours":8,"in":"08:00","out":"16:30","inKind":null}]"#)?;
    assert_eq!(Payload::punches(&reordered)?.digest, payload.digest);
    for changed in [
        json!([{"hours":8,"in":"08:00","out":"16:30"}]),
        json!([{"hours":8.0,"in":"08:00","inKind":null,"out":"16:30"}]),
        json!([{"hours":8,"in":" 08:00","inKind":null,"out":"16:30"}]),
        json!([{"hours":8,"in":"08:01","inKind":null,"out":"16:30"}]),
    ] {
        assert_ne!(Payload::punches(&changed)?.digest, payload.digest);
    }
    let mut ordered = json!([{"in":"08:00"}, {"in":"12:30"}]);
    let before = Payload::punches(&ordered)?.digest;
    ordered.as_array_mut().unwrap().reverse();
    assert_ne!(Payload::punches(&ordered)?.digest, before);
    let mut data = json!({"employees":[],"from":"2026-08-09","timecards":[],
        "to":"2026-08-22","collectedAt":"2026-08-23T00:00:00Z"});
    let capture = Payload::capture(&data)?;
    assert_eq!(
        capture.body,
        r#"{"employees":[],"from":"2026-08-09","timecards":[],"to":"2026-08-22"}"#
    );
    assert_eq!(
        capture.digest,
        "3a5e8e3176a7655116c0ae25d16a731c2d9387449faeae6d7ef35066aa985cf0"
    );
    data["collectedAt"] = json!("2026-08-24T00:00:00Z");
    assert_eq!(Payload::capture(&data)?.digest, capture.digest);
    data["sources"] = json!([{"url":"https://paycom.example/changed"}]);
    assert_ne!(Payload::capture(&data)?.digest, capture.digest);
    // Supported bodies win; absent/unknown refs accept only full inline data.
    let mut row = json!({"punches":r#"{"$dispatchPayload":"paycom.punches.v1"}"#,
        "payload_format":PUNCHES,"payload_body":payload.body});
    punches(&mut row)?;
    assert_eq!(row["punches"], value);
    assert!(row.get("payload_body").is_none());
    let mut fallback = json!({"punches":"[]","payload_format":"paycom.punches.v2",
        "payload_body":"[null]"});
    punches(&mut fallback)?;
    assert_eq!(fallback["punches"], json!([]));
    for (inline, body) in [
        (r#"{"$dispatchPayload":"paycom.punches.v1"}"#, Value::Null),
        ("[]", json!("[null]")),
        ("[]", json!("{")),
    ] {
        let mut row = json!({"punches":inline,"payload_format":PUNCHES,"payload_body":body});
        assert_eq!(
            punches(&mut row).unwrap_err().code,
            "paycom_payload_unavailable"
        );
    }
    Ok(())
}

#[test]
fn pool_rejects_updates_and_conflicting_bytes_without_replacing_a_body() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    crate::testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&crate::FEATURE],
    );
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let db = Db::create(
        &root.path().join("paycom.sqlite"),
        dispatch_paycom::DATABASE,
        "",
    )?;
    let mut payload = Payload::punches(&serde_json::json!([]))?;
    db.transaction(|| payload.intern(&db))?;
    assert!(db.exec("UPDATE paycom_payloads SET body='[]'", []).is_err());
    // Deliberately force a conflicting candidate, retaining the already stored key.
    payload.body = "[{}]".into();
    assert_eq!(
        db.transaction(|| payload.intern(&db)).unwrap_err().code,
        "paycom_payload_collision"
    );
    assert_eq!(
        db.one("SELECT body FROM paycom_payloads", [])?.unwrap()["body"],
        "[]"
    );
    Ok(())
}
