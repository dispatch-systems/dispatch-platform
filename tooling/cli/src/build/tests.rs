use super::*;
use sha2::Digest;
use std::{
    cell::Cell,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};
struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    env: Environment,
    builds: Cell<usize>,
    change_during_build: Cell<bool>,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join("app/backend")).unwrap();
        fs::create_dir_all(root.join("tooling")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app/backend\"]\n",
        )
        .unwrap();
        fs::write(
            root.join("app/backend/Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .unwrap();
        fs::write(root.join("app/backend/main.rs"), "one").unwrap();
        let env = [(
            "CARGO_HOME".into(),
            temp.path().join("cargo-home").to_str().unwrap().into(),
        )]
        .into_iter()
        .collect();
        Self {
            temp,
            root,
            env,
            builds: Cell::new(0),
            change_during_build: Cell::new(false),
        }
    }
    fn build(&self) {
        build_with_limit(&self.root, &DEBUG, &self.env, self, 2).unwrap();
    }
    fn source(&self, text: &str) {
        fs::write(self.root.join("app/backend/main.rs"), text).unwrap();
    }
    fn store(&self) -> PathBuf {
        self.root.join(".git/dispatch-rust-builds")
    }
}
impl Runner for Fixture {
    fn command(&self, args: &[&str], cwd: Option<&Path>, _timeout: u64) -> Result<Vec<u8>> {
        assert_eq!(cwd, Some(self.root.as_path()));
        match args[0] {
            "rustc" | "cc" | "ld" => Ok(format!("pinned {}", args[0]).into_bytes()),
            "git" => Ok(self.root.join(".git").to_str().unwrap().as_bytes().to_vec()),
            "cargo" => {
                self.builds.set(self.builds.get() + 1);
                let target = self
                    .root
                    .join("target")
                    .join(if args.contains(&"--release") {
                        "release/dispatch-backend"
                    } else if args.contains(&"--example") {
                        "debug/examples/assessment-fixture"
                    } else {
                        "debug/dispatch-backend"
                    });
                fs::create_dir_all(target.parent().unwrap())?;
                let mut built = fs::read(self.root.join("app/backend/main.rs"))?;
                if args.contains(&"--example") {
                    built.extend(b" fixture");
                }
                fs::write(&target, built)?;
                if self.change_during_build.get() {
                    self.source("changed mid-build");
                }
                Ok(vec![])
            }
            _ => panic!("Unexpected command"),
        }
    }
}
#[test]
fn custom_build_inputs_and_untracked_external_sources_disable_reuse() {
    let f = Fixture::new();
    assert!(eligible(&f.root, &f.env, false).unwrap());
    for (key, value) in [
        ("CI", "true"),
        ("DISPATCH_DISABLE_RUST_CACHE", "1"),
        ("CARGO_TARGET_DIR", "/custom"),
        ("RUSTC_WRAPPER", "/wrapper"),
        ("CARGO_SOURCE_LOCAL_DIRECTORY", "/outside"),
        ("CARGO_BUILD_TARGET", "other"),
        ("RUSTFLAGS", "--cfg=custom"),
        ("RUSTFLAGS", ""),
        ("CARGO_ENCODED_RUSTFLAGS", "--cfg=custom"),
        ("CARGO_ENCODED_RUSTFLAGS", ""),
        (
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "--cfg=custom",
        ),
        ("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", ""),
        (
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "--cfg=custom",
        ),
        ("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS", ""),
    ] {
        let mut env = f.env.clone();
        env.insert(key.into(), value.into());
        assert!(!eligible(&f.root, &env, false).unwrap(), "{key}");
    }
    for name in ["app/backend/build.rs", "app/backend/linked"] {
        let path = f.root.join(name);
        if name.ends_with("linked") {
            symlink(f.temp.path(), &path).unwrap();
        } else {
            fs::write(&path, "build").unwrap();
        }
        assert!(!eligible(&f.root, &f.env, false).unwrap());
        fs::remove_file(path).unwrap();
    }
    fs::create_dir_all(f.root.join("ops/host-manager")).unwrap();
    fs::create_dir_all(f.root.join("tooling/shared")).unwrap();
    fs::create_dir_all(f.root.join("features/one")).unwrap();
    fs::write(
        f.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"app/backend\", \"ops/host-manager\", \"tooling/shared\", \"features/*\"]\n",
    )
    .unwrap();
    fs::write(
        f.root.join("app/backend/Cargo.toml"),
        "[dependencies]\nhost = { path = \"../../ops/host-manager\" }\none = { path = \"../../features/one\" }\n",
    )
    .unwrap();
    let manifest = f.root.join("ops/host-manager/Cargo.toml");
    fs::write(
        &manifest,
        "[dependencies]\nshared = { path = \"../../tooling/shared\" }\n",
    )
    .unwrap();
    assert!(eligible(&f.root, &f.env, false).unwrap());
    // A member outside the fingerprinted roots, or one that escapes them, disables reuse.
    for members in [
        "\"app/backend\", \"elsewhere\"",
        "\"app/../outside\"",
        "\"*\"",
    ] {
        fs::write(
            f.root.join("Cargo.toml"),
            format!("[workspace]\nmembers = [{members}]\n"),
        )
        .unwrap();
        assert!(!eligible(&f.root, &f.env, false).unwrap(), "{members}");
    }
    fs::write(
        f.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"app/backend\", \"ops/host-manager\", \"tooling/shared\"]\n",
    )
    .unwrap();
    assert!(eligible(&f.root, &f.env, false).unwrap());
    for text in [
        "[dependencies]\nexternal = { path = \"../../../outside\" }\n",
        "[package]\nbuild = \"elsewhere.rs\"\n",
    ] {
        fs::write(&manifest, text).unwrap();
        assert!(!eligible(&f.root, &f.env, false).unwrap());
    }
    fs::write(&manifest, "[package]\nbuild = false\n").unwrap();
    assert!(eligible(&f.root, &f.env, false).unwrap());
    let config = f.temp.path().join(".cargo/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config, "[build]\n").unwrap();
    assert!(!eligible(&f.root, &f.env, false).unwrap());
}
#[test]
fn every_rust_and_embedded_launcher_input_is_fingerprinted_but_dashboard_is_not() {
    let f = Fixture::new();
    let g = Fixture::new();
    let key = |f: &Fixture| {
        fingerprint(
            &f.root,
            "release",
            "compiler",
            &Environment::from([(
                "CARGO_HOME".into(),
                "/nonexistent-dispatch-test-home".into(),
            )]),
        )
        .unwrap()
    };
    assert_eq!(key(&f), key(&g));
    let original = key(&f);
    for name in [
        "dashboard/app.tsx",
        "features/one/frontend/Page.tsx",
        "features/one/frontend/assets/logo.png",
        "core/shell/frontend/runtime/api.ts",
        "features/one/tests/browser/page.spec.ts",
        "features/one/tests/support/fixture.ts",
        "features/one/frontend/page.css",
    ] {
        let path = f.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "ui").unwrap();
        assert_eq!(original, key(&f), "{name}");
    }
    for name in [
        "Cargo.lock",
        "rust-toolchain.toml",
        "app/backend/provider.js",
        "core/db/migrations/schema.sql",
        "ops/host-manager/src/management.rs",
        "tooling/cli/src/build/key.rs",
        "tooling/cli/dispatchdev",
        "tooling/shared/src/process.rs",
        "core/foundation/backend/error.rs",
        "collectors/cortex/scripts/meal.js",
        "features/dvic/migrations/dvic/0001_baseline.sql",
        "tooling/build/rustc-remap.py",
        "ops/launchers/runtime_artifact.py",
        "ops/launchers/update-dev.py",
        "ops/launchers/update-production.py",
    ] {
        let path = f.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "new input").unwrap();
        assert_ne!(original, key(&f), "{name}");
        fs::remove_file(path).unwrap();
    }
    assert_ne!(
        fingerprint(&f.root, "release", "compiler", &f.env).unwrap(),
        fingerprint(&f.root, "debug", "compiler", &f.env).unwrap()
    );
    let mut flags = f.env.clone();
    flags.insert("RUSTFLAGS".into(), "custom".into());
    assert_ne!(
        fingerprint(&f.root, "release", "compiler", &f.env).unwrap(),
        fingerprint(&f.root, "release", "compiler", &flags).unwrap()
    );
    assert_ne!(
        fingerprint(&f.root, "release", "compiler", &f.env).unwrap(),
        fingerprint(&f.root, "release", "new compiler", &f.env).unwrap()
    );
}

