//! DSP Dispatch: Relay's APKs, packed into the runtime the build makes. CI builds and signs them
//! (`.github/workflows/checks.yml`) and names their folder in `DISPATCH_RELAY_APPS`: each build's
//! entry, `dev.json` or `production.json`, beside its APK. Each APK is checked against its entry
//! and packed with `relay.json`, the entries together, in the runtime's `dashboard/relay/`, where
//! the server reads them. A checkout is the debug runtime's root, as the package is the release's;
//! `npm run build` copies the folder into the package. Without the folder, no app is packed, and
//! none is offered.
use crate::{Result, hex, require};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// The variable naming the folder of the builds CI made.
pub const APPS: &str = "DISPATCH_RELAY_APPS";
/// Where the APKs and their file are packed, under the runtime's root.
pub const PACKED: &str = "dashboard/relay";
/// The file naming each build packed, beside the APKs.
pub const FILE: &str = "relay.json";
/// The largest APK taken.
const MOST: u64 = 200 * 1024 * 1024;
/// The highest `versionCode` Android takes.
const VERSION_CODE_MOST: u32 = 2_100_000_000;
/// The longest version a phone may say it runs (the phones' protocol's `VERSION_MOST`).
const VERSION_MOST: usize = 40;

/// The builds packed: Dev's (`com.dspdispatch.relay.dev`), which a preview server hands out, and
/// Production's (`com.dspdispatch.relay`), which only a release's package carries.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<Build>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production: Option<Build>,
}
/// One build: its version and `versionCode`, the commit it was built from, and its APK's file
/// name, SHA-256 in lowercase hex and size in bytes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    pub version: String,
    pub version_code: u32,
    pub commit: String,
    pub asset: String,
    pub sha256: String,
    pub size: u64,
}

impl Build {
    /// `name`'s entry, `text`, as a build, refused unless every field is as the format has it: a
    /// Production version is three numbers, as 1.4.0, and a Dev one names the next of them and
    /// its `versionCode`, as 1.4.1-dev.812.
    pub fn parse(name: &str, text: &str) -> Result<Self> {
        let build: Self = serde_json::from_str(text)
            .map_err(|error| format!("{name}.json is not a build: {error}"))?;
        let version = match name {
            "production" => release(&build.version),
            _ => build
                .version
                .split_once("-dev.")
                .is_some_and(|(next, count)| {
                    release(next) && count == build.version_code.to_string()
                }),
        };
        require(
            version && build.version.len() <= VERSION_MOST,
            &format!("{name}.json: version is as 1.4.0, or 1.4.1-dev.812 for Dev's"),
        )?;
        require(
            (1..=VERSION_CODE_MOST).contains(&build.version_code),
            &format!("{name}.json: versionCode is 1 to 2,100,000,000"),
        )?;
        require(
            hex(&build.commit, 40),
            &format!("{name}.json: commit is 40 lowercase hex digits"),
        )?;
        require(
            file_name(&build.asset),
            &format!("{name}.json: asset is a file name ending .apk"),
        )?;
        require(
            hex(&build.sha256, 64),
            &format!("{name}.json: sha256 is 64 lowercase hex digits"),
        )?;
        require(
            (1..=MOST).contains(&build.size),
            &format!("{name}.json: size is 1 byte to 200 MiB"),
        )?;
        Ok(build)
    }
}
/// Whether `version` is three numbers, as 1.4.0, without leading zeros.
fn release(version: &str) -> bool {
    let numbers: Vec<_> = version.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && (n == &"0" || !n.starts_with('0'))
        })
}
/// Whether `name` is a plain file name of an APK: letters, digits, `.`, `_` and `-`.
fn file_name(name: &str) -> bool {
    name.len() <= 100
        && name.ends_with(".apk")
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Packs the builds in `apps`, the folder CI made, in `root`'s `dashboard/relay/`; without it,
/// leaves none packed. Refused, and nothing packed, unless each APK is the one its entry names.
pub fn pack(root: &Path, apps: Option<&Path>) -> Result<()> {
    let packed = root.join(PACKED);
    // One build at a time packs a checkout, as its `predev` and `pretest` may at once: the next
    // finds the app packed, never a folder another is replacing.
    let _packing = packing(&packed)?;
    gone(&packed)?;
    let Some(apps) = apps else {
        return Ok(());
    };
    let parent = packed.parent().ok_or("Packed folder parent required")?;
    let staged = tempfile::Builder::new()
        .prefix(".relay-")
        .tempdir_in(parent)?;
    let mut packing = Apps::default();
    for name in ["dev", "production"] {
        let build = match fs::read_to_string(apps.join(format!("{name}.json"))) {
            Ok(text) => Build::parse(name, &text)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        require(
            packing
                .dev
                .as_ref()
                .is_none_or(|dev| dev.asset != build.asset),
            "Each build has an APK of its own",
        )?;
        // The copy, no longer than its entry says, is what's checked, so what's packed is what
        // its entry names.
        let to = staged.path().join(&build.asset);
        std::io::copy(
            &mut opened(&apps.join(&build.asset))?.take(build.size),
            &mut File::create(&to)?,
        )?;
        require(
            alike(&mut opened(&to)?, &build),
            &format!("{} differs from {name}.json", build.asset),
        )?;
        match name {
            "dev" => packing.dev = Some(build),
            _ => packing.production = Some(build),
        }
    }
    require(
        packing.dev.is_some() || packing.production.is_some(),
        &format!("{} holds no build of DSP Dispatch: Relay", apps.display()),
    )?;
    fs::write(
        staged.path().join(FILE),
        format!("{}\n", serde_json::to_string_pretty(&packing)?),
    )?;
    fs::rename(staged.keep(), &packed)?;
    let versions: Vec<_> = [&packing.dev, &packing.production]
        .into_iter()
        .flatten()
        .map(|build| build.version.as_str())
        .collect();
    println!(
        "Packed DSP Dispatch: Relay {} in {PACKED}.",
        versions.join(" and ")
    );
    Ok(())
}
/// The checkout's packing lock, held until it's dropped: beside `packed`, which it outlives.
fn packing(packed: &Path) -> Result<File> {
    let parent = packed.parent().ok_or("Packed folder parent required")?;
    fs::create_dir_all(parent)?;
    let lock = File::create(parent.join(".relay.lock"))?;
    lock.lock()?;
    Ok(lock)
}
/// The file at `path`, open to read, unless it's a symbolic link.
fn opened(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}
/// Whether `file` is the APK `build` names: a regular file of its size and SHA-256.
fn alike(file: &mut File, build: &Build) -> bool {
    file.metadata()
        .is_ok_and(|info| info.is_file() && info.len() == build.size)
        && file.rewind().is_ok()
        && super::store::digest(file).is_ok_and(|hash| hash == build.sha256)
}
/// Removes what's packed in `packed`, if anything.
fn gone(packed: &Path) -> Result<()> {
    match fs::remove_dir_all(packed) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
