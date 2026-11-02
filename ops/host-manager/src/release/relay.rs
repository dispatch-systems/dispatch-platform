//! DSP Dispatch: Relay in a release ([Android apps](../../../../android/README.md#releases)).
//! The last released app is the `production` entry of the `relay.json` GitHub's latest release
//! attaches: the release Production installs and the checks carry from, whatever the highest
//! version is. A release whose commit changed `android/` since then asks its checks for the
//! app's next version, which they build and sign; any other has them carry the last released
//! app unchanged. Once an app exists, each release attaches its package's `relay.json` and
//! Production's APK, which the next release's checks and main's Dev builds read.
use super::*;

/// The file naming the builds a package carries, in its `dashboard/relay/`, and as a release
/// attaches it.
const FILE: &str = "relay.json";
/// Where a package carries the app.
const PACKED: &str = "dashboard/relay";
/// The app's first version, before any is released.
const FIRST: &str = "0.0.1";
/// The largest `relay.json` taken from a release.
const FILE_MOST: u64 = 64 * 1024;
/// The largest APK taken, as the build takes it.
const APK_MOST: u64 = 200 * 1024 * 1024;

/// Why a release carrying an app unlike the last released one is refused.
const CARRIES_ANOTHER: &str = "carries another DSP Dispatch: Relay than the last released one";

/// One build of the app, as `relay.json` names it.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Build {
    version: String,
    version_code: u64,
    commit: String,
    asset: String,
    sha256: String,
    size: u64,
}
/// The builds a package carries: Dev's, which a release leaves to the build, and Production's
/// in a release's once an app exists.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apps {
    #[serde(default, rename = "dev")]
    _dev: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    production: Option<Build>,
}

/// Production's build `relay.json`'s bytes name, refused unless it's a release's own: three
/// numbers, its APK named for them, the full commit, a digest and a size within bounds.
fn production(bytes: &[u8]) -> Result<Option<Build>> {
    let apps: Apps = serde_json::from_slice(bytes)
        .map_err(|error| format!("{FILE} is not DSP Dispatch: Relay's builds: {error}"))?;
    if let Some(build) = &apps.production {
        require(
            releases::version(&build.version).is_ok()
                && build.asset == format!("dsp-dispatch-relay-{}.apk", build.version)
                && artifact::hex(&build.commit, 40)
                && artifact::hex(&build.sha256, 64)
                && (1..=APK_MOST).contains(&build.size)
                && build.version_code > 0,
            &format!("{FILE} names no Production build of its own"),
        )?;
    }
    Ok(apps.production)
}

/// Copies `app`, Production's app the package `candidate` carries, and its `relay.json` into
/// `staging`, the release being prepared.
pub(super) fn stage(candidate: &Path, staging: &Path, app: Option<&Build>) -> Result<()> {
    let packed = candidate.join(PACKED);
    if packed.join(FILE).try_exists()? {
        fs::copy(packed.join(FILE), staging.join(FILE))?;
    }
    if let Some(app) = app {
        fs::copy(packed.join(&app.asset), staging.join(&app.asset))?;
    }
    Ok(())
}

/// Production's app the package `candidate` carries, refused unless it's what the release
/// asked its checks for: the version `wanted`, built at `commit`, or with none wanted,
/// exactly `last`, the last released one.
pub(super) fn packed_app(
    candidate: &Path,
    commit: &str,
    wanted: &str,
    last: Option<&Build>,
) -> Result<Option<Build>> {
    let packed = candidate.join(PACKED);
    let app = if packed.join(FILE).try_exists()? {
        production(&fs::read(packed.join(FILE))?)?
    } else {
        None
    };
    if wanted.is_empty() {
        require(
            app.as_ref() == last,
            &format!("The release's build {CARRIES_ANOTHER}"),
        )?;
    } else {
        require(
            app.as_ref()
                .is_some_and(|app| app.version == wanted && app.commit == commit),
            &format!("The release's build lacks DSP Dispatch: Relay {wanted} built at {commit}"),
        )?;
    }
    if let Some(app) = &app {
        let apk = packed.join(&app.asset);
        require(
            apk.is_file()
                && apk.metadata()?.len() == app.size
                && artifact::file_hash(&apk)? == app.sha256,
            &format!("{} differs from {FILE}", app.asset),
        )?;
    }
    Ok(app)
}

/// Refuses `version`, a new app's, unless it's above `last`, the last released one.
fn above(version: &str, last: Option<&Build>) -> Result<()> {
    let Some(last) = last else {
        return Ok(());
    };
    require(
        releases::version(version)? > releases::version(&last.version)?,
        &format!(
            "DSP Dispatch: Relay {version} is not above {}, the last released",
            last.version
        ),
    )
}

