use super::environment::Tools;
use crate::{Result, Runner, build::Environment, require, slot::Slot, to_hex, workspace};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Variant {
    pub module: String,
    pub variant: String,
    flavors: Vec<String>,
    task: String,
    report: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Apk {
    pub application_id: String,
    pub variant: String,
    apks: Vec<PathBuf>,
}

pub(super) fn apps(
    root: &Path,
    tools: &Tools,
    env: &Environment,
    runner: &dyn Runner,
) -> Result<Vec<Variant>> {
    tools.java(runner)?;
    let _slot = Slot::take(root, "Android metadata", env, runner)?;
    tools.gradle(root, &["dispatchdevApps"], runner)?;
    let mut apps: Vec<Variant> =
        serde_json::from_slice(&fs::read(root.join("android/build/dispatchdev/apps.json"))?)?;
    apps.sort_by(|a, b| (&a.module, &a.variant).cmp(&(&b.module, &b.variant)));
    Ok(apps)
}

pub(super) fn select<'a>(
    apps: &'a [Variant],
    app: &str,
    variant: Option<&str>,
) -> Result<&'a Variant> {
    let module = format!(":{}", app.trim_start_matches(':'));
    let matches: Vec<_> = apps.iter().filter(|entry| entry.module == module).collect();
    require(
        !matches.is_empty(),
        &format!(
            "{app} is not an application with a debuggable variant. Run dispatchdev android apps."
        ),
    )?;
    let choices = matches
        .iter()
        .map(|entry| entry.variant.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(variant) = variant {
        return matches
            .into_iter()
            .find(|entry| entry.variant == variant)
            .ok_or_else(|| {
                format!("No variant {variant} for {app}. Choose --variant from: {choices}.").into()
            });
    }
    let development: Vec<_> = matches
        .iter()
        .filter(|entry| entry.flavors.iter().any(|flavor| flavor == "dev"))
        .copied()
        .collect();
    match (development.as_slice(), matches.as_slice()) {
        ([one], _) | ([], [one]) => Ok(one),
        _ => Err(format!("Choose --variant for {app}: {choices}.").into()),
    }
}

pub(super) fn build(
    root: &Path,
    variant: &Variant,
    tools: &Tools,
    env: &Environment,
    runner: &dyn Runner,
) -> Result<(Apk, PathBuf)> {
    let _slot = Slot::take(root, "Android build", env, runner)?;
    eprintln!(
        "Building {} {} incrementally…",
        variant.module, variant.variant
    );
    let output = tools.gradle(root, &[&variant.task], runner)?;
    require(
        !String::from_utf8_lossy(&output)
            .lines()
            .any(|line| line.starts_with("w:")),
        "Android build emitted Kotlin compiler warnings.",
    )?;
    let built: Apk = serde_json::from_slice(&fs::read(&variant.report)?)?;
    require(
        built.variant == variant.variant,
        "The APK report names a different variant.",
    )?;
    require(
        !built.application_id.is_empty(),
        "The APK report has no application ID.",
    )?;
    let [apk] = built.apks.as_slice() else {
        return Err(
            "The variant produces multiple or no APKs. Choose a variant with one installable APK."
                .into(),
        );
    };
    let apk = fs::canonicalize(apk)?;
    require(
        apk.starts_with(fs::canonicalize(root.join("android"))?) && apk.is_file(),
        "The APK must be a file in this checkout's Android build.",
    )?;
    Ok((built, apk))
}

#[derive(PartialEq, Eq, Serialize)]
pub(super) struct Source {
    commit: String,
    dirty: bool,
    android_sha256: String,
}

pub(super) fn source(root: &Path, runner: &dyn Runner) -> Result<Source> {
    let commit = workspace::text(runner, &["git", "rev-parse", "HEAD"], Some(root))?;
    let dirty = !workspace::text(runner, &["git", "status", "--porcelain"], Some(root))?.is_empty();
    let paths = runner.command(
        &[
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "android",
        ],
        Some(root),
        30,
    )?;
    let mut paths: Vec<_> = paths
        .split(|b| *b == 0)
        .filter(|path| !path.is_empty())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let mut hash = Sha256::new();
    for path in paths {
        hash.update(path);
        hash.update([0]);
        let path = root.join(std::str::from_utf8(path)?);
        match fs::read(&path) {
            Ok(bytes) => {
                hash.update([1]);
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                hash.update([0]);
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Source {
        commit,
        dirty,
        android_sha256: to_hex(&hash.finalize()),
    })
}
