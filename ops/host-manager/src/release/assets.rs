use super::*;
use crate::releases::Provenance;
use std::{
    fs::File,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

pub(super) fn regular(path: &Path) -> Result<()> {
    let info = fs::symlink_metadata(path)?;
    require(
        info.is_file() && !info.is_symlink() && info.nlink() == 1 && info.len() <= crate::MAX_BYTES,
        "Release assets must be bounded regular files",
    )
}
fn checksums(directory: &Path, names: &[String]) -> Result<String> {
    let mut result = String::new();
    for name in names {
        result.push_str(&format!(
            "{}  {name}\n",
            artifact::file_hash(&directory.join(name))?
        ));
    }
    Ok(result)
}

impl Release<'_> {
    fn archive(&self) -> String {
        format!("dispatch-platform-{}.tar.gz", self.version)
    }
    /// The runtime's assets, which Production's updater installs: `SHA256SUMS`, last, sums
    /// the others.
    fn runtime_assets(&self) -> Vec<String> {
        vec![
            self.archive(),
            "release.json".into(),
            "provenance.json".into(),
            "dispatch-build.tar.gz".into(),
            "attestation.sigstore.jsonl".into(),
            "SHA256SUMS".into(),
        ]
    }
    /// Every asset the release prepared in `directory` attaches: the runtime's, then the app's.
    fn asset_names(&self, directory: &Path) -> Result<Vec<String>> {
        Ok([self.runtime_assets(), self.app_assets(directory)?].concat())
    }
    pub(super) fn prepared(&self, commit: Option<&str>) -> Result<Provenance> {
        self.verify_preparation(&self.output, commit)
    }
    fn verify_preparation(&self, directory: &Path, commit: Option<&str>) -> Result<Provenance> {
        artifact::real_directory(directory)?;
        let names = self.runtime_assets();
        for name in &names {
            regular(&directory.join(name))?;
        }
        let prepared: Provenance =
            serde_json::from_value(io::read_json(&directory.join("provenance.json"))?)?;
        require(
            crate::REPOSITORIES.contains(&prepared.repository.as_str())
                && prepared.version == self.version
                && artifact::hex(&prepared.commit, 40)
                && commit.is_none_or(|c| c == prepared.commit)
                && prepared.workflow_run > 0
                && prepared.workflow_attempt > 0
                && prepared.artifact_id > 0
                && prepared
                    .artifact_digest
                    .strip_prefix("sha256:")
                    .is_some_and(|h| artifact::hex(h, 64))
                && artifact::hex(&prepared.build_sha256, 64)
                && artifact::hex(&prepared.attestation_sha256, 64)
                && artifact::hex(&prepared.runtime_digest, 64)
                && artifact::hex(&prepared.archive_sha256, 64),
            "Prepared provenance does not match this release",
        )?;
        require(
            fs::read_to_string(directory.join("SHA256SUMS"))? == checksums(directory, &names[..5])?
                && artifact::file_hash(&directory.join(self.archive()))? == prepared.archive_sha256,
            "Prepared release bytes changed; preserve this directory and inspect it",
        )?;
        let mut value = io::read_json(&directory.join("release.json"))?;
        let manifest: artifact::Manifest = serde_json::from_value(value.clone())?;
        value
            .as_object_mut()
            .ok_or("Invalid manifest")?
            .shift_remove("digest");
        require(
            manifest.format == 3
                && manifest.schema == 3
                && manifest.runtime == "rust"
                && manifest.version == self.version
                && manifest.digest == prepared.runtime_digest
                && artifact::hash(&serde_json::to_vec(&value)?) == manifest.digest,
            "Prepared manifest differs from provenance",
        )?;
        require(
            self.journal()?
                .commit
                .as_ref()
                .is_none_or(|c| *c == prepared.commit),
            "Prepared release commit changed",
        )?;
        self.app_assets(directory)?;
        Ok(prepared)
    }
    pub(super) fn prepare(&self, commit: &str) -> Result<Provenance> {
        if self.output.try_exists()? {
            return self.prepared(Some(commit)).map_err(|error| format!(
                "Existing preparation {} is incomplete or changed: {error}. Preserve it for inspection",
                self.output.display()
            ).into());
        }
        io::private_directory(&self.directory)?;
        let last = self.released_app()?;
        let relay = self.relay_version(commit, last.as_ref())?;
        self.full_suite(commit, &relay)?;
        // The release's own run of the checks published its build, with Production's app.
        let run = self.checked_source(commit)?;
        let records = io::github(
            self.system,
            &format!("actions/runs/{}/artifacts?per_page=100", run["id"]),
        )?;
        let artifacts: Vec<_> = records["artifacts"]
            .as_array()
            .ok_or("Missing main artifacts")?
            .iter()
            .filter(|a| a["name"] == format!("dispatch-main-{commit}") && a["expired"] == false)
            .collect();
        require(artifacts.len() == 1, "Verified main artifact unavailable")?;
        // A failed download never creates the final directory. A killed process
        // may leave a private staging directory; retries leave it untouched.
        let staging = tempfile::Builder::new()
            .prefix(&format!(".prepare-{}-", self.tag))
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&self.directory)?;
        let download = tempfile::tempdir_in(staging.path())?;
        releases::download_run(self.system, artifacts[0], download.path(), commit)?;
        self.command(
            &[
                "gh",
                "attestation",
                "download",
                download
                    .path()
                    .join("build.tar.gz")
                    .to_str()
                    .ok_or("Invalid downloaded package path")?,
                "--repo",
                REPOSITORY,
            ],
            Some(download.path()),
            120,
        )?;
        let build_sha256 = artifact::file_hash(&download.path().join("build.tar.gz"))?;
        let downloaded_attestation = download.path().join(format!("sha256:{build_sha256}.jsonl"));
        regular(&downloaded_attestation)?;
        self.command(
            &[
                "gh",
                "attestation",
                "verify",
                download
                    .path()
                    .join("build.tar.gz")
                    .to_str()
                    .ok_or("Invalid downloaded package path")?,
                "--bundle",
                downloaded_attestation
                    .to_str()
                    .ok_or("Invalid attestation path")?,
                "--repo",
                REPOSITORY,
                "--signer-workflow",
                &format!("{REPOSITORY}/.github/workflows/checks.yml"),
                "--source-digest",
                commit,
                "--deny-self-hosted-runners",
            ],
            None,
            120,
        )?;
        // CI built the version main's source names; the release publishes those bytes under
        // its own version, and only once Dev runs the commit. Dev runs main's build of it,
        // which Production's app alone makes unlike this one, so Dev is held to the commit its
        // build names rather than the package's digest.
        let candidate = download.path().join("candidate");
        let health: Value = serde_json::from_slice(&self.fetch(&self.origins.dev, "/api/health")?)?;
        require(
            health["source"]["commit"] == commit,
            &format!(
                "Dev is not serving this commit's build; wait for Dev to install {commit}, or release the commit Dev serves"
            ),
        )?;
        let app = relay::packed_app(&candidate, commit, &relay, last.as_ref())?;
        let manifest = artifact::stamp(&candidate, commit, &self.version)?;
        artifact::pack(&candidate, &staging.path().join(self.archive()))?;
        fs::copy(
            download.path().join("build.tar.gz"),
            staging.path().join("dispatch-build.tar.gz"),
        )?;
        fs::copy(
            downloaded_attestation,
            staging.path().join("attestation.sigstore.jsonl"),
        )?;
        fs::copy(
            candidate.join("release.json"),
            staging.path().join("release.json"),
        )?;
        relay::stage(&candidate, staging.path(), app.as_ref())?;
        let prepared = Provenance {
            repository: REPOSITORY.into(),
            commit: commit.into(),
            version: self.version.clone(),
            workflow_run: run["id"].as_u64().ok_or("Invalid workflow id")?,
            workflow_attempt: run["run_attempt"]
                .as_u64()
                .ok_or("Invalid workflow attempt")?,
            artifact_id: artifacts[0]["id"].as_u64().ok_or("Invalid artifact id")?,
            artifact_digest: io::text(artifacts[0], "digest"),
            build_sha256,
            attestation_sha256: artifact::file_hash(
                &staging.path().join("attestation.sigstore.jsonl"),
            )?,
            runtime_digest: manifest.digest,
            archive_sha256: artifact::file_hash(&staging.path().join(self.archive()))?,
        };
        io::write_json(
            &staging.path().join("provenance.json"),
            &serde_json::to_value(&prepared)?,
        )?;
        fs::write(
            staging.path().join("SHA256SUMS"),
            checksums(staging.path(), &self.runtime_assets()[..5])?,
        )?;
        download.close()?;
        self.verify_preparation(staging.path(), Some(commit))?;
        for name in self.asset_names(staging.path())? {
            File::open(staging.path().join(name))?.sync_all()?;
        }
        File::open(staging.path())?.sync_all()?;
        crate::management::rename(
            staging.path(),
            &self.output,
            crate::management::Rename::NoReplace,
        )?;
        File::open(&self.directory)?.sync_all()?;
        say(format!(
            "Prepared {} from checked main commit {commit}: {}",
            self.version, prepared.runtime_digest
        ));
        Ok(prepared)
    }
    /// Unpacks the prepared runtime into `build`, checked to be the one prepared, answering
    /// where its backend is, ready to run.
    pub(super) fn unpack_prepared(&self, prepared: &Provenance, build: &Path) -> Result<PathBuf> {
        artifact::unpack(&self.output.join(self.archive()), build)?;
        let manifest = artifact::verify(build, Some(&prepared.commit))?;
        require(
            manifest.digest == prepared.runtime_digest && manifest.version == self.version,
            "Archive differs from prepared manifest",
        )?;
        let backend = build.join("services/rust/dispatch-backend");
        fs::set_permissions(&backend, fs::Permissions::from_mode(0o700))?;
        Ok(backend)
    }
    fn smoke_receipt(&self, prepared: &Provenance) -> Value {
        json!({"commit":prepared.commit,"runtimeDigest":prepared.runtime_digest,"archiveSha256":prepared.archive_sha256})
    }
    pub(super) fn smoke(&self, prepared: &Provenance) -> Result<()> {
        self.prepared(Some(&prepared.commit))?;
        let receipt = self.output.join("smoke-verification.json");
        let expected = self.smoke_receipt(prepared);
        if receipt.try_exists()? && io::read_json(&receipt)? == expected {
            return Ok(());
        }
        let temporary = tempfile::Builder::new()
            .prefix("dispatch-release-smoke-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()?;
        // The check runs from the commit's own source, with the Node packages its lockfile
        // names installed beside it: the checkout has none, and only its updater changes it.
        let source = temporary.path().join("source");
        let exported = temporary.path().join("source.tar");
        let exported_path = exported.to_str().ok_or("Invalid export path")?;
        self.command(
            &[
                "git",
                "archive",
                "--output",
                exported_path,
                &prepared.commit,
            ],
            None,
            120,
        )?;
        fs::create_dir(&source)?;
        self.command(
            &[
                "tar",
                "-xf",
                exported_path,
                "-C",
                source.to_str().ok_or("Invalid source path")?,
            ],
            None,
            120,
        )?;
        self.command(
            &["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"],
            Some(&source),
            600,
        )?;
        let tsx = source.join("node_modules/.bin/tsx");
        require(
            tsx.is_file(),
            "The release commit's Node packages hold no tsx for the smoke check",
        )?;
        self.unpack_prepared(prepared, &temporary.path().join(".build"))?;
        self.command(
            &[
                tsx.to_str().ok_or("Invalid Node path")?,
                source
                    .join("tooling/testing/browser-check.ts")
                    .to_str()
                    .ok_or("Invalid smoke path")?,
                "--smoke-only",
            ],
            Some(temporary.path()),
            300,
        )?;
        io::write_json(&receipt, &expected)
    }
    fn verify_assets(&self, release: &Value, allow_missing: bool) -> Result<Vec<String>> {
        let names = self.asset_names(&self.output)?;
        let assets = release["assets"]
            .as_array()
            .ok_or("Missing GitHub assets")?;
        let mut found = BTreeSet::new();
        for asset in assets {
            let name = asset["name"].as_str().ok_or("Invalid asset name")?;
            require(
                names.iter().any(|n| n == name) && found.insert(name),
                "Unexpected or duplicated release asset",
            )?;
            let local = self.output.join(name);
            require(
                asset["state"] == "uploaded"
                    && asset["size"].as_u64() == Some(local.metadata()?.len())
                    && asset["digest"] == format!("sha256:{}", artifact::file_hash(&local)?),
                &format!(
                    "{name} differs from prepared bytes or its upload is incomplete; existing assets are never overwritten"
                ),
            )?;
        }
        let missing: Vec<_> = names
            .into_iter()
            .filter(|n| !found.contains(n.as_str()))
            .collect();
        require(
            allow_missing || missing.is_empty(),
            "Release assets are missing",
        )?;
        Ok(missing)
    }
    fn tag_commit(&self) -> Result<Option<String>> {
        let refs = self.api(&format!("git/matching-refs/tags/{}", self.tag), &[])?;
        let refs: Vec<_> = refs
            .as_array()
            .ok_or("Invalid tag refs")?
            .iter()
            .filter(|r| r["ref"] == format!("refs/tags/{}", self.tag))
            .collect();
        require(refs.len() <= 1, "Duplicate release tag")?;
        let Some(reference) = refs.first() else {
            return Ok(None);
        };
        let mut object = reference["object"].clone();
        for _ in 0..5 {
            if object["type"] == "commit" {
                break;
            }
            require(
                object["type"] == "tag" && artifact::hex(&io::text(&object, "sha"), 40),
                "Invalid release tag",
            )?;
            object =
                self.api(&format!("git/tags/{}", io::text(&object, "sha")), &[])?["object"].clone();
        }
        let commit = io::text(&object, "sha");
        require(
            object["type"] == "commit" && artifact::hex(&commit, 40),
            "Invalid release tag commit",
        )?;
        Ok(Some(commit))
    }
    fn verify_identity(&self, release: &Value, prepared: &Provenance) -> Result<()> {
        require(
            release["tag_name"] == self.tag && release["prerelease"] == false,
            "Expected stable release",
        )?;
        let tag = self.tag_commit()?;
        if release["draft"] == true {
            require(
                release["target_commitish"] == prepared.commit
                    && tag.as_ref().is_none_or(|c| *c == prepared.commit),
                "Draft targets another commit",
            )?;
        } else {
            require(
                releases::release_version(release)? == self.version
                    && tag.as_deref() == Some(&prepared.commit),
                "Published tag differs from prepared commit",
            )?;
        }
        Ok(())
    }
    fn wait_release(&self) -> Result<Value> {
        let deadline = self.system.monotonic() + Duration::from_secs(60);
        loop {
            if let Some(release) = self.listed_release()? {
                return Ok(release);
            }
            require(
                self.system.monotonic() < deadline,
                "Release is not yet visible; rerun to inspect it",
            )?;
            self.system.sleep(Duration::from_secs(3));
        }
    }
    pub(super) fn ensure_draft(&self, prepared: &Provenance) -> Result<Value> {
        self.prepared(Some(&prepared.commit))?;
        self.checked_source(&prepared.commit)?;
        let release = match self.listed_release()? {
            Some(release) => release,
            None => {
                if !self.notes.try_exists()? || fs::read_to_string(&self.notes)?.trim().is_empty() {
                    self.generate_notes(&prepared.commit)?;
                    say("Generated the release notes from the merged PRs");
                }
                say(format!("Release notes: {}", self.notes.display()));
                let deadline = self.system.monotonic() + Duration::from_secs(1800);
                while !self.notes.try_exists()?
                    || fs::read_to_string(&self.notes)?.trim().is_empty()
                {
                    require(
                        self.system.monotonic() < deadline,
                        "Release notes are still missing; write them and rerun",
                    )?;
                    self.system.sleep(Duration::from_secs(5));
                }
                // Keep a durable copy before creating the draft. Missing uploads
                // are added on retry; a conflicting existing upload stops it.
                let notes = fs::read(&self.notes)?;
                let mut saved = tempfile::NamedTempFile::new_in(&self.output)?;
                saved.write_all(&notes)?;
                saved.as_file().sync_all()?;
                saved.persist(self.output.join("notes.md"))?;
                File::open(&self.output)?.sync_all()?;
                self.command(
                    &[
                        "gh",
                        "release",
                        "create",
                        &self.tag,
                        "--repo",
                        REPOSITORY,
                        "--draft",
                        "--target",
                        &prepared.commit,
                        "--title",
                        &format!("Dispatch {}", self.version),
                        "--notes-file",
                        self.output
                            .join("notes.md")
                            .to_str()
                            .ok_or("Invalid notes path")?,
                    ],
                    None,
                    600,
                )?;
                self.wait_release()?
            }
        };
        require(release["draft"] == true, "Release is already published")?;
        self.verify_identity(&release, prepared)?;
        let missing = self.verify_assets(&release, true)?;
        for name in missing {
            self.command(
                &[
                    "gh",
                    "release",
                    "upload",
                    &self.tag,
                    "--repo",
                    REPOSITORY,
                    self.output
                        .join(name)
                        .to_str()
                        .ok_or("Invalid asset path")?,
                ],
                None,
                600,
            )?;
        }
        let release = self.wait_release()?;
        require(
            release["draft"] == true,
            "Release was published while preparing",
        )?;
        self.verify_identity(&release, prepared)?;
        self.verify_assets(&release, false)?;
        io::write_json(
            &self.output.join("draft-verification.json"),
            &json!({"releaseId":release["id"],"assetsVerified":true,"commit":prepared.commit}),
        )?;
        say(format!(
            "Verified draft: {}",
            io::text(&release, "html_url")
        ));
        Ok(release)
    }
    /// Publishes the verified draft, answering the release and the backup this run took.
    pub(super) fn publish(&self, prepared: &Provenance) -> Result<(Value, Option<Value>)> {
        let release = self.listed_release()?.ok_or("No draft exists to publish")?;
        self.verify_identity(&release, prepared)?;
        if release["draft"] == false {
            self.verify_assets(&release, false)?;
            return Ok((release, None));
        }
        let needs_backup = self.require_backup(&prepared.commit)?;
        self.checked_source(&prepared.commit)?;
        self.smoke(prepared)?;
        let release = self.ensure_draft(prepared)?;
        // Recheck after uploads/smoke: a newer failed or pending run must stop publication,
        // as must a release published since this one was prepared that its app no longer follows.
        self.checked_source(&prepared.commit)?;
        self.app_follows()?;
        let needs_rehearsal = !self.new_migrations(&prepared.commit)?.is_empty();
        if needs_rehearsal {
            self.dev_preflight()?;
        }
        self.check_production()?;
        // Only once Production can take the release, so stopping it for the copy never
        // interrupts a collection, fills a short disk or lands on a failing updater.
        let backup = needs_backup.then(|| self.backup()).transpose()?;
        self.rehearse(prepared)?;
        self.checked_source(&prepared.commit)?;
        self.app_follows()?;
        self.revalidate_rehearsal(prepared)?;
        // Both startup and the final export can take minutes. Readiness belongs after
        // all of them; only local candidate/config/freshness validation follows it.
        self.check_production()?;
        self.prepared(Some(&prepared.commit))?;
        if needs_rehearsal {
            self.ready_rehearsal(prepared)?;
        }
        self.api(
            &format!("releases/{}", release["id"]),
            &[
                "--method",
                "PATCH",
                "-F",
                "draft=false",
                "-f",
                "make_latest=true",
            ],
        )?;
        let release = self.wait_release()?;
        require(
            release["draft"] == false,
            "Publication is not yet visible; rerun to inspect it",
        )?;
        self.verify_identity(&release, prepared)?;
        self.verify_assets(&release, false)?;
        say(format!("Published {}", io::text(&release, "html_url")));
        Ok((release, backup))
    }
    fn fetch(&self, origin: &str, path: &str) -> Result<Vec<u8>> {
        let response = self
            .system
            .request(&format!("{origin}{path}"), false, true, 20)?;
        require(
            response.status == 200,
            &format!("{origin} returned an unsuccessful response"),
        )?;
        io::bounded_body(response.body)?
            .filter(|bytes| !bytes.is_empty())
            .ok_or_else(|| format!("{origin} response empty or oversized").into())
    }
    pub(super) fn verify_production(&self, prepared: &Provenance, release: &Value) -> Result<()> {
        say("Waiting for Production to install the release");
        let deadline = self.system.monotonic() + Duration::from_secs(600);
        let health = loop {
            if let Ok(bytes) = self.fetch(&self.origins.production, "/api/health")
                && let Ok(health) = serde_json::from_slice::<Value>(&bytes)
                && health["release"] == prepared.runtime_digest
                && health["status"] == "ready"
                && health["environment"] == "production"
            {
                break health;
            }
            require(
                self.system.monotonic() < deadline,
                "Production did not report the release digest; inspect its updater with read-only access, then rerun",
            )?;
            self.system.sleep(Duration::from_secs(5));
        };
        let html = String::from_utf8(self.fetch(&self.origins.production, "/")?)?;
        let pattern = regex::Regex::new(r#"(?:src|href)="(\.?/assets/[^\"]+)""#)?;
        let assets: BTreeSet<_> = pattern
            .captures_iter(&html)
            .map(|c| c[1].to_owned())
            .collect();
        require(
            assets.len() >= 2,
            "Production dashboard is missing its assets",
        )?;
        for asset in &assets {
            self.fetch(
                &self.origins.production,
                &format!("/{}", asset.trim_start_matches(['.', '/'])),
            )?;
        }
        io::write_json(
            &self.output.join("deployment-verification.json"),
            &json!({
            "version":self.version,"commit":prepared.commit,"runtimeDigest":prepared.runtime_digest,
            "publicHealth":health,"publicAssetsVerified":assets.len(),"mainCheckRun":prepared.workflow_run,
            "release":release["html_url"]}),
        )
    }
}
