//! Exact-input build cache for the executables the package ships, the backend and the support
//! worker, and for the browser tests' assessment fixture. Schema 4 excluded binaries built before
//! path remapping; schema 6 keeps test code out of the key; schema 7 adds the worker to the
//! backend's entry and the CLI library it links to the key, so no entry of the backend alone is
//! used.
mod key;
mod store;
#[cfg(test)]
mod tests;
use crate::{Result, Runner, require};
pub use key::{Environment, eligible, fingerprint, frontend, key};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn ci_enabled(root: &Path, env: &Environment) -> bool {
    env.get("CI").is_some_and(|v| v == "true")
        && env
            .get("DISPATCH_CI_RUST_CACHE")
            .is_some_and(|v| Path::new(v) == root.join(".ci-rust-cache"))
}
/// What a build makes: the name its cache key and messages use, Cargo's arguments, and where
/// Cargo puts each executable in the target folder. One cache entry holds them all.
pub struct Artifact {
    profile: &'static str,
    name: &'static str,
    args: &'static [&'static str],
    paths: &'static [&'static str],
}
/// The package's two executables in one Cargo pass: their dependencies resolve and compile once
/// for both, with only the features these two ask for.
pub const RELEASE: Artifact = Artifact {
    profile: "release",
    name: "release backend and support worker",
    args: &[
        "cargo",
        "build",
        "--locked",
        "--release",
        "-p",
        "dispatch-backend",
        "-p",
        "dispatch-support-worker",
    ],
    paths: &[
        "release/dispatch-backend",
        "release/dispatch-support-worker",
    ],
};
pub const DEBUG: Artifact = Artifact {
    profile: "debug",
    name: "debug backend and support worker",
    args: &[
        "cargo",
        "build",
        "--locked",
        "-p",
        "dispatch-backend",
        "-p",
        "dispatch-support-worker",
    ],
    paths: &["debug/dispatch-backend", "debug/dispatch-support-worker"],
};
/// The Timecard assessment companion some browser tests run, built like the debug backend.
pub const FIXTURE: Artifact = Artifact {
    profile: "assessment-fixture",
    name: "assessment fixture",
    args: &[
        "cargo",
        "build",
        "--locked",
        "--example",
        "assessment-fixture",
    ],
    paths: &["debug/examples/assessment-fixture"],
};
/// Where `artifact`'s executables are once built in the target folder `target`.
pub fn built(target: &Path, artifact: &Artifact) -> Vec<PathBuf> {
    artifact
        .paths
        .iter()
        .map(|path| target.join(path))
        .collect()
}
/// Where a debug build of `root` goes, and the environment its key reads. A test run's own build
/// folder inside the checkout (`crate::test::Run`) changes where Cargo builds, not what, so its
/// builds share entries with the checkout's own target folder. Elsewhere, it stays in the key and
/// keeps the build out of the cache.
fn located(root: &Path, env: &Environment) -> (PathBuf, Environment) {
    if let Some(target) = env.get("CARGO_TARGET_DIR").map(|dir| root.join(dir))
        && target.starts_with(root)
        && !target
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        let mut keyed = env.clone();
        keyed.remove("CARGO_TARGET_DIR");
        keyed.remove("CARGO_INCREMENTAL");
        return (target, keyed);
    }
    (root.join("target"), env.clone())
}
pub fn build(
    root: &Path,
    artifact: &Artifact,
    env: &Environment,
    runner: &dyn Runner,
) -> Result<()> {
    build_with_limit(root, artifact, env, runner, 12)
}
fn build_with_limit(
    root: &Path,
    artifact: &Artifact,
    env: &Environment,
    runner: &dyn Runner,
    keep: usize,
) -> Result<()> {
    let (profile, name, args) = (artifact.profile, artifact.name, artifact.args);
    let release = profile == "release";
    let ci = release && ci_enabled(root, env);
    let (folder, env) = if release {
        (root.join("target"), env.clone())
    } else {
        located(root, env)
    };
    let env = &env;
    if !eligible(root, env, ci)? {
        println!("Building {name} with Cargo.");
        runner.command(args, Some(root), 1200)?;
        return Ok(());
    }
    let compiler = key::compiler(root, runner)?;
    let key = fingerprint(root, profile, &compiler, env)?;
    require(
        !ci || env
            .get("DISPATCH_CI_RUST_KEY")
            .is_none_or(|expected| expected == &key),
        "CI Rust inputs changed after cache selection",
    )?;
    let cache = if ci {
        root.join(".ci-rust-cache")
    } else {
        let common = runner.command(
            &[
                "git",
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ],
            Some(root),
            30,
        )?;
        Path::new(std::str::from_utf8(&common)?.trim()).join("dispatch-rust-builds")
    };
    store::directory(&cache)?;
    let entry = cache.join(&key);
    let targets = built(&folder, artifact);
    let _lock = store::Lock::acquire(&cache, &key, true)?.ok_or("Cache lock unavailable")?;
    if let Some(binaries) = store::cached_binaries(&entry, &targets) {
        for (binary, target) in binaries.iter().zip(&targets) {
            store::copy_binary(binary, target)?;
        }
        println!("Reused {name} for identical Rust inputs.");
    } else {
        println!("Building {name} with Cargo.");
        runner.command(args, Some(root), 1200)?;
        require(
            fingerprint(root, profile, &compiler, env)? == key,
            "Rust inputs changed during the build; run it again before packaging",
        )?;
        store::save(&entry, &targets)?;
        println!("Cached {name} for identical inputs.");
    }
    if !ci {
        fs::File::open(&entry)?
            .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()))?;
        store::prune(&cache, keep, &key)?;
    }
    Ok(())
}
