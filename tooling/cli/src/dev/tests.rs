use super::*;
use crate::Native;
use serde_json::json;

fn checkout() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".gitignore"), ".runtime/\n").unwrap();
    fs::write(temp.path().join("source.rs"), "first").unwrap();
    for args in [
        vec!["git", "init", "-qb", "main"],
        vec!["git", "add", "."],
        vec![
            "git",
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        Native.command(&args, Some(temp.path()), 30).unwrap();
    }
    temp
}
fn receipt(root: &Path) -> serde_json::Value {
    let pid = std::process::id().to_string();
    let identity = workspace::text(
        &Native,
        &[
            "env", "LC_ALL=C", "ps", "-ww", "-p", &pid, "-o", "lstart=", "-o", "args=",
        ],
        Some(root),
    )
    .unwrap();
    let words: Vec<_> = identity.split_whitespace().collect();
    json!({
        "version": 1, "root": fs::canonicalize(root).unwrap(),
        "pid": std::process::id(),
        "process": { "pid": std::process::id(), "started": words[..5].join(" "), "command": words[5..].join(" ") },
        "source": source(root, &Native).unwrap(),
        "origin": "http://127.0.0.1:4321", "link": "http://127.0.0.1:4321/__preview/private-token", "password": "private-password",
    })
}
fn write(root: &Path, receipt: &serde_json::Value) {
    workspace::write_private(&root.join(api::RECORD), &receipt.to_string()).unwrap();
}

#[test]
fn status_preserves_startup_provenance_and_never_serializes_credentials() {
    let fixture = checkout();
    let root = fixture.path();
    let missing = status(root, &Native).unwrap();
    assert!(!missing.running);
    assert!(!root.join(".runtime").exists());
    let recorded = receipt(root);
    write(root, &recorded);
    let running = status(root, &Native).unwrap();
    assert!(running.running);
    assert_eq!(running.source_matches, Some(true));
    assert!(running.link.as_deref().unwrap().contains("private-token"));
    let public = serde_json::to_string(&running).unwrap();
    for secret in [
        "private-token",
        "private-password",
        "\"link\"",
        "\"password\"",
        "\"command\"",
    ] {
        assert!(!public.contains(secret), "{public}");
    }
    // Its pages' code is served as it is saved, and a commit changes no file: neither makes
    // the server another's.
    fs::create_dir_all(root.join("features/dvic/frontend")).unwrap();
    fs::write(root.join("features/dvic/frontend/Week.tsx"), "page").unwrap();
    assert_eq!(status(root, &Native).unwrap().source_matches, Some(true));
    for args in [
        vec!["git", "add", "."],
        vec![
            "git",
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "a page",
        ],
    ] {
        Native.command(&args, Some(root), 30).unwrap();
    }
    assert_eq!(status(root, &Native).unwrap().source_matches, Some(true));
    fs::write(root.join("source.rs"), "second").unwrap();
    let changed = status(root, &Native).unwrap();
    assert!(changed.running);
    assert_eq!(changed.source_matches, Some(false));
    assert_eq!(changed.source, running.source);
    assert_ne!(changed.checkout.fingerprint, running.checkout.fingerprint);
}

#[test]
fn stale_foreign_legacy_and_public_receipts_never_expose_a_link() {
    let fixture = checkout();
    let root = fixture.path();
    let original = receipt(root);
    for bad in [
        json!({ "pid": std::process::id(), "origin": original["origin"], "link": original["link"], "password": "private-password" }),
        {
            let mut bad = original.clone();
            bad["root"] = json!("/some-other-checkout");
            bad
        },
        {
            let mut bad = original.clone();
            bad["process"]["started"] = json!("earlier process");
            bad
        },
        {
            let mut bad = original.clone();
            bad["process"]["pid"] = json!(0);
            bad
        },
        {
            let mut bad = original.clone();
            bad["origin"] = json!("https://admin.dspdispatch.com");
            bad
        },
    ] {
        write(root, &bad);
        let state = status(root, &Native).unwrap();
        assert!(!state.running);
        assert!(state.link.is_none() && state.origin.is_none());
    }
    let mut unknown = original.clone();
    unknown["source"] = serde_json::Value::Null;
    write(root, &unknown);
    let state = status(root, &Native).unwrap();
    assert!(state.running);
    assert_eq!(state.source_matches, None);
    assert!(state.source.is_none());
    write(root, &original);
    fs::set_permissions(root.join(api::RECORD), fs::Permissions::from_mode(0o644)).unwrap();
    let state = status(root, &Native).unwrap();
    assert!(!state.running && state.link.is_none());
}
