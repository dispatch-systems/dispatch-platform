//! DSP Dispatch: Relay's APKs, packed into the runtime the build makes. `android/relay/release.json`
//! names the app's version and each build's APK, as the MacBook published them to the GitHub
//! pre-release `relay-v<version>`: each is fetched from there once, checked against its SHA-256 and
//! size, kept by its SHA-256 beside the repository's shared Git data, and copied with the file to
//! `dashboard/relay/`, where the server reads them. A checkout is the debug runtime's root, as the
//! package is the release's; `npm run build` copies the folder into the package. Without the file,
//! no app is packed, and none is offered.
use super::store;
use crate::{REPOSITORY, Result, Runner, hex, require};
use serde::Deserialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Seek,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// The file naming the app's version and its builds, in the repository.
pub const RELEASE: &str = "android/relay/release.json";
/// Where the APKs and that file are packed, under the runtime's root.
pub const PACKED: &str = "dashboard/relay";
/// The APKs fetched, by their SHA-256, in the repository's shared Git folder.
const CACHE: &str = "dispatch-relay-apks";
/// How many APKs the cache keeps: those of this version and the one before, for each build.
const KEEP: usize = 4;
/// The largest APK taken.
const MOST: u64 = 200 * 1024 * 1024;
/// The longest a download may take, in seconds.
const DOWNLOAD_SECONDS: u64 = 600;

/// `android/relay/release.json`: the app's version, the `versionCode` Android compares, and each
/// build's APK as published.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub version_code: u32,
    pub builds: Builds,
}
/// Dev's build (`com.dspdispatch.relay.dev`), which a preview server hands out, and Production's
/// (`com.dspdispatch.relay`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Builds {
    pub dev: Apk,
    pub production: Apk,
}
/// One build's APK: its asset's name on the pre-release, its SHA-256 in lowercase hex, and its
/// size in bytes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apk {
    pub asset: String,
    pub sha256: String,
    pub size: u64,
}

impl Release {
    /// The file's text as a release, refused unless every field is as the format has it.
    pub fn parse(text: &str) -> Result<Self> {
        let release: Self = serde_json::from_str(text)
            .map_err(|error| format!("{RELEASE} is not a release: {error}"))?;
        let numbers: Vec<_> = release.version.split('.').collect();
        require(
            numbers.len() == 3
                && numbers
                    .iter()
                    .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            &format!("{RELEASE}: version is three numbers, as 1.4.0"),
        )?;
        require(
            release.version_code > 0,
            &format!("{RELEASE}: versionCode is 1 or more"),
        )?;
        for apk in release.apks() {
            require(
                file_name(&apk.asset),
                &format!("{RELEASE}: asset is a file name ending .apk"),
            )?;
            require(
                hex(&apk.sha256, 64),
                &format!("{RELEASE}: sha256 is 64 lowercase hex digits"),
            )?;
            require(
                (1..=MOST).contains(&apk.size),
                &format!("{RELEASE}: size is 1 byte to 200 MiB"),
            )?;
        }
        require(
            release.builds.dev.asset != release.builds.production.asset,
            &format!("{RELEASE}: each build has an asset of its own"),
        )?;
        Ok(release)
    }
    fn apks(&self) -> [&Apk; 2] {
        [&self.builds.dev, &self.builds.production]
    }
    /// Where `apk` is downloaded from: its asset on the version's pre-release.
    fn url(&self, apk: &Apk) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/download/relay-v{}/{}",
            self.version, apk.asset
        )
    }
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

/// How curl says a download was over `--max-filesize`: its exit code 63.
const CURL_TOO_LARGE: &str = "curl: (63)";

/// An APK the build couldn't download, as offline: a debug build goes on without the app.
#[derive(Debug)]
struct Unavailable(String);
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unavailable {}

