use super::*;
#[test]
fn cooldown_and_interruption_survive_restart() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let path = dir.path().join("attempt.json");
    let mut attempts = Attempts::open(&path)?;
    assert!(!attempts.check(false)?);
    attempts.submitted()?;
    let mut attempts = Attempts::open(&path)?;
    assert!(attempts.check(true)?);
    attempts.succeeded()?;
    attempts.submitted()?;
    attempts.failed("primary_credentials_rejected")?;
    assert_eq!(
        Attempts::open(&path)?.check(true).unwrap_err().code,
        "attempt_cooldown"
    );
    Ok(())
}
#[test]
fn invalid_and_duplicate_state_is_rejected() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    let path = dir.path().join("attempt.json");
    for bytes in [br#"{"failures":4,"blocked_until":0,"pending":false,"manual":false,"recover":false}"#.as_slice(),
        br#"{"failures":0,"failures":0,"blocked_until":0,"pending":false,"manual":false,"recover":false}"#] {
        db::write_private(&path,bytes)?;
        assert!(Attempts::open(&path).is_err());
    }
    Ok(())
}
