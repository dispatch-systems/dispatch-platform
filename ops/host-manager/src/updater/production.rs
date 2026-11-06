use super::{GH, Updater};
use crate::{
    Result,
    artifact::{self, Manifest},
    io, management,
    releases::{self, Provenance},
    require,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

impl Updater<'_> {
    fn settled(&self, digest: &str) -> bool {
        let Ok(check) = io::read_json(&self.platform.join("production-release-check.json")) else {
            return false;
        };
        let age = self.system.now() - check["checkedAt"].as_f64().unwrap_or(f64::NEG_INFINITY);
        (0.0..600.0).contains(&age)
            && check["digest"] == digest
            && check["tag"]
                .as_str()
                .is_some_and(|tag| releases::latest_tag(self.system).as_deref() == Some(tag))
    }
    fn settle(&self, release: &Value, current: &Manifest) -> Result<()> {
        io::write_json(
            &self.platform.join("production-release-check.json"),
            &json!({"tag":release["tag_name"],"digest":current.digest,"checkedAt":self.system.now()}),
        )
    }
    /// Whether this tick tried: not while it waits out the delay after a failed attempt.
    pub(super) fn update_production(&self) -> Result<bool> {
        // Checked on every tick, so a missing tool is reported before a release needs it.
        require(
            self.system.executable(Path::new(GH)),
            &format!("Production's updater needs {GH}"),
        )?;
        // Nothing new since the last full check: the recorded digest tells, so the runtime is
        // hashed again only once there may be something to do, at least every ten minutes.
        if self.settled(&artifact::manifest(&self.active)?.digest) {
            return Ok(true);
        }
        let retry = self.platform.join("production-update-retry.json");
        let pending = io::read_json(&retry).unwrap_or(Value::Null);
        if self.system.now() < pending["retryAt"].as_f64().unwrap_or(f64::NEG_INFINITY) {
            return Ok(false);
        }
        let result = self.install_latest();
        if result.is_ok() {
            if retry.try_exists()? {
                fs::remove_file(&retry)?;
            }
        } else {
            // This host has 60 unauthenticated GitHub API calls an hour, and an attempt makes
            // up to five, so a failure repeated every 30 seconds would soon exhaust them. Wait
            // 1, 2, 4, 8, then 10 minutes between attempts.
            let failures = pending["failures"].as_u64().unwrap_or(0) + 1;
            let delay = (60u64 << (failures - 1).min(4)).min(600);
            let record = json!({"failures":failures,"retryAt":self.system.now() + delay as f64});
            if let Err(error) = io::write_json(&retry, &record) {
                eprintln!("Retry delay was not recorded: {error}");
            }
        }
        result.map(|()| true)
    }
    fn install_latest(&self) -> Result<()> {
        let current = artifact::verify(&self.active, None)?;
        let release = releases::public_github(self.system, "releases/latest")?;
        require(
            release["immutable"] == true,
            "Production release must be immutable",
        )?;
        let selected = releases::release_version(&release)?;
        let order = releases::version(&selected)?.cmp(&releases::version(&current.version)?);
        if order.is_lt() {
            return self.settle(&release, &current);
        }
        if order.is_eq() {
            require(
                self.healthy(&current.digest, 5),
                "Installed Production runtime is not healthy",
            )?;
            let status = io::read_json(&self.status_file).unwrap_or(Value::Null);
            if status["status"] != "ready" || status["digest"] != current.digest {
                self.status("ready",&current,json!({"commit":io::read_json(&self.active.join("tooling/build-info.json"))?["commit"]}))?;
            }
            return self.settle(&release, &current);
        }
        if self.status_file.exists()
            && io::read_json(&self.status_file)?["failedReleaseId"] == release["id"]
        {
            return self.settle(&release, &current);
        }
        let commit = releases::release_commit(self.system, &io::text(&release, "tag_name"))?;
        let temp = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(&self.runtime)?;
        let archive = temp.path().join("runtime.tar.gz");
        releases::download_asset(
            self.system,
            &release,
            &format!("dispatch-platform-{selected}.tar.gz"),
            &archive,
        )?;
        releases::download_asset(
            self.system,
            &release,
            "release.json",
            &temp.path().join("release.json"),
        )?;
        releases::download_asset(
            self.system,
            &release,
            "provenance.json",
            &temp.path().join("provenance.json"),
        )?;
        releases::download_asset(
            self.system,
            &release,
            "SHA256SUMS",
            &temp.path().join("SHA256SUMS"),
        )?;
        let build_archive = temp.path().join("dispatch-build.tar.gz");
        let attestation = temp.path().join("attestation.sigstore.jsonl");
        releases::download_asset(
            self.system,
            &release,
            "dispatch-build.tar.gz",
            &build_archive,
        )?;
        releases::download_asset(
            self.system,
            &release,
            "attestation.sigstore.jsonl",
            &attestation,
        )?;
        let published: Manifest =
            serde_json::from_value(io::read_json(&temp.path().join("release.json"))?)?;
        let provenance: Provenance =
            serde_json::from_value(io::read_json(&temp.path().join("provenance.json"))?)?;
        let archive_name = format!("dispatch-platform-{selected}.tar.gz");
        let release_manifest = temp.path().join("release.json");
        let provenance_file = temp.path().join("provenance.json");
        let checksums = [
            (archive_name.as_str(), &archive),
            ("release.json", &release_manifest),
            ("provenance.json", &provenance_file),
            ("dispatch-build.tar.gz", &build_archive),
            ("attestation.sigstore.jsonl", &attestation),
        ]
        .into_iter()
        .map(|(name, path)| Ok(format!("{}  {name}\n", artifact::file_hash(path)?)))
        .collect::<Result<String>>()?;
        require(
            fs::read_to_string(temp.path().join("SHA256SUMS"))? == checksums
                && crate::REPOSITORIES.contains(&provenance.repository.as_str())
                && provenance.commit == commit
                && provenance.version == selected
                && provenance.workflow_run > 0
                && provenance.workflow_attempt > 0
                && provenance.artifact_id > 0
                && provenance
                    .artifact_digest
                    .strip_prefix("sha256:")
                    .is_some_and(|value| artifact::hex(value, 64))
                && provenance.build_sha256 == artifact::file_hash(&build_archive)?
                && provenance.attestation_sha256 == artifact::file_hash(&attestation)?
                && provenance.runtime_digest == published.digest
                && provenance.archive_sha256 == artifact::file_hash(&archive)?,
            "Published release provenance differs",
        )?;
        self.system.command(
            &[
                GH,
                "attestation",
                "verify",
                build_archive.to_str().ok_or("Invalid build archive path")?,
                "--bundle",
                attestation.to_str().ok_or("Invalid attestation path")?,
                "--repo",
                crate::REPOSITORY,
                "--signer-workflow",
                &format!("{}/.github/workflows/checks.yml", crate::REPOSITORY),
                "--source-digest",
                &commit,
                "--deny-self-hosted-runners",
            ],
            None,
            120,
            None,
        )?;
        let candidate = temp.path().join("candidate");
        artifact::unpack(&build_archive, &candidate)?;
        artifact::verify(&candidate, Some(&commit))?;
        let manifest = artifact::stamp(&candidate, &commit, &selected)?;
        require(published == manifest, "Published release inventory differs")?;
        if releases::public_github(self.system, "releases/latest")?["id"] != release["id"] {
            return Ok(());
        }
        if let Err(error) = self.activate(&candidate, &commit) {
            if !self.receipt.exists() {
                self.status(
                    "rolled_back",
                    &artifact::verify(&self.active, None)?,
                    json!({"failedReleaseId":release["id"],"failedDigest":manifest.digest}),
                )?;
            }
            return Err(error);
        }
        // Adopt the updater the release just proved healthy, as Dev does. Only after an
        // activation, never on an ordinary tick, so an operator who restores the previous
        // updater by hand keeps it until the next release. A copy that fails its own
        // self-check is not installed and the running updater stays.
        management::refresh(self)
    }
}
