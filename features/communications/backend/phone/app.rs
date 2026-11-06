//! The app this server hands out to the DSP's phones: the latest version of DSP Dispatch: Relay
//! its build packed, as CI built and signed it, in the runtime's `dashboard/relay/`. A preview
//! server, as Dev is, hands out Dev's build, which main's and a release's packages carry, and
//! Production its own, which only a release's package carries. None while the build packed none.
use dispatch_core::foundation::config::{Config, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where the build packs the app, in the runtime's dashboard folder.
const FOLDER: &str = "relay";
/// The file naming each build packed, beside their APKs.
const BUILDS: &str = "relay.json";

/// Each build packed, as the build checked it.
#[derive(Deserialize)]
struct Builds {
    dev: Option<Build>,
    production: Option<Build>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    version: String,
    version_code: u32,
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
    packed(&config.dashboard.join(FOLDER), config.environment)
}
/// The build of the app packed in `folder` for `environment`, while its APK is there whole.
fn packed(folder: &Path, environment: Environment) -> Option<App> {
    let builds: Builds = serde_json::from_slice(&std::fs::read(folder.join(BUILDS)).ok()?).ok()?;
    let (name, build) = match environment {
        Environment::Preview => ("DSP Dispatch: Relay Dev", builds.dev?),
        Environment::Production => ("DSP Dispatch: Relay", builds.production?),
    };
    let path = folder.join(&build.asset);
    let whole = Path::new(&build.asset).file_name() == Some(build.asset.as_ref())
        && std::fs::symlink_metadata(&path)
            .is_ok_and(|kept| kept.is_file() && kept.len() == build.size);
    whole.then_some(App {
        name,
        version: build.version,
        version_code: build.version_code,
        sha256: build.sha256,
        size: build.size,
        file: build.asset,
        path,
    })
}

#[cfg(test)]
#[path = "../../tests/backend/phone/app.rs"]
mod tests;
