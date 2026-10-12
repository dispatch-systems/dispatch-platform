//! Credential transfer, environment binding and destination privacy through the real command
//! entry point. The host stand-in only mints a fixture; the wrapper owns validation and storage.
use dispatchdev_cli::{Native, Result, Runner, observation, workspace::Workspace};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn credential(origin: &str) -> Value {
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 900_000;
    json!({
        "origin": origin,
        "token": "observation-read:fixtureClaims:fixtureSignature",
        "expiresAt": expires,
        "readOnly": true,
    })
}

fn private_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

struct Host {
    answer: Value,
    calls: AtomicUsize,
}
impl Host {
    fn new(answer: Value) -> Self {
        Self {
            answer,
            calls: AtomicUsize::new(0),
        }
    }
}
impl Runner for Host {
    fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(args[0], "ssh");
        assert!(args.contains(&"dispatch-production"));
        Ok(serde_json::to_vec(&self.answer)?)
    }
}

#[test]
fn dev_mints_using_installed_config_saves_private_access_and_cleans_host_temporary_files() {
    let root = private_root();
    let dev = root.path().join("dev");
    let active = dev.join(".build");
    fs::create_dir_all(active.join("services/rust")).unwrap();
    fs::create_dir(dev.join("config")).unwrap();
    fs::write(
        dev.join("config/platform.env"),
        "DISPATCH_ENVIRONMENT=preview\nDISPATCH_ORIGIN=https://dispatchdev.dillonlille.com\n\
         DISPATCH_TEST_SECRET=should-never-be-printed\necho \"$DISPATCH_TEST_SECRET\"\n",
    )
    .unwrap();
    let expected = credential("https://dispatchdev.dillonlille.com");
    let backend = active.join("services/rust/dispatch-backend");
    fs::write(
        &backend,
        format!(
            "#!/bin/bash\nset -eu\n\
             [ \"$1\" = observation-session ]\n\
             [ \"$DISPATCH_ARTIFACT_ROOT\" = \"$PWD\" ]\n\
             [ \"$DISPATCH_TEST_SECRET\" = should-never-be-printed ]\n\
             printf '%s' \"$2\" > minted-path\n\
             printf '%s' '{}' > \"$2\"\n\
             echo \"$DISPATCH_TEST_SECRET\"\n\
             echo \"$DISPATCH_TEST_SECRET\" >&2\n",
            expected
        ),
    )
    .unwrap();
    fs::set_permissions(&backend, fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.path().join("session.json");
    let summary = observation::run(
        &Workspace {
            root: root.path().into(),
        },
        "dev",
        &output,
        &Native,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap(),
        expected
    );
    let metadata = fs::metadata(&output).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(
        summary,
        json!({
            "origin": expected["origin"],
            "expiresAt": expected["expiresAt"],
            "readOnly": true,
        })
    );
    assert!(!summary.to_string().contains("fixtureClaims"));
    assert!(!summary.to_string().contains("should-never-be-printed"));
    let temporary = fs::read_to_string(active.join("minted-path")).unwrap();
    assert!(!Path::new(&temporary).exists());
    assert!(!Path::new(&temporary).parent().unwrap().exists());
}

#[test]
fn production_rejects_wrong_host_writable_or_expired_credentials_without_saving_them() {
    let root = private_root();
    let workspace = Workspace {
        root: root.path().into(),
    };
    let valid = credential("https://admin.dspdispatch.com");
    let output = root.path().join("session.json");
    let change = |key: &str, value: Value| {
        let mut changed = valid.clone();
        changed[key] = value;
        changed
    };
    for bad in [
        change("origin", json!("https://dispatchdev.dillonlille.com")),
        change("readOnly", json!(false)),
        change("expiresAt", json!(1)),
        change("expiresAt", json!(u64::MAX)),
        change("token", json!("normal-owner-session")),
        change(
            "token",
            json!("observation-read:fixtureClaims:fixtureSignature\nleak"),
        ),
        change("extra", json!("secret")),
    ] {
        let host = Host::new(bad);
        let error = observation::run(&workspace, "production", &output, &host)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("fixtureClaims"));
        assert!(!error.contains("fixtureSignature"));
        assert!(!output.exists());
    }
    let host = Host::new(valid.clone());
    observation::run(&workspace, "production", &output, &host).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap(),
        valid
    );
}

#[test]
fn unsafe_destinations_are_refused_before_minting_and_existing_files_are_preserved() {
    let root = private_root();
    let workspace = Workspace {
        root: root.path().into(),
    };
    let host = Host::new(credential("https://admin.dspdispatch.com"));
    let existing = root.path().join("existing.json");
    fs::write(&existing, "keep this").unwrap();
    let linked = root.path().join("linked.json");
    symlink(&existing, &linked).unwrap();
    let linked_parent = root.path().join("linked-parent");
    symlink(root.path(), &linked_parent).unwrap();
    let public = root.path().join("public");
    fs::create_dir(&public).unwrap();
    fs::set_permissions(&public, fs::Permissions::from_mode(0o755)).unwrap();
    for output in [
        Path::new("relative.json").to_path_buf(),
        existing.clone(),
        linked,
        linked_parent.join("session.json"),
        public.join("session.json"),
        root.path().join("missing/session.json"),
        root.path().join("public/../session.json"),
    ] {
        assert!(observation::run(&workspace, "production", &output, &host).is_err());
    }
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_to_string(existing).unwrap(), "keep this");
}

#[test]
fn failed_mints_hide_subprocess_diagnostics_and_do_not_save_credentials() {
    struct Failed;
    impl Runner for Failed {
        fn command(&self, _: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            Err("backend failed: sensitive-credential".into())
        }
    }
    let root = private_root();
    let output = root.path().join("session.json");
    let error = observation::run(
        &Workspace {
            root: root.path().into(),
        },
        "production",
        &output,
        &Failed,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Could not mint production browser access"));
    assert!(!error.contains("sensitive-credential"));
    assert!(!output.exists());
}