/// Packs the APKs the release names in `root`'s `dashboard/relay/`, fetching any not kept yet;
/// without the release, leaves none packed. A debug build that can't download one warns and packs
/// none; a release build fails, as both do on an APK or release unlike the format.
pub fn pack(root: &Path, runner: &dyn Runner, release: bool) -> Result<()> {
    let packed = root.join(PACKED);
    // One build at a time packs a checkout, as its `predev` and `pretest` may at once: the next
    // finds the app already packed, never a folder another is replacing.
    let _packing = packing(&packed)?;
    match fill(root, &packed, runner) {
        Err(error) if !release && error.is::<Unavailable>() => {
            eprintln!("Warning: {error}. Going on without DSP Dispatch: Relay.");
            gone(&packed)
        }
        done => done,
    }
}
/// The checkout's packing lock, held until it's dropped: beside `packed`, which it outlives.
fn packing(packed: &Path) -> Result<File> {
    let parent = packed.parent().ok_or("Packed folder parent required")?;
    fs::create_dir_all(parent)?;
    let lock = File::create(parent.join(".relay.lock"))?;
    lock.lock()?;
    Ok(lock)
}
fn fill(root: &Path, packed: &Path, runner: &dyn Runner) -> Result<()> {
    let text = match fs::read_to_string(root.join(RELEASE)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return gone(packed);
        }
        Err(error) => return Err(error.into()),
    };
    let release = Release::parse(&text)?;
    if holds(packed, &text, &release) {
        return Ok(());
    }
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
    let cache = Path::new(std::str::from_utf8(&common)?.trim()).join(CACHE);
    store::directory(&cache)?;
    // Each APK is held open, and locked shared, from the cache until it's copied: another
    // worktree's build pruning the cache passes it over, and one removed anyway still copies.
    let mut kept = Vec::new();
    for apk in release.apks() {
        kept.push(fetched(&cache, &release, apk, runner)?);
    }
    let parent = packed.parent().ok_or("Packed folder parent required")?;
    fs::create_dir_all(parent)?;
    let staged = tempfile::Builder::new()
        .prefix(".relay-")
        .tempdir_in(parent)?;
    for (apk, from) in release.apks().into_iter().zip(&mut kept) {
        from.rewind()?;
        std::io::copy(from, &mut File::create(staged.path().join(&apk.asset))?)?;
    }
    fs::write(staged.path().join("release.json"), &text)?;
    gone(packed)?;
    fs::rename(staged.keep(), packed)?;
    prune(&cache, &release)?;
    println!(
        "Packed DSP Dispatch: Relay {} in {PACKED}.",
        release.version
    );
    Ok(())
}
/// Whether `packed` holds exactly this release already: its file as written, and each APK as the
/// release names it.
fn holds(packed: &Path, text: &str, release: &Release) -> bool {
    let Ok(entries) = fs::read_dir(packed) else {
        return false;
    };
    entries.count() == 3
        && fs::read_to_string(packed.join("release.json")).is_ok_and(|kept| kept == text)
        && release
            .apks()
            .iter()
            .all(|apk| opened(&packed.join(&apk.asset)).is_ok_and(|mut file| alike(&mut file, apk)))
}
/// The file at `path`, open to read, unless it's a symbolic link.
fn opened(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}
/// Whether `file` is the APK the release names: a regular file of its size and SHA-256.
fn alike(file: &mut File, apk: &Apk) -> bool {
    file.metadata()
        .is_ok_and(|info| info.is_file() && info.len() == apk.size)
        && file.rewind().is_ok()
        && store::digest(file).is_ok_and(|hash| hash == apk.sha256)
}
/// Where the cache keeps `apk`.
fn entry(cache: &Path, apk: &Apk) -> PathBuf {
    cache.join(format!("{}.apk", apk.sha256))
}
/// The cache's copy of `apk`, held, downloaded from its pre-release unless the cache has it:
/// refused unless it's a regular file of the SHA-256 and size the release names.
fn fetched(cache: &Path, release: &Release, apk: &Apk, runner: &dyn Runner) -> Result<File> {
    let kept = entry(cache, apk);
    if let Ok(mut file) = opened(&kept) {
        file.lock_shared()?;
        if alike(&mut file, apk) {
            file.set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()))?;
            return Ok(file);
        }
    }
    let mut arriving = tempfile::Builder::new()
        .prefix(".arriving-")
        .tempfile_in(cache)?;
    arriving.as_file().lock_shared()?;
    let url = release.url(apk);
    println!(
        "Fetching DSP Dispatch: Relay {}'s {}.",
        release.version, apk.asset
    );
    runner
        .command(
            &[
                "curl",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--retry",
                "3",
                "--max-filesize",
                &MOST.to_string(),
                "--output",
                arriving.path().to_str().ok_or("Invalid cache path")?,
                &url,
            ],
            None,
            DOWNLOAD_SECONDS,
        )
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> {
            // An APK over the most a release may name is unlike its release, not unavailable.
            if error.to_string().contains(CURL_TOO_LARGE) {
                format!(
                    "{} of relay-v{} is larger than {RELEASE} allows",
                    apk.asset, release.version
                )
                .into()
            } else {
                Unavailable(format!("Couldn't download {url}: {error}")).into()
            }
        })?;
    require(
        alike(arriving.as_file_mut(), apk),
        &format!(
            "{} of relay-v{} differs from {RELEASE}",
            apk.asset, release.version
        ),
    )?;
    Ok(arriving.persist(&kept)?)
}
/// Removes what's packed in `packed`, if anything.
fn gone(packed: &Path) -> Result<()> {
    match fs::remove_dir_all(packed) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
/// Lets go of the cache's APKs but the release's and the most recently used, up to `KEEP`: never
/// one another build holds, and passing over one already gone.
fn prune(cache: &Path, release: &Release) -> Result<()> {
    let ours = release.apks().map(|apk| entry(cache, apk));
    let mut others = Vec::new();
    for found in fs::read_dir(cache)? {
        let path = found?.path();
        if path.extension().is_some_and(|e| e == "apk")
            && !ours.contains(&path)
            && let Ok(info) = path.symlink_metadata()
            && info.is_file()
        {
            others.push((info.modified()?, path));
        }
    }
    others.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in others.into_iter().skip(KEEP.saturating_sub(ours.len())) {
        let Ok(file) = opened(&path) else {
            continue;
        };
        // Held by another build, or replaced since it was opened: left alone.
        let held = file.metadata()?;
        if file.try_lock().is_err()
            || !fs::symlink_metadata(&path)
                .is_ok_and(|now| now.ino() == held.ino() && now.dev() == held.dev())
        {
            continue;
        }
        match fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
