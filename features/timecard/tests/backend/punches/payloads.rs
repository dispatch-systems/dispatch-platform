//! Literal stored-format reader fixtures remain compatible with older pooled data.
use super::*;
use dispatch_core::db::Db;
use serde_json::json;

#[test]
fn pooled_punches_and_inline_fallback_preserve_stored_shapes() -> Result<()> {
    let body = r#"[{"hours":8,"in":"08:00","inKind":null,"out":"16:30"}]"#;
    let value: Value = serde_json::from_str(body)?;
    // Supported bodies win; absent/unknown refs accept only full inline data.
    let mut row = json!({"punches":r#"{"$dispatchPayload":"paycom.punches.v1"}"#,
        "payload_format":PUNCHES,"payload_body":body});
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
fn stored_pool_rejects_updates_and_conflicting_keys_without_replacing_a_body() -> Result<()> {
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
    db.exec(
        "INSERT INTO paycom_payloads VALUES ('paycom.punches.v1',?, '[]')",
        ["7412eb7a35ec10359b6c19b0d76f8fdc458d976561d6e028e9a52186472cd74a"],
    )?;
    assert!(db.exec("UPDATE paycom_payloads SET body='[]'", []).is_err());
    assert!(
        db.exec(
            "INSERT INTO paycom_payloads VALUES ('paycom.punches.v1',?, '[{}]')",
            ["7412eb7a35ec10359b6c19b0d76f8fdc458d976561d6e028e9a52186472cd74a"],
        )
        .is_err()
    );
    assert_eq!(
        db.one("SELECT body FROM paycom_payloads", [])?.unwrap()["body"],
        "[]"
    );
    Ok(())
}
