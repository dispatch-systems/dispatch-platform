//! The app this server hands out to the DSP's phones: the latest version of DSP Dispatch: Relay
//! its build packed, as `android/relay/release.json` names it, in the runtime's `dashboard/relay/`.
//! A preview server, as Dev is, hands out Dev's build, and Production its own. None while the build
//! packed none.
use dispatch_core::foundation::config::{Config, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where the build packs the app, in the runtime's dashboard folder.
const FOLDER: &str = "relay";
/// The release the app was packed from, beside its APKs.
const RELEASE: &str = "release.json";

/// What the release says, as the build checked it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    version: String,
    version_code: u32,
    builds: Builds,
}
#[derive(Deserialize)]
struct Builds {
    dev: Apk,
    production: Apk,
}
#[derive(Deserialize)]
struct Apk {
    asset: String,
    sha256: String,
    size: u64,
}

/// The app's latest version, as this server hands it out.
pub struct App {
    /// The build's name, as a phone shows it.
    pub name: &'static str,
    pub version: String,
    /// What Android compares to tell a newer version.
    pub version_code: u32,
    /// Its APK's SHA-256, in lowercase hex.
    pub sha256: String,
    /// Its APK's size, in bytes.
    pub size: u64,
    /// Its APK's file name, as its download is named.
    pub file: String,
    /// Where its APK is.
    pub path: PathBuf,
}

/// The app this server hands out, if its build packed one.
pub fn latest(config: &Config) -> Option<App> {
    packed(&config.dashboard.join(FOLDER), config.env())
}
/// The build of the app packed in `folder` for `environment`, while its APK is there whole.
fn packed(folder: &Path, environment: Environment) -> Option<App> {
    let release: Release =
        serde_json::from_slice(&std::fs::read(folder.join(RELEASE)).ok()?).ok()?;
    let (name, apk) = match environment {
        Environment::Preview => ("DSP Dispatch: Relay Dev", release.builds.dev),
        Environment::Production => ("DSP Dispatch: Relay", release.builds.production),
    };
    let path = folder.join(&apk.asset);
    let whole = Path::new(&apk.asset).file_name() == Some(apk.asset.as_ref())
        && std::fs::symlink_metadata(&path)
            .is_ok_and(|kept| kept.is_file() && kept.len() == apk.size);
    whole.then_some(App {
        name,
        version: release.version,
        version_code: release.version_code,
        sha256: apk.sha256,
        size: apk.size,
        file: apk.asset,
        path,
    })
}

#[cfg(test)]
#[path = "../../tests/backend/phone/app.rs"]
mod tests;
