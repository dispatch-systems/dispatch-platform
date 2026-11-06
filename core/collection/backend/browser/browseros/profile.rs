//! Persistent profile preparation, privacy traces and exclusive ownership.
use crate::{Error, Result, db, ensure};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
};

pub(super) fn prepare(profile: &Path) -> Result<()> {
    // Credentials belong to the DSP vault; password-manager chrome must not
    // cover native PIN entry or persist another copy in the browser profile.
    let defaults = db::private_dir(&profile.join("Default"))?;
    let preferences = defaults.join("Preferences");
    db::private_file(&preferences, false)?;
    let mut settings: Value = if preferences.exists() {
        ensure(
            fs::metadata(&preferences)?.len() <= 4 * 1024 * 1024,
            "browser_profile_invalid",
            409,
        )?;
        serde_json::from_slice(&fs::read(&preferences)?)?
    } else {
        json!({})
    };
    ensure(
        settings.is_object() && (settings["profile"].is_null() || settings["profile"].is_object()),
        "browser_profile_invalid",
        409,
    )?;
    settings["credentials_enable_service"] = json!(false);
    settings["profile"]["password_manager_enabled"] = json!(false);
    db::write_private(&preferences, &serde_json::to_vec(&settings)?)
}

/// What a browser keeps of the pages it opened: its caches, history, sessions, icons,
/// page storage, and the stores its shopping and metrics features fill from what pages
/// show. Everything a collection reads goes into a database, so none of it may stay in
/// a profile: it is removed before every start and after every exit, leaving only what
/// signing in needs (cookies, local storage, preferences).
const PAGE_TRACES: &[&str] = &[
    "Default/Cache",
    "Default/Code Cache",
    "Default/GPUCache",
    "Default/DawnGraphiteCache",
    "Default/DawnWebGPUCache",
    "Default/History",
    "Default/History-journal",
    "Default/Visited Links",
    "Default/Top Sites",
    "Default/Top Sites-journal",
    "Default/Favicons",
    "Default/Favicons-journal",
    "Default/Shortcuts",
    "Default/Shortcuts-journal",
    "Default/Network Action Predictor",
    "Default/Network Action Predictor-journal",
    "Default/Sessions",
    "Default/Session Storage",
    "Default/Service Worker",
    "Default/blob_storage",
    "Default/Shared Dictionary",
    "Default/SharedStorage",
    "Default/SharedStorage-shm",
    "Default/SharedStorage-wal",
    "Default/Reporting and NEL",
    "Default/Reporting and NEL-journal",
    "Default/BrowsingTopicsSiteData",
    "Default/BrowsingTopicsSiteData-journal",
    "Default/BrowsingTopicsState",
    "Default/Site Characteristics Database",
    "Default/optimization_guide_hint_cache_store",
    "Default/parcel_tracking_db",
    "Default/chrome_cart_db",
    "Default/commerce_subscription_db",
    "Default/discount_infos_db",
    "Default/discounts_db",
    "Default/DIPS",
    "Default/DIPS-shm",
    "Default/DIPS-wal",
    "Default/DIPS-journal",
    "Default/Web Data",
    "Default/Web Data-journal",
    "Default/Account Web Data",
    "Default/Account Web Data-journal",
    "Default/Segmentation Platform",
    "Default/shared_proto_db",
    "segmentation_platform",
    "GPUPersistentCache",
    "BrowserMetrics",
    "BrowserMetrics-spare.pma",
    "CrashpadMetrics-active.pma",
    "config/browser-os/Crash Reports",
];
/// Removes every page trace from `profile`. Runs only while this process holds the
/// profile's lease, so no browser is writing to it. A trace that cannot be removed does
/// not keep the rest: every entry is tried, and the first failure is returned.
fn scrub(profile: &Path) -> Result<()> {
    let mut first = None;
    for entry in PAGE_TRACES {
        let result = remove_trace(profile, Path::new(entry));
        match result {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                first.get_or_insert(error);
            }
            _ => (),
        }
    }
    first.map_or(Ok(()), |error| Err(error.into()))
}
/// Refuse to traverse a link (or a file standing in for a directory) anywhere
/// before the trace itself. The final component may be a link: unlinking that
/// component removes the profile entry without touching its target.
fn remove_trace(profile: &Path, entry: &Path) -> std::io::Result<()> {
    let mut components = Vec::new();
    for component in entry.components() {
        match component {
            Component::Normal(name) => components.push(name),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "browser trace path is not relative",
                ));
            }
        }
    }
    let Some((final_name, ancestors)) = components.split_last() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "browser trace path is empty",
        ));
    };
    let mut path = profile.to_path_buf();
    for component in ancestors {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "browser trace ancestor is not a directory",
            ));
        }
        path.push(component);
    }
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "browser trace parent is not a directory",
        ));
    }
    path.push(final_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(path),
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) => Err(error),
    }
}
/// A scrub that failed is reported, never fatal: the browser still starts or stops, and
/// the next start or exit tries again.
pub(super) fn scrub_reported(profile: &Path) {
    if let Err(error) = scrub(profile) {
        crate::foundation::observability::event(
            "error",
            "browser.scrub_failed",
            serde_json::json!({"error":error.code}),
        );
    }
}

pub(super) fn lease(profile: &Path) -> Result<File> {
    db::private_dir(profile)?;
    let parent = profile
        .parent()
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    db::private_dir(parent)?;
    let name = profile
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    let path = parent.join(format!(".{name}.browseros.lock"));
    db::private_file(&path, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    let stat = file.metadata()?;
    ensure(
        stat.is_file()
            && stat.nlink() == 1
            && stat.uid() == unsafe { libc::geteuid() }
            && stat.mode() & 0o077 == 0,
        "unsafe_storage_file",
        500,
    )?;
    file.try_lock_exclusive()
        .map_err(|_| Error::new("browser_profile_busy", 409))?;
    Ok(file)
}

#[cfg(test)]
#[path = "../../../tests/backend/browser/browseros/profile.rs"]
mod tests;
