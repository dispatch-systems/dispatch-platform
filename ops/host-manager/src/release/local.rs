//! Private Dev execution of restore and startup. Production only exports a captured backup.
use super::*;
use crate::releases::Provenance;
use std::{
    fs::File,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Component,
};

const MEMBERS: usize = 100_000;
pub(super) const INPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
// Tar headers and alignment add bytes beyond the extracted content.
const TRANSPORT_BYTES: u64 = INPUT_BYTES + MEMBERS as u64 * 2048 + 10240;
const RESERVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const CHECK: &str = include_str!("local.py");

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Check {
    Inventory,
    Restore,
    Upgrade,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CopyProof {
    pub content_sha256: String,
    pub config_sha256: String,
    pub taken_at: f64,
    pub files: u64,
    pub databases: u64,
    pub report: Option<Value>,
}

impl Release<'_> {
    pub(super) fn dev_preflight(&self) -> Result<()> {
        require(
            self.command(&["/usr/bin/cat", "/proc/sys/kernel/hostname"], None, 30)?
                == "dispatch-dev",
            "Backup verification requires the physical dispatch-dev host",
        )?;
        let custody = self.staging.clone();
        io::private_directory(&custody)?;
        // Interrupted copies from the previous location still require explicit cleanup.
        let legacy = self.directory.join(".dev-verification");
        if legacy.try_exists()? || legacy.is_symlink() {
            io::private_directory(&legacy)?;
            require(
                fs::read_dir(&legacy)?.next().is_none(),
                &format!(
                    "Unfinished private Dev staging in {}; obtain approval to inspect and clean it before retrying",
                    legacy.display()
                ),
            )?;
        }

        require(
            fs::read_dir(&custody)?.next().is_none(),
            &format!(
                "Unfinished private Dev staging in {}; obtain approval to inspect and clean it before retrying",
                custody.display()
            ),
        )?;
        require(
            self.system.available_space(&custody)? >= 2 * INPUT_BYTES + RESERVE_BYTES,
            "Dev backup staging requires at least 24 GiB free",
        )?;
        let args = sandbox();
        let mut check: Vec<&str> = args.iter().map(String::as_str).collect();
        check.extend(["--", "/usr/bin/true"]);
        self.command(&check, None, 30).map_err(|_| "Dev sandbox unavailable; approve installation/permissions before retrying; no fallback")?;
        Ok(())
    }

    /// Every reuse exports again, including a comparison with the current startup env.
    pub(super) fn dev_copy(
        &self,
        destination: &str,
        prepared: &Provenance,
        check: Check,
    ) -> Result<CopyProof> {
        self.dev_preflight()?;
        let current = self.prepared(Some(&prepared.commit))?;
        require(
            current.runtime_digest == prepared.runtime_digest
                && current.archive_sha256 == prepared.archive_sha256,
            "Dev candidate changed after preparation",
        )?;
        let work = tempfile::Builder::new()
            .prefix("copy-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(self.staging.clone())?;
        // A killed coordinator leaves private custody and blocks a retry. Explicit cleanup
        // must finish before returning evidence, including after an ordinary failure.
        let work = work.keep();
        let result = (|| {
            let archive = work.join("backup.tar");
            let script = format!(
                "set -eu; cmp -s {destination}/config/platform.env \"$HOME/dispatch-platform/public/config/platform.env\"; exec tar -cf - -C {destination} ."
            );
            let command = backup::remote(&script);
            self.system
                .download(
                    &[
                        "ssh",
                        "-o",
                        "BatchMode=yes",
                        "-o",
                        "ConnectTimeout=10",
                        backup::PRODUCTION_HOST,
                        &command,
                    ],
                    Some(&self.root),
                    1800,
                    &archive,
                    TRANSPORT_BYTES,
                )
                .map_err(|_| "Production backup export failed; no Dev evidence recorded")?;
            let input = work.join("input");
            let (content, config, manifest) = extract(&archive, &input)?;
            fs::remove_file(&archive)?;
            let taken_at = chrono::DateTime::parse_from_rfc3339(
                manifest["createdAt"]
                    .as_str()
                    .ok_or("Backup has no original capture timestamp")?,
            )?
            .timestamp_millis() as f64
                / 1000.0;
            backup::fresh(self.system.now(), taken_at)?;
            if check == Check::Inventory {
                return Ok(CopyProof {
                    content_sha256: content,
                    config_sha256: config,
                    taken_at,
                    files: manifest["files"].as_array().map_or(0, |v| v.len() as u64),
                    databases: 0,
                    report: None,
                });
            }
            let runtime = work.join("runtime");
            self.unpack_prepared(prepared, &runtime)?;
            let checker = work.join("check.py");
            fs::write(&checker, CHECK)?;
            let scratch = work.join("work");
            io::private_directory(&scratch)?;
            let mut args = sandbox();
            args.extend([
                "--bind".into(),
                scratch.to_str().ok_or("Invalid Dev path")?.into(),
                "/work".into(),
            ]);
            for (from, to) in [
                (&input, "/input"),
                (&runtime, "/runtime"),
                (&checker, "/check.py"),
            ] {
                args.extend([
                    "--ro-bind".into(),
                    from.to_str().ok_or("Invalid Dev path")?.into(),
                    to.into(),
                ]);
            }
            args.extend([
                "--chdir".into(),
                "/runtime".into(),
                "--".into(),
                "/usr/bin/python3".into(),
                "-I".into(),
                "/check.py".into(),
                if check == Check::Upgrade {
                    "upgrade"
                } else {
                    "restore"
                }
                .into(),
            ]);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let output = self
                .command(&args, None, 3600)
                .map_err(|_| "Dev restore/startup sandbox failed; private output suppressed")?;
            let found: Value = serde_json::from_str(&output)
                .map_err(|_| "Dev check returned an invalid report")?;
            require(
                found["files"].as_u64() == manifest["files"].as_array().map(|v| v.len() as u64),
                "The backup didn't restore every file",
            )?;
            let databases = found["databases"].as_u64().unwrap_or(0);
            require(databases > 0, "The restored backup holds no database")?;
            require(
                found["integrity"] == true,
                "The restored backup failed its integrity check",
            )?;
            let report = (check == Check::Upgrade).then(|| found["upgrade"].clone());
            if let Some(report) = &report {
                require(report["stopped"].is_null(), "Dev startup stopped")?;
                require(
                    report["carryOverFailures"]
                        .as_array()
                        .is_some_and(Vec::is_empty),
                    "Dev startup carry-over failed",
                )?;
            }
            Ok(CopyProof {
                content_sha256: content,
                config_sha256: config,
                taken_at,
                files: found["files"].as_u64().unwrap_or(0),
                databases,
                report,
            })
        })();
        if fs::remove_dir_all(&work).is_err() {
            return Err(format!(
                "Dev cleanup failed; private staging retained at {}; obtain approval for cleanup",
                work.display()
            )
            .into());
        }
        result
    }
}

fn sandbox() -> Vec<String> {
    [
        "/usr/bin/bwrap",
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        "/usr",
        "/usr",
        "--symlink",
        "usr/bin",
        "/bin",
        "--symlink",
        "usr/lib",
        "/lib",
        "--symlink",
        "usr/lib64",
        "/lib64",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--size",
        "67108864",
        "--tmpfs",
        "/tmp",
        "--clearenv",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
    ]
    .map(String::from)
    .to_vec()
}

/// Extract a bounded private inventory; tar cannot create links/devices or escape staging.
fn extract(archive: &Path, input: &Path) -> Result<(String, String, Value)> {
    let info = fs::symlink_metadata(archive)?;
    require(
        info.is_file() && !info.is_symlink() && info.nlink() == 1 && info.len() <= TRANSPORT_BYTES,
        "Backup transport exceeds its limit or is not a regular file",
    )?;
    io::private_directory(input)?;
    let mut inventory = std::collections::BTreeMap::new();
    let mut bytes = 0u64;
    let mut names = BTreeSet::new();
    for entry in tar::Archive::new(File::open(archive)?).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        require(
            !path.is_absolute()
                && path
                    .components()
                    .all(|p| matches!(p, Component::Normal(_) | Component::CurDir)),
            "Unsafe backup export path",
        )?;
        let path: PathBuf = path
            .components()
            .filter(|p| *p != Component::CurDir)
            .collect();
        let kind = entry.header().entry_type();
        require(
            kind.is_dir() || kind.is_file(),
            "Backup export must contain only regular files and directories",
        )?;
        if path.as_os_str().is_empty() {
            continue;
        }
        require(
            names.len() < MEMBERS && names.insert(path.clone()),
            "Backup export has too many or duplicate members",
        )?;
        let target = input.join(&path);
        if kind.is_dir() {
            io::private_directory(&target)?;
            continue;
        }
        bytes = bytes
            .checked_add(entry.size())
            .ok_or("Backup export size overflow")?;
        require(
            bytes <= INPUT_BYTES,
            "Backup export exceeds the 8 GiB input limit",
        )?;
        io::private_directory(target.parent().ok_or("Invalid backup export path")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
        std::io::copy(&mut entry, &mut file)?;
        require(
            file.metadata()?.nlink() == 1,
            "Backup export must not contain linked files",
        )?;
        inventory.insert(
            path.to_str()
                .ok_or("Invalid backup export name")?
                .to_owned(),
            artifact::file_hash(&target)?,
        );
    }
    let manifest = io::read_json(&input.join("backup.json"))?;
    require(manifest["format"] == 2, "Unsupported backup format")?;
    let files = manifest["files"]
        .as_array()
        .ok_or("Backup manifest has no files")?;
    let mut listed = BTreeSet::from(["backup.json".to_owned()]);
    for file in files {
        let name = file["path"]
            .as_str()
            .ok_or("Invalid backup manifest path")?;
        require(
            (name.starts_with("data/") || name.starts_with("dsps/"))
                && listed.insert(name.into())
                && inventory.get(name).is_some_and(|h| file["sha256"] == *h)
                && input.join(name).metadata()?.len() == file["size"].as_u64().unwrap_or(u64::MAX),
            "Backup content differs from its manifest",
        )?;
    }
    require(
        inventory
            .keys()
            .all(|p| listed.contains(p) || p.starts_with("config/")),
        "Backup export contains unlisted data",
    )?;
    let config = inventory
        .get("config/platform.env")
        .ok_or("Captured startup config is missing")?
        .clone();
    require(
        inventory.contains_key("data/platform/platform.key")
            && input.join("data/platform/platform.key").metadata()?.len() == 32,
        "Captured platform key is missing or invalid; refusing key regeneration",
    )?;
    let content = artifact::hash(&serde_json::to_vec(&inventory)?);
    Ok((content, config, manifest))
}