#[test]
fn a_checkout_inside_a_folder_named_like_frontend_code_still_fingerprints_its_rust() {
    let temp = tempfile::tempdir().unwrap();
    let env = Environment::from([(
        "CARGO_HOME".into(),
        "/nonexistent-dispatch-test-home".into(),
    )]);
    for parent in ["frontend", "node_modules"] {
        let root = temp.path().join(parent).join("repo");
        let source = root.join("app/backend/main.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "one").unwrap();
        let before = fingerprint(&root, "release", "compiler", &env).unwrap();
        fs::write(&source, "two").unwrap();
        assert_ne!(
            before,
            fingerprint(&root, "release", "compiler", &env).unwrap(),
            "{parent}"
        );
    }
}

#[test]
fn only_the_exact_fingerprinted_remap_config_allows_cache_reuse() {
    let f = Fixture::new();
    fs::create_dir(f.root.join(".cargo")).unwrap();
    let config = f.root.join(".cargo/config.toml");
    let wrapper = f.root.join("tooling/build/rustc-remap.py");
    fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    fs::write(&wrapper, "reviewed policy").unwrap();
    let hash = crate::to_hex(&sha2::Sha256::digest(b"reviewed policy"));
    let text = format!(
        "[build]\nrustc-wrapper = 'tooling/build/rustc-remap.py'\nrustflags = ['--cfg=dispatch_path_policy_{hash}']\n"
    );
    fs::write(&config, &text).unwrap();
    assert!(eligible(&f.root, &f.env, false).unwrap());
    let before = fingerprint(&f.root, "release", "compiler", &f.env).unwrap();
    fs::write(&wrapper, "changed policy").unwrap();
    assert!(!eligible(&f.root, &f.env, false).unwrap());
    assert_ne!(
        before,
        fingerprint(&f.root, "release", "compiler", &f.env).unwrap()
    );
    fs::write(&wrapper, "reviewed policy").unwrap();
    fs::write(&config, format!("{text}target = 'custom-target'\n")).unwrap();
    assert!(!eligible(&f.root, &f.env, false).unwrap());
    fs::write(&config, &text).unwrap();
    let elsewhere = f.temp.path().join("external-policy");
    fs::rename(&wrapper, &elsewhere).unwrap();
    symlink(&elsewhere, &wrapper).unwrap();
    assert!(!eligible(&f.root, &f.env, false).unwrap());
    fs::remove_file(&wrapper).unwrap();
    fs::rename(&elsewhere, &wrapper).unwrap();
    let external_tools = f.temp.path().join("external-tools");
    fs::rename(f.root.join("tooling"), &external_tools).unwrap();
    symlink(&external_tools, f.root.join("tooling")).unwrap();
    assert!(!eligible(&f.root, &f.env, false).unwrap());
}
#[test]
fn cache_reuses_intact_binaries_refreshes_recency_and_rebuilds_corruption() {
    let f = Fixture::new();
    f.build();
    f.source("two");
    f.build();
    f.source("one");
    f.build();
    assert_eq!(f.builds.get(), 2);
    f.source("three");
    f.build();
    f.source("one");
    f.build();
    assert_eq!(f.builds.get(), 3);
    f.source("two");
    f.build();
    assert_eq!(f.builds.get(), 4);
    let entries: Vec<_> = fs::read_dir(f.store())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert!(store::cached_binary(&entry).is_some());
        fs::write(entry.join("dispatch-backend"), "corrupt").unwrap();
    }
    f.build();
    assert_eq!(f.builds.get(), 5);
    assert_eq!(
        fs::read(f.root.join("target/debug/dispatch-backend")).unwrap(),
        b"two"
    );
    assert_eq!(
        fs::metadata(f.root.join("target/debug/dispatch-backend"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
#[test]
fn the_assessment_fixture_keeps_its_own_entry_beside_the_backend() {
    let f = Fixture::new();
    f.build();
    let fixture = || build_with_limit(&f.root, &FIXTURE, &f.env, &f, 4).unwrap();
    fixture();
    assert_eq!(
        f.builds.get(),
        2,
        "the backend's entry is not the fixture's"
    );
    assert_eq!(
        fs::read(built(&f.root.join("target"), &FIXTURE)).unwrap(),
        b"one fixture"
    );
    fs::remove_dir_all(f.root.join("target")).unwrap();
    fixture();
    f.build();
    assert_eq!(f.builds.get(), 2, "each is reused for the same inputs");
    assert_eq!(
        fs::read(built(&f.root.join("target"), &FIXTURE)).unwrap(),
        b"one fixture"
    );
    assert_eq!(
        fs::read(built(&f.root.join("target"), &DEBUG)).unwrap(),
        b"one"
    );
    f.source("two");
    fixture();
    assert_eq!(f.builds.get(), 3);
    assert_eq!(
        fs::read(built(&f.root.join("target"), &FIXTURE)).unwrap(),
        b"two fixture"
    );
}
#[test]
fn a_test_runs_build_folder_in_the_checkout_shares_debug_entries_but_not_release() {
    let f = Fixture::new();
    f.build();
    let mut run = f.env.clone();
    run.insert("CARGO_TARGET_DIR".into(), ".test-build/42".into());
    run.insert("CARGO_INCREMENTAL".into(), "0".into());
    let (folder, keyed) = located(&f.root, &run);
    assert_eq!(folder, f.root.join(".test-build/42"));
    assert_eq!(keyed, f.env, "where it builds is not what it builds");
    let mut outside = f.env.clone();
    outside.insert("CARGO_TARGET_DIR".into(), "/elsewhere".into());
    assert_eq!(
        located(&f.root, &outside).1,
        outside,
        "outside the checkout it stays keyed"
    );
    outside.insert("CARGO_TARGET_DIR".into(), "../elsewhere".into());
    assert_eq!(located(&f.root, &outside).1, outside);
    // Release builds keep a test run's folder out of the cache, as before.
    assert!(!eligible(&f.root, &run, false).unwrap());
}
#[test]
fn changing_inputs_during_build_cannot_publish_cache_entry() {
    let f = Fixture::new();
    f.change_during_build.set(true);
    assert!(build(&f.root, &DEBUG, &f.env, &f).is_err());
    assert!(
        !fs::read_dir(f.store())
            .unwrap()
            .any(|p| p.unwrap().path().is_dir())
    );
}
#[test]
fn cache_pruning_keeps_busy_entries_and_lock_identity_survives_deletion() {
    let f = Fixture::new();
    let cache = f.store();
    store::directory(&cache).unwrap();
    let keys: Vec<_> = (1..=5).map(|n| format!("{n:064x}")).collect();
    for (index, key) in keys.iter().enumerate() {
        let entry = cache.join(key);
        fs::create_dir(&entry).unwrap();
        fs::File::open(entry)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + Duration::from_secs(1000 - index as u64)),
            )
            .unwrap();
    }
    let busy = store::Lock::acquire(&cache, &keys[3], true)
        .unwrap()
        .unwrap();
    assert!(
        store::Lock::acquire(&cache, &keys[3], false)
            .unwrap()
            .is_none()
    );
    store::prune(&cache, 3, &keys[4]).unwrap();
    assert!(cache.join(&keys[3]).exists());
    assert!(!cache.join(&keys[2]).exists());
    drop(busy);
    store::prune(&cache, 3, &keys[4]).unwrap();
    assert!(!cache.join(&keys[3]).exists());
    let stale = store::Lock::acquire(&cache, &keys[0], true)
        .unwrap()
        .unwrap();
    fs::remove_file(cache.join(format!("{}.lock", keys[0]))).unwrap();
    assert!(
        store::Lock::acquire(&cache, &keys[0], false)
            .unwrap()
            .is_some()
    );
    drop(stale);
    store::prune(&cache, 0, &keys[4]).unwrap();
    assert!(cache.join(&keys[4]).exists());
    assert_eq!(
        fs::read_dir(cache)
            .unwrap()
            .filter(|p| p.as_ref().unwrap().path().is_dir())
            .count(),
        1
    );
}
#[test]
fn ci_cache_needs_explicit_location_and_current_receipt_key() {
    let mut f = Fixture::new();
    f.env.insert("CI".into(), "true".into());
    assert!(!eligible(&f.root, &f.env, false).unwrap());
    assert!(eligible(&f.root, &f.env, true).unwrap());
    assert!(!ci_enabled(&f.root, &f.env));
    f.env.insert(
        "DISPATCH_CI_RUST_CACHE".into(),
        f.root.join(".ci-rust-cache").to_str().unwrap().into(),
    );
    assert!(ci_enabled(&f.root, &f.env));
    build(&f.root, &RELEASE, &f.env, &f).unwrap();
    build(&f.root, &RELEASE, &f.env, &f).unwrap();
    assert_eq!(f.builds.get(), 1);
    f.env.insert("DISPATCH_CI_RUST_KEY".into(), "wrong".into());
    assert!(build(&f.root, &RELEASE, &f.env, &f).is_err());
    assert_eq!(f.builds.get(), 1);
}
#[test]
fn promoted_binary_can_only_seed_its_original_eligible_compiler_key() {
    let f = Fixture::new();
    let source = f.temp.path().join("verified");
    fs::write(&source, "verified backend").unwrap();
    let key = key(&f.root, "release", &f.env, &f).unwrap();
    seed(&f.root, &source, &"f".repeat(64), &f.env, &f).unwrap();
    assert!(!f.root.join(".ci-rust-cache").exists());
    let mut ineligible = f.env.clone();
    ineligible.insert("DISPATCH_DISABLE_RUST_CACHE".into(), "1".into());
    seed(&f.root, &source, &key, &ineligible, &f).unwrap();
    assert!(!f.root.join(".ci-rust-cache").exists());
    seed(&f.root, &source, &key, &f.env, &f).unwrap();
    let binary = store::cached_binary(&f.root.join(".ci-rust-cache").join(key)).unwrap();
    assert_eq!(fs::read(binary).unwrap(), b"verified backend");
}

#[test]
fn runner_image_builds_share_entries_but_the_toolchain_and_distribution_do_not() {
    let f = Fixture::new();
    let base = fingerprint(&f.root, "release", "compiler", &f.env).unwrap();
    // GitHub rolls a new image build out over days, with both builds serving jobs.
    let mut rolled = f.env.clone();
    rolled.insert("ImageVersion".into(), "20260920.314.1".into());
    assert_eq!(
        fingerprint(&f.root, "release", "compiler", &rolled).unwrap(),
        base
    );
    let mut distribution = f.env.clone();
    distribution.insert("ImageOS".into(), "ubuntu26".into());
    assert_ne!(
        fingerprint(&f.root, "release", "compiler", &distribution).unwrap(),
        base
    );
    // The toolchain is named by version, so a linker update changes the key directly.
    let toolchain = key::compiler(&f.root, &f).unwrap();
    assert_eq!(toolchain, "pinned rustc\npinned cc\npinned ld");
    assert_ne!(
        fingerprint(&f.root, "release", &toolchain, &f.env).unwrap(),
        fingerprint(
            &f.root,
            "release",
            "pinned rustc\npinned cc\npinned ld 2",
            &f.env
        )
        .unwrap()
    );
}