impl Release<'_> {
    /// GitHub's latest release, none before any.
    fn latest_release(&self) -> Result<Option<Value>> {
        match io::github(self.system, "releases/latest") {
            Ok(release) => Ok(Some(release)),
            Err(error) if error.to_string().contains("HTTP 404") => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// The last released app: Production's entry in the `relay.json` GitHub's latest release
    /// attaches, none before a release carried one.
    pub(super) fn released_app(&self) -> Result<Option<Build>> {
        let Some(release) = self.latest_release()? else {
            return Ok(None);
        };
        let Some(attached) = release["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|found| found["name"] == FILE)
        else {
            return Ok(None);
        };
        require(
            attached["size"]
                .as_u64()
                .is_some_and(|size| size <= FILE_MOST),
            &format!("{FILE} on {} is too large", io::text(&release, "tag_name")),
        )?;
        let folder = tempfile::tempdir()?;
        releases::download_asset(self.system, &release, FILE, &folder.path().join(FILE))?;
        production(&fs::read(folder.path().join(FILE))?)
    }
    /// The `relay-version` this release's checks run with: Production's app version it
    /// releases, or empty to carry `last`, the last released app. Decided once for its commit
    /// and kept in its journal, so a rerun asks for the same; refused at or below `last`.
    pub(super) fn relay_version(&self, commit: &str, last: Option<&Build>) -> Result<String> {
        let mut journal = self.journal()?;
        let version = match journal.relay_version.clone() {
            Some(version) => version,
            None => {
                let version = self.next_app(commit, last)?;
                journal.relay_version = Some(version.clone());
                self.save(&journal)?;
                version
            }
        };
        if !version.is_empty() {
            above(&version, last)?;
        }
        say(match (version.as_str(), last) {
            ("", None) => "No DSP Dispatch: Relay to release".into(),
            ("", Some(last)) => format!("Carrying DSP Dispatch: Relay {}", last.version),
            (version, _) => format!("Releasing DSP Dispatch: Relay {version}"),
        });
        Ok(version)
    }
    /// The app's version after `last`, as the release's bump asks, `0.0.1` first, when `commit`
    /// changed `android/` since `last`'s commit; empty when it didn't.
    fn next_app(&self, commit: &str, last: Option<&Build>) -> Result<String> {
        let changed = match last {
            Some(last) => self.git(
                &["diff", "--name-only", &last.commit, commit, "--", "android"],
                None,
                120,
            )?,
            None => self.git(
                &["ls-tree", "--name-only", commit, "--", "android"],
                None,
                120,
            )?,
        };
        Ok(match (changed.is_empty(), last) {
            (true, _) => String::new(),
            (false, Some(last)) => next_version(&last.version, &self.bump)?,
            (false, None) => FIRST.into(),
        })
    }
    /// Refuses publishing the prepared release unless its app still follows the last released
    /// one, which a release published since it was prepared may have changed: a new version
    /// still above it, or a carried app still exactly it.
    pub(super) fn app_follows(&self) -> Result<()> {
        let last = self.released_app()?;
        let wanted = self.journal()?.relay_version.unwrap_or_default();
        if !wanted.is_empty() {
            return above(&wanted, last.as_ref());
        }
        let record = self.output.join(FILE);
        let app = if record.try_exists()? {
            production(&fs::read(record)?)?
        } else {
            None
        };
        require(
            app == last,
            &format!("The prepared release {CARRIES_ANOTHER}"),
        )
    }
    /// The app's assets the release prepared in `directory` attaches, each the file its
    /// runtime's manifest names: `relay.json` and Production's APK once an app exists.
    pub(super) fn app_assets(&self, directory: &Path) -> Result<Vec<String>> {
        let manifest = artifact::manifest(directory)?;
        let packed = |name: &str| {
            manifest
                .files
                .iter()
                .find(|entry| entry.path == format!("{PACKED}/{name}"))
        };
        let same = |name: &str, entry: &artifact::Entry| -> Result<()> {
            assets::regular(&directory.join(name))?;
            require(
                artifact::file_hash(&directory.join(name))? == entry.sha256,
                "Prepared release bytes changed; preserve this directory and inspect it",
            )
        };
        let Some(record) = packed(FILE) else {
            return Ok(vec![]);
        };
        same(FILE, record)?;
        let Some(app) = production(&fs::read(directory.join(FILE))?)? else {
            return Ok(vec![]);
        };
        same(
            &app.asset,
            packed(&app.asset).ok_or("Prepared manifest lacks Production's app")?,
        )?;
        Ok(vec![FILE.into(), app.asset])
    }
    /// The release notes' line naming the app's version, when the release carries a new one.
    pub(super) fn app_note(&self) -> Result<Option<String>> {
        Ok(self
            .journal()?
            .relay_version
            .filter(|version| !version.is_empty())
            .map(|version| format!("- DSP Dispatch: Relay {version}.\n")))
    }
}
