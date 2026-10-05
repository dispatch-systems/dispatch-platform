use super::*;
use std::os::unix::fs::symlink;
#[test]
fn a_scrub_removes_every_page_trace_and_keeps_what_signing_in_needs() {
    use std::os::unix::fs::PermissionsExt;
    let profile = tempfile::tempdir().unwrap();
    let default = profile.path().join("Default");
    fs::create_dir_all(default.join("Cache/Cache_Data")).unwrap();
    fs::create_dir_all(default.join("Service Worker/CacheStorage")).unwrap();
    for file in ["History", "Web Data", "Cookies", "Preferences"] {
        fs::write(default.join(file), b"x").unwrap();
    }
    fs::create_dir_all(default.join("Local Storage")).unwrap();
    // A trace it cannot remove is reported, and every other trace still goes.
    let locked = default.join("Service Worker/CacheStorage");
    fs::write(locked.join("entry"), b"x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
    assert!(scrub(profile.path()).is_err());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    for gone in ["Cache", "History", "Web Data"] {
        assert!(!default.join(gone).exists(), "{gone}");
    }
    for kept in ["Cookies", "Preferences", "Local Storage"] {
        assert!(default.join(kept).exists(), "{kept}");
    }
    assert!(scrub(profile.path()).is_ok());
    assert!(!default.join("Service Worker").exists());
}
#[test]
fn a_scrub_never_follows_ancestor_links_outside_the_profile() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let outside = root.path().join("outside");
    fs::create_dir_all(&profile).unwrap();
    fs::create_dir_all(outside.join("Cache")).unwrap();
    fs::write(outside.join("Cache/sentinel"), b"outside").unwrap();
    symlink(&outside, profile.join("Default")).unwrap();
    fs::write(profile.join("BrowserMetrics"), b"inside").unwrap();

    assert!(scrub(&profile).is_err());
    assert_eq!(
        fs::read(outside.join("Cache/sentinel")).unwrap(),
        b"outside"
    );
    assert!(
        fs::symlink_metadata(profile.join("Default"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!profile.join("BrowserMetrics").exists());
}
#[test]
fn a_scrub_rejects_relative_dangling_and_nested_ancestor_links() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let outside = root.path().join("outside");
    fs::create_dir_all(profile.join("Default")).unwrap();
    fs::create_dir_all(profile.join("config")).unwrap();
    fs::create_dir_all(outside.join("Crash Reports")).unwrap();
    fs::write(outside.join("Crash Reports/sentinel"), b"outside").unwrap();
    symlink("../../outside", profile.join("config/browser-os")).unwrap();

    assert!(scrub(&profile).is_err());
    assert_eq!(
        fs::read(outside.join("Crash Reports/sentinel")).unwrap(),
        b"outside"
    );
    fs::remove_file(profile.join("config/browser-os")).unwrap();
    symlink(
        root.path().join("missing"),
        profile.join("config/browser-os"),
    )
    .unwrap();
    assert!(scrub(&profile).is_err());
    assert!(
        fs::symlink_metadata(profile.join("config/browser-os"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
#[test]
fn a_scrub_unlinks_a_final_link_without_touching_its_target() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let outside = root.path().join("outside");
    fs::create_dir_all(profile.join("Default")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("history"), b"outside").unwrap();
    symlink(outside.join("history"), profile.join("Default/History")).unwrap();

    assert!(scrub(&profile).is_ok());
    assert!(!profile.join("Default/History").exists());
    assert_eq!(fs::read(outside.join("history")).unwrap(), b"outside");
}
#[test]
fn a_scrub_refuses_non_directory_ancestors_and_a_linked_profile_root() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("Default"), b"not a directory").unwrap();
    fs::write(profile.join("BrowserMetrics"), b"safe trace").unwrap();

    assert!(scrub(&profile).is_err());
    assert_eq!(
        fs::read(profile.join("Default")).unwrap(),
        b"not a directory"
    );
    assert!(!profile.join("BrowserMetrics").exists());

    fs::remove_file(profile.join("Default")).unwrap();
    fs::write(profile.join("BrowserMetrics"), b"outside").unwrap();
    let linked = root.path().join("linked-profile");
    symlink(&profile, &linked).unwrap();
    assert!(scrub(&linked).is_err());
    assert_eq!(
        fs::read(profile.join("BrowserMetrics")).unwrap(),
        b"outside"
    );
}
#[test]
fn a_failed_start_names_the_workers_own_reason() {
    assert_eq!(
        start_failure(Some(&json!({"error":"browser_display_failed"})), ""),
        "browser_display_failed"
    );
    assert_eq!(start_failure(None, ""), "worker exited before reporting");
    // When the worker never reports, the sandbox's own complaint explains why.
    assert_eq!(
        start_failure(None, "bwrap: setting up uid map: Permission denied"),
        "worker exited before reporting: bwrap: setting up uid map: Permission denied"
    );
    // A worker that did report is the authority; the sandbox's echo of it adds nothing.
    assert_eq!(
        start_failure(
            Some(&json!({"error":"browser_lost"})),
            "core.failed browser_lost"
        ),
        "browser_lost"
    );
    // An unexpected frame is kept, bounded, rather than replaced by a generic failure.
    assert_eq!(
        start_failure(Some(&json!({"ready":false})), ""),
        "unexpected frame {\"ready\":false}"
    );
    assert_eq!(
        start_failure(
            Some(&json!({"error":"browser_lost","detail":"bwrap: no permission"})),
            ""
        ),
        "browser_lost: bwrap: no permission"
    );
    assert_eq!(
        start_failure(Some(&json!({"error":""})), ""),
        "unexpected frame {\"error\":\"\"}"
    );
    assert_eq!(
        start_failure(Some(&json!({"error":"x".repeat(400)})), "").len(),
        400
    );
    assert!(start_failure(Some(&json!({"note":"y".repeat(400)})), "").len() <= 200);
}
