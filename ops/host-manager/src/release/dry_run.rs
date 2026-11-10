//! A dry run rehearses what only a release runs, for the commit `publish` would release: its
//! own checks, which build or carry Production's DSP Dispatch: Relay, the package they publish,
//! the smoke check and a GitHub draft holding every asset, verified. Then it deletes the draft,
//! its checks branch and its files, whatever happened. It never publishes, tags or touches
//! Production, and keeps out of the release's own journal and folder, so the release that
//! follows starts as if it never ran.
//!
//! Its draft and checks branch are named for the release's tag with `-dry-` and the commit's
//! first eight digits after it, as `v0.0.36-dry-0123abcd`: no stable version, so a release
//! never takes it for one of its own, and as a draft it is never GitHub's latest release, the
//! one Production installs.
use super::*;

impl Release<'_> {
    /// The dry run of this release at `commit`, under its own tag and checks branch, with its
    /// journal and files in `dry-run/` beside the releases.
    fn dry_release(&self, commit: &str) -> Self {
        let tag = format!("{}-dry-{}", self.tag, &commit[..8]);
        let directory =
            self.directory
                .join("dry-run")
                .join(format!("{}-{}", self.version, &commit[..8]));
        Self {
            checks_branch: format!("release-checks/{tag}"),
            tag,
            bump: self.bump.clone(),
            ..Self::at(
                self.system,
                &self.root,
                &directory,
                &self.origins,
                &self.version,
            )
        }
    }
    /// `release dry-run`: the release-only path for main's head, or the commit `--commit`
    /// names, then the dry run's draft, checks branch and files deleted, even when it failed.
    pub(super) fn dry_run(&self, requested: Option<&str>) -> Result<Value> {
        let started = self.system.monotonic();
        self.git(&["fetch", "--tags", "origin", "main"], None, 300)?;
        let commit = self.resolve(requested)?;
        require(artifact::hex(&commit, 40), "Full release commit required")?;
        let dry = self.dry_release(&commit);
        say(format!(
            "Dry run of {} at {commit}: what only a release runs, without publishing",
            self.version
        ));
        // A dry run of the same commit that was stopped may have left what it made.
        dry.discard_dry_run()?;
        let ran = dry.release_only_path(&commit);
        let discarded = dry.discard_dry_run();
        if discarded.is_ok() {
            say(format!(
                "Cleaned up the dry run: its draft {}, checks branch {} and {} are gone",
                dry.tag,
                dry.checks_branch,
                dry.directory.display()
            ));
        }
        let mut result = match (ran, discarded) {
            (Ok(result), Ok(())) => result,
            (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
            (Err(error), Err(left)) => return Err(format!("{error}\n{left}").into()),
        };
        let elapsed = (self.system.monotonic() - started).as_secs();
        result["elapsedSeconds"] = json!(elapsed);
        say(format!(
            "Dry run of {} passed in {}m {:02}s",
            self.version,
            elapsed / 60,
            elapsed % 60
        ));
        Ok(result)
    }
    /// What `publish` runs for `commit` before it publishes, up to its verified draft.
    fn release_only_path(&self, commit: &str) -> Result<Value> {
        io::private_directory(&self.directory)?;
        let commit = self.pin(Some(commit))?;
        let prepared = self.prepare(&commit)?;
        self.smoke(&prepared)?;
        let draft = self.ensure_draft(&prepared)?;
        let checks = self.checked_source(&commit)?;
        let assets: Vec<_> = draft["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|asset| io::text(asset, "name"))
            .collect();
        Ok(
            json!({"version":self.version,"stage":"dry-run","commit":commit,
            "checks":checks["html_url"],"relayVersion":self.journal()?.relay_version,
            "verified":{"runtimeDigest":prepared.runtime_digest,"smoke":true,"draftAssets":assets}}),
        )
    }
    /// Deletes the dry run's GitHub drafts, checks branch and folder, whichever are there,
    /// each whatever became of the others.
    fn discard_dry_run(&self) -> Result<()> {
        let problems: Vec<_> = [
            self.delete_dry_drafts(),
            self.remove_checks_branch(),
            self.remove_dry_files(),
        ]
        .into_iter()
        .filter_map(|done| done.err().map(|error| error.to_string()))
        .collect();
        require(
            problems.is_empty(),
            &format!(
                "The dry run couldn't delete all it made: {}. Delete what's left of {} by hand",
                problems.join("; "),
                self.tag
            ),
        )
    }
    fn delete_dry_drafts(&self) -> Result<()> {
        for release in all_releases(self.system)? {
            if release["tag_name"] != self.tag {
                continue;
            }
            require(
                release["draft"] == true,
                &format!(
                    "{} was published, though a dry run never publishes",
                    self.tag
                ),
            )?;
            self.delete(&format!("releases/{}", release["id"]))?;
        }
        Ok(())
    }
    fn remove_dry_files(&self) -> Result<()> {
        if self.directory.try_exists()? {
            fs::remove_dir_all(&self.directory)?;
        }
        Ok(())
    }
}
