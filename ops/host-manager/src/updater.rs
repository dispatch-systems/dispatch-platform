mod dev;
mod production;

use crate::{
    Result,
    artifact::{self, Manifest},
    io::{self, System},
    releases, require,
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// How long a release's backup of Production is kept: what's deleted from Production is gone from
/// every backup within 30 days, as from its nightly ones.
const RELEASE_BACKUPS_KEPT: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Lets go of the backups in `folder` that releases took before `now` less `RELEASE_BACKUPS_KEPT`:
/// each a folder of its own, named for when it was taken and the release it was taken before
/// (`2026-11-05-091500-before-v1.2.3`), so one only partly let go of stays due until it's gone.
/// Anything else stays.
pub fn expire_release_backups(folder: &Path, now: SystemTime) -> Result<()> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(taken) = release_backup_taken(&name) else {
            continue;
        };
        if entry.file_type()?.is_dir() && taken + RELEASE_BACKUPS_KEPT < now {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
/// When the release backup named `name` was taken, by the time it's named for: none for a name
/// not a release backup's.
fn release_backup_taken(name: &str) -> Option<SystemTime> {
    let (taken, _) = name.split_once("-before-v")?;
    let taken = chrono::NaiveDateTime::parse_from_str(taken, "%Y-%m-%d-%H%M%S").ok()?;
    let seconds = u64::try_from(taken.and_utc().timestamp()).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Production,
}
impl Environment {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Production => "production",
        }
    }
    fn service(self) -> &'static str {
        match self {
            Self::Dev => "dispatch-dev.service",
            Self::Production => "dispatch-production.service",
        }
    }
}
/// The `gh` Production verifies releases with, by its root-owned path, so a user-writable `gh`
/// earlier on PATH can't fake `gh attestation verify`.
pub const GH: &str = "/usr/local/bin/gh";
pub struct Updater<'a> {
    pub root: PathBuf,
    pub active: PathBuf,
    pub runtime: PathBuf,
    pub platform: PathBuf,
    pub previous: PathBuf,
    pub receipt: PathBuf,
    pub status_file: PathBuf,
    /// The update that keeps failing, and why. The status says what was last installed, which
    /// a failing update leaves as it was, so a stalled environment would otherwise read ready.
    pub failure_file: PathBuf,
    pub environment: Environment,
    pub system: &'a dyn System,
    health_url: String,
}
impl<'a> Updater<'a> {
    pub fn new(root: &Path, environment: Environment, system: &'a dyn System) -> Result<Self> {
        let root = std::path::absolute(root)?;
        require(
            root.file_name().and_then(|s| s.to_str())
                == Some(if environment == Environment::Dev {
                    "live"
                } else {
                    "public"
                }),
            "Invalid environment root",
        )?;
        io::private_directory(&root)?;
        let runtime = root.join(".runtime");
        let platform = root.join("data/platform");
        io::private_directory(&runtime)?;
        io::private_directory(&platform)?;
        let active = root.join(if environment == Environment::Dev {
            ".build"
        } else {
            "live"
        });
        require(!active.is_symlink(), "Runtime symlink denied")?;
        let config = io::read_json(&root.join("config/updater.json"))?;
        require(
            config["service"] == environment.service(),
            "Environment service required",
        )?;
        let health_url = io::text(&config, "healthUrl");
        require(
            regex::Regex::new(r"^http://127\.0\.0\.1:[0-9]+/api/health$")?.is_match(&health_url),
            "Loopback health endpoint required",
        )?;
        if environment == Environment::Dev {
            require(
                root.join(".git").is_dir(),
                "Dev requires its persistent repository checkout",
            )?;
            let exclude = root.join(".git/info/exclude");
            let existing = fs::read_to_string(&exclude).unwrap_or_default();
            let mut out = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(exclude)?;
            for name in ["config", "data", "dsps", ".platform.lock"] {
                if !existing.lines().any(|line| line == format!("/{name}")) {
                    writeln!(out, "/{name}")?;
                }
            }
        }
        let name = environment.name();
        Ok(Self {
            previous: runtime.join("previous"),
            receipt: platform.join(format!("{name}-activation.json")),
            status_file: platform.join(format!("{name}-update.json")),
            failure_file: platform.join(format!("{name}-update-failure.json")),
            root,
            active,
            runtime,
            platform,
            environment,
            system,
            health_url,
        })
    }
    pub fn git(&self, args: &[&str]) -> Result<String> {
        let mut cmd = vec!["git"];
        cmd.extend_from_slice(args);
        Ok(
            String::from_utf8(self.system.command(&cmd, Some(&self.root), 120, None)?)?
                .trim()
                .to_owned(),
        )
    }
    /// The branch Dev follows: main. A checkout still on `dev`, as a legacy host handing
    /// over from the Python updater is, keeps following it until it is moved onto main.
    pub fn tracked_branch(&self) -> Result<&'static str> {
        let branch = self.git(&["branch", "--show-current"])?;
        ["main", "dev"]
            .into_iter()
            .find(|trusted| *trusted == branch)
            .ok_or_else(|| "Dev checkout must be on main".into())
    }
    pub fn clean_checkout(&self) -> Result<()> {
        self.tracked_branch()?;
        require(
            self.git(&["status", "--porcelain", "--untracked-files=all"])?
                .is_empty(),
            "Dev checkout contains unfinished changes",
        )?;
        let origin = self.git(&["remote", "get-url", "origin"])?;
        require(
            crate::REPOSITORIES.iter().any(|name| {
                origin == format!("https://github.com/{name}.git")
                    || origin == format!("git@github.com:{name}.git")
            }),
            "Unexpected repository origin",
        )?;
        self.check_source("HEAD")
    }
    fn check_source(&self, commit: &str) -> Result<()> {
        require(
            self.git(&[
                "ls-tree",
                "-r",
                "--name-only",
                commit,
                "--",
                "config",
                "data",
                "dsps",
                ".platform.lock",
            ])?
            .is_empty(),
            "Dev source must not contain private environment paths",
        )
    }
    fn service(&self, action: &str) -> Result<()> {
        self.system.command(
            &["systemctl", "--user", action, self.environment.service()],
            None,
            90,
            None,
        )?;
        Ok(())
    }
    fn healthy(&self, digest: &str, timeout: u64) -> bool {
        let deadline = self.system.monotonic() + Duration::from_secs(timeout);
        loop {
            if let Ok(value) = self
                .system
                .request(&self.health_url, false, false, 2)
                .and_then(io::json_response)
                && value["status"] == "ready"
                && value["release"] == digest
                && value["environment"]
                    == if self.environment == Environment::Dev {
                        "preview"
                    } else {
                        "production"
                    }
                && (self.environment == Environment::Dev || value["runtime"] == "rust")
            {
                return true;
            }
            if self.system.monotonic() >= deadline {
                return false;
            }
            self.system.sleep(Duration::from_secs(1));
        }
    }
    fn status(&self, state: &str, manifest: &Manifest, extra: Value) -> Result<()> {
        let mut value = json!({"status":state,"digest":manifest.digest,"updatedAt":chrono::Utc::now().to_rfc3339()});
        if self.environment == Environment::Production {
            value["version"] = json!(manifest.version);
        }
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().ok_or("Invalid status")?.clone());
        io::write_json(&self.status_file, &value)
    }
    pub fn verify(&self) -> Result<Manifest> {
        let manifest = if self.environment == Environment::Dev {
            self.clean_checkout()?;
            artifact::verify(&self.active, Some(&self.git(&["rev-parse", "HEAD"])?))?
        } else {
            let m = artifact::verify(&self.active, None)?;
            releases::version(&m.version)?;
            m
        };
        fs::set_permissions(
            self.active.join("services/rust/dispatch-backend"),
            fs::Permissions::from_mode(0o700),
        )?;
        Ok(manifest)
    }
    pub fn recover(&self) -> Result<()> {
        if !self.receipt.try_exists()? {
            return Ok(());
        }
        let receipt = io::read_json(&self.receipt)?;
        let old_digest = io::text(&receipt, "oldDigest");
        require(artifact::hex(&old_digest, 64), "Invalid activation receipt")?;
        let old_commit = io::text(&receipt, "oldCommit");
        if self.environment == Environment::Dev {
            let commit = io::text(&receipt, "commit");
            require(
                receipt["previous"] == "previous"
                    && artifact::hex(&old_commit, 40)
                    && artifact::hex(&commit, 40),
                "Invalid activation receipt",
            )?;
            self.clean_checkout()?;
            require(
                [old_commit.as_str(), commit.as_str()]
                    .contains(&self.git(&["rev-parse", "HEAD"])?.as_str()),
                "Checkout changed during interrupted update",
            )?;
        } else {
            require(
                artifact::hex(&io::text(&receipt, "newDigest"), 64),
                "Invalid activation receipt",
            )?;
        }
        self.service("stop")?;
        let retained = self.previous.try_exists()?;
        let source = if retained {
            &self.previous
        } else {
            &self.active
        };
        let old = artifact::verify(
            source,
            (self.environment == Environment::Dev).then_some(old_commit.as_str()),
        )?;
        require(old.digest == old_digest, "Rollback inventory differs")?;
        if retained {
            if self.active.try_exists()? {
                artifact::real_directory(&self.active)?;
                fs::remove_dir_all(&self.active)?;
            }
            fs::rename(&self.previous, &self.active)?;
        }
        if self.environment == Environment::Dev {
            self.git(&["reset", "--hard", &old_commit])?;
        }
        fs::set_permissions(
            self.active.join("services/rust/dispatch-backend"),
            fs::Permissions::from_mode(0o700),
        )?;
        self.service("start")?;
        require(
            self.healthy(&old_digest, 40),
            "Previous runtime failed health check",
        )?;
        self.status(
            "rolled_back",
            &old,
            if self.environment == Environment::Dev {
                json!({"commit":old_commit})
            } else {
                json!({"failedDigest":receipt["newDigest"]})
            },
        )?;
        io::remove_receipt(&self.receipt)
    }
    pub fn activate(&self, candidate: &Path, commit: &str) -> Result<()> {
        require(artifact::hex(commit, 40), "Invalid build commit")?;
        let manifest = artifact::verify(candidate, Some(commit))?;
        let current = if self.environment == Environment::Dev {
            self.clean_checkout()?;
            self.check_source(commit)?;
            Some(self.git(&["rev-parse", "HEAD"])?)
        } else {
            None
        };
        let old = artifact::verify(&self.active, current.as_deref())?;
        require(
            old.schema == manifest.schema,
            "Schema change requires an explicit migration plan",
        )?;
        if let Some(current) = &current {
            self.git(&["merge-base", "--is-ancestor", current, commit])?;
            let tracked = format!("origin/{}", self.tracked_branch()?);
            self.git(&["merge-base", "--is-ancestor", commit, &tracked])?;
        } else {
            require(
                releases::version(&manifest.version)? > releases::version(&old.version)?,
                "Release downgrade/replacement denied",
            )?;
        }
        fs::set_permissions(
            candidate.join("services/rust/dispatch-backend"),
            fs::Permissions::from_mode(0o700),
        )?;
        if self.previous.try_exists()? {
            artifact::real_directory(&self.previous)?;
            fs::remove_dir_all(&self.previous)?;
        }
        let record = if let Some(current) = &current {
            json!({"commit":commit,"oldCommit":current,"oldDigest":old.digest,"previous":"previous"})
        } else {
            json!({"oldDigest":old.digest,"newDigest":manifest.digest})
        };
        io::write_json(&self.receipt, &record)?;
        let activation = self.switch(
            candidate,
            &manifest,
            current.as_deref().map(|current| (current, commit)),
            "ready",
            json!({"commit":commit}),
        );
        if let Err(error) = activation {
            self.recover()?;
            crate::support::refresh(self);
            return Err(error);
        }
        crate::support::refresh(self);
        Ok(())
    }
    /// With the receipt written, stops the service, makes `candidate` the active runtime and
    /// the active one the previous, and starts it; once it is healthy on `manifest`, records
    /// `state` with `extra` and removes the receipt. On Dev the checkout moves with it, from
    /// the first commit of `checkout` to the second. A failure leaves the receipt to `recover`.
    fn switch(
        &self,
        candidate: &Path,
        manifest: &Manifest,
        checkout: Option<(&str, &str)>,
        state: &str,
        extra: Value,
    ) -> Result<()> {
        self.service("stop")?;
        if let Some((current, _)) = checkout {
            self.clean_checkout()?;
            require(
                self.git(&["rev-parse", "HEAD"])? == current,
                "Checkout changed during update",
            )?;
        }
        fs::rename(&self.active, &self.previous)?;
        fs::rename(candidate, &self.active)?;
        if let Some((_, commit)) = checkout {
            self.git(&["merge", "--ff-only", commit])?;
        }
        self.service("start")?;
        require(
            self.healthy(&manifest.digest, 40),
            "New runtime failed health check",
        )?;
        self.status(state, manifest, extra)?;
        io::remove_receipt(&self.receipt)
    }
    /// Puts Production back on the release it ran before, which it keeps as its previous
    /// runtime, for a release that installed healthy but is wrong. The two swap, so the newer
    /// one is kept in turn, and its release is recorded as failed: the updater stays off it
    /// and installs the next one published.
    pub fn rollback(&self) -> Result<Value> {
        require(
            self.environment == Environment::Production,
            "Only Production rolls back",
        )?;
        let _lock = self
            .lock()?
            .ok_or("Production's updater is running; try again once it finishes")?;
        self.recover()?;
        let current = artifact::verify(&self.active, None)?;
        require(
            self.previous.try_exists()?,
            "Production keeps no previous release to roll back to",
        )?;
        let older = artifact::verify(&self.previous, None)?;
        require(
            releases::version(&older.version)? < releases::version(&current.version)?,
            &format!(
                "Production's previous release, {}, is not older than the {} it runs",
                older.version, current.version
            ),
        )?;
        let release =
            releases::public_github(self.system, &format!("releases/tags/v{}", current.version))?;
        require(
            releases::release_version(&release)? == current.version && release["id"].is_u64(),
            "GitHub's release differs from the one Production runs",
        )?;
        let candidate = self.runtime.join("rollback");
        fs::rename(&self.previous, &candidate)?;
        let switched = io::write_json(
            &self.receipt,
            &json!({"oldDigest":current.digest,"newDigest":older.digest}),
        )
        .and_then(|()| {
            self.switch(
                &candidate,
                &older,
                None,
                "rolled_back",
                json!({"failedReleaseId":release["id"],"failedDigest":current.digest}),
            )
        });
        if let Err(error) = switched {
            // Recovery removes the runtime it finds active, so the older one moves aside first
            // and is kept as the previous again.
            if !candidate.try_exists()? {
                fs::rename(&self.active, &candidate)?;
            }
            self.recover()?;
            fs::rename(&candidate, &self.previous)?;
            return Err(format!(
                "Rolling back to {} failed, so Production is back on {}: {error}",
                older.version, current.version
            )
            .into());
        }
        Ok(json!({
            "status": "rolled_back",
            "from": current.version,
            "to": older.version,
            "digest": older.digest,
        }))
    }
    /// The updater's lock, or `None` while another run of it holds it.
    fn lock(&self) -> Result<Option<io::ExclusiveLock>> {
        io::ExclusiveLock::try_acquire(
            &self
                .platform
                .join(format!("{}-update.lock", self.environment.name())),
        )
    }
    pub fn run_locked(&self) -> Result<()> {
        // Another run of this environment holds the lock: this one has nothing to do.
        let Some(_lock) = self.lock()? else {
            return Ok(());
        };
        let result = self.recover().and_then(|()| match self.environment {
            Environment::Dev => self.update_dev().map(|()| true),
            Environment::Production => {
                // Release backups expire whatever the update made of this tick.
                let updated = self.update_production();
                let expired =
                    expire_release_backups(&self.root.with_file_name("backups"), SystemTime::now());
                updated.and_then(|updated| expired.map(|()| updated))
            }
        });
        let recorded = self.record_update(&result);
        result.map(drop).and(recorded)
    }
    /// Keeps a failing update's error, from its first attempt, until one gets through. A tick
    /// that tried nothing, `Ok(false)`, leaves it as it was.
    fn record_update(&self, result: &Result<bool>) -> Result<()> {
        match result {
            Ok(true) if self.failure_file.exists() => io::remove_receipt(&self.failure_file),
            Ok(_) => Ok(()),
            Err(error) => {
                let previous = io::read_json(&self.failure_file).unwrap_or(Value::Null);
                let now = chrono::Utc::now().to_rfc3339();
                io::write_json(
                    &self.failure_file,
                    &json!({
                        "error": error.to_string(),
                        "since": previous["since"].as_str().unwrap_or(&now),
                        "attempts": previous["attempts"].as_u64().unwrap_or(0) + 1,
                        "updatedAt": now,
                    }),
                )
            }
        }
    }
}
