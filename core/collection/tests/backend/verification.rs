use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn verifier_and_regular_provider_sessions_exclude_each_other_across_file_handles() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dsp = format!("dsp_{}", "a".repeat(32));
    let paycom = Provider::new("paycom");
    let lease = credential_lease(root.path(), &dsp, paycom)?;
    let refusal = credential_lease(root.path(), &dsp, paycom).unwrap_err();
    assert_eq!(refusal.code, "browser_capacity_busy");
    // A different provider account remains available, while the exact account is held.
    let other = credential_lease(root.path(), &dsp, Provider::new("cortex"))?;
    drop(lease);
    let resumed = credential_lease(root.path(), &dsp, paycom)?;
    drop((resumed, other));
    Ok(())
}

#[test]
fn rotating_or_disabling_the_source_revokes_verification_and_removes_its_temporary_vault()
-> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let dsp = format!("dsp_{}", "a".repeat(32));
    let provider = Provider::new("paycom");
    let source_connection = root.path().join("source.sqlite");
    let db = rusqlite::Connection::open(&source_connection)?;
    db.execute_batch(
        "CREATE TABLE connections(provider TEXT PRIMARY KEY,enabled INTEGER,revision INTEGER); \
         INSERT INTO connections VALUES('paycom',1,3)",
    )?;
    let target = root.path().join("temporary.enc");
    db::write_private(&target, b"encrypted-ephemeral-value")?;
    let reference = Credentials {
        path: target.clone(),
        _account_lease: credential_lease(root.path(), &dsp, provider)?,
        source_connection,
        revision: 3,
        provider,
    };
    assert!(reference.current()?);
    db.execute_batch("UPDATE connections SET revision=4")?;
    assert!(!reference.current()?);
    db.execute_batch("UPDATE connections SET revision=3,enabled=0")?;
    assert!(!reference.current()?);
    drop(reference);
    assert!(!target.exists());
    let _released = credential_lease(root.path(), &dsp, provider)?;
    Ok(())
}

#[test]
fn native_verification_binds_the_configured_account_and_preserves_its_fixture_url() -> Result<()> {
    crate::testing::install(&[], &[]);
    let (_root, mut db, dsp) = crate::testing::bootstrapped();
    let original = db.find_dsp(&dsp)?.code;
    db.config.fixture = false;
    db.set_verification_profile(
        &dsp,
        "Configured verification logistics",
        "America/Chicago",
        "TEST",
        "TST1",
    )?;
    assert_eq!(db.find_dsp(&dsp)?.code, original);
    let target = db.find_dsp(&dsp)?;
    assert_eq!(target.name, "Configured verification logistics");
    assert_eq!(target.timezone, "America/Chicago");
    let profile = db.profile(&dsp)?;
    assert_eq!(profile.abbreviation, "TEST");
    assert_eq!(profile.station_code, "TST1");
    db.config.environment = "production".into();
    let refused = db
        .set_verification_profile(&dsp, "Changed", "UTC", "OTHER", "TST2")
        .unwrap_err();
    assert_eq!(refused.code, "support_verification_dev_only");
    assert_eq!(db.find_dsp(&dsp)?.name, "Configured verification logistics");
    Ok(())
}
