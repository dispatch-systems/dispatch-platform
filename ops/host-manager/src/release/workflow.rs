use super::{Release, all_releases, latest_version, say};
use crate::{REPOSITORY, Result, artifact, io, releases, require};
use serde_json::Value;
use std::{fs, path::Path, time::Duration};

/// How long a release waits for main's run of its commit to end.
const MAIN_RUN_ENDS: u64 = 1800;

impl Release<'_> {
    pub(super) fn git(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<String> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        self.command(&command, cwd, timeout)
    }
    /// The newest published stable release before this one.
    pub(super) fn previous(&self) -> Result<Option<String>> {
        let listed: Vec<_> = all_releases(self.system)?
            .into_iter()
            .filter(|r| r["tag_name"] != self.tag)
            .collect();
        let latest = latest_version(&listed);
        Ok((latest != "0.0.0").then(|| format!("v{latest}")))
    }
    /// The commit `--commit` names, or main's head.
    pub(super) fn resolve(&self, requested: Option<&str>) -> Result<String> {
        self.git(
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", requested.unwrap_or("origin/main")),
            ],
            None,
            120,
        )
    }
    /// The commit this release publishes: main's head, or the commit on main `--commit`
    /// names. A resumed release keeps its pin; `--commit` replaces it only while nothing
    /// has been prepared from it, such as after its full checks failed.
    pub(super) fn pin(&self, requested: Option<&str>) -> Result<String> {
        let mut journal = self.journal()?;
        let commit = match (&journal.commit, requested) {
            (Some(pinned), None) => pinned.clone(),
            (pinned, requested) => {
                let commit = self.resolve(requested)?;
                if let Some(pinned) = pinned
                    && *pinned != commit
                {
                    require(
                        !self.output.try_exists()?,
                        "The release is prepared from its pinned commit; resume without --commit",
                    )?;
                    say(format!("Replacing the unprepared release commit {pinned}"));
                }
                commit
            }
        };
        require(artifact::hex(&commit, 40), "Full release commit required")?;
        self.git(
            &["merge-base", "--is-ancestor", &commit, "origin/main"],
            None,
            120,
        )
        .map_err(|_| "The release commit must be on main")?;
        if journal.commit.as_ref() != Some(&commit) {
            if let Some(previous) = self.previous()? {
                let comparison =
                    io::github(self.system, &format!("compare/{previous}...{commit}"))?;
                require(
                    comparison["status"] == "ahead",
                    &format!("Main has nothing new since {previous}"),
                )?;
                let changes = self.changes_after(&previous, &commit)?;
                if !changes.is_empty() {
                    say(format!(
                        "Included changes:\n{}",
                        changes
                            .iter()
                            .map(|(number, title)| format!("- #{number} {title}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
            }
            journal.commit = Some(commit.clone());
            // The app's version is decided for the commit it's built from.
            journal.relay_version = None;
            self.save(&journal)?;
        }
        say(format!("Releasing main commit {commit}"));
        Ok(commit)
    }
    /// Whether `run` validated the whole suite on `commit`: its `platform` gate passed, which a
    /// partial manual run skips, and its `core` job passed, which a scoped run of the former
    /// workflow skipped, or was skipped for main's push run of the commit, which passed it.
    fn full_run(&self, run: &Value, commit: &str) -> Result<bool> {
        let jobs = io::github(
            self.system,
            &format!("actions/runs/{}/jobs?filter=latest&per_page=100", run["id"]),
        )?;
        let ended = |name: &str, conclusion: &str| {
            jobs["jobs"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|job| job["name"] == name && job["conclusion"] == conclusion)
        };
        Ok(ended("platform", "success")
            && (ended("core", "success")
                || (ended("core", "skipped") && self.tested(commit)?.is_some())))
    }
    /// Main's push run of the checks on exactly `commit`, if its newest passed: every suite ran
    /// on the commit there, so the release's own run skips those that test its source. One still
    /// going is awaited: Dev serves a commit once that run published its build, before its
    /// browser suite ends, and a release that follows at once would run every suite again.
    fn tested(&self, commit: &str) -> Result<Option<Value>> {
        let deadline = self.system.monotonic() + Duration::from_secs(MAIN_RUN_ENDS);
        let mut announced = false;
        loop {
            let runs = io::github(
                self.system,
                &format!(
                    "actions/workflows/checks.yml/runs?event=push&head_sha={commit}&per_page=100"
                ),
            )?;
            let run =
                releases::latest_run(&runs["workflow_runs"], commit, "push", Some("main"), false);
            match run {
                Some(going)
                    if going["status"] != "completed" && self.system.monotonic() < deadline =>
                {
                    if !announced {
                        say(format!(
                            "Waiting for main's checks on the commit to end: {}",
                            io::text(going, "html_url")
                        ));
                        announced = true;
                    }
                    self.system.sleep(Duration::from_secs(10));
                }
                _ => return Ok(run.filter(|run| releases::passed(run)).cloned()),
            }
        }
    }
    /// The release's own run of the checks on exactly `commit`, dispatched on its temporary
    /// branch, if its newest passed the full suite, or the package's checks on main's passed
    /// suites. Main's push run alone never counts: only a release's run builds Production's app.
    pub(super) fn fully_validated(&self, commit: &str) -> Result<Option<Value>> {
        if let Some(run) = self.dispatched_run(commit)?
            && releases::passed(&run)
            && self.full_run(&run, commit)?
        {
            return Ok(Some(run));
        }
        Ok(None)
    }
    /// The newest run of the checks dispatched on the release's temporary branch at exactly
    /// `commit`.
    fn dispatched_run(&self, commit: &str) -> Result<Option<Value>> {
        let runs = io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={commit}&per_page=100"
            ),
        )?;
        Ok(releases::latest_run(
            &runs["workflow_runs"],
            commit,
            "workflow_dispatch",
            Some(&self.checks_branch),
            false,
        )
        .cloned())
    }
    /// Requires the full suite on exactly `commit`, in the release's own run: dispatched on a
    /// temporary branch at the commit with `relay`, the app version it builds (empty to carry
    /// the last released app), and awaited. Once main's push run of the commit passed, the
    /// release's run names it and builds, packs and checks the package alone. A rerun awaits the
    /// run already dispatched.
    pub(super) fn full_suite(&self, commit: &str, relay: &str) -> Result<()> {
        let url = self.dispatch_full(commit, relay)?;
        say(format!("Checks passed: {url}"));
        // The temporary branch only ever serves a dispatched run.
        self.remove_checks_branch()
    }
    fn dispatch_full(&self, commit: &str, relay: &str) -> Result<String> {
        if self.dispatched_run(commit)?.is_none() {
            let tested = self.tested(commit)?;
            let relay = format!("relay-version={relay}");
            let reused = tested
                .as_ref()
                .map(|run| format!("tested-run={}", run["id"]));
            let mut command = vec![
                "gh",
                "workflow",
                "run",
                "checks.yml",
                "--repo",
                REPOSITORY,
                "--ref",
                &self.checks_branch,
                "-f",
                &relay,
            ];
            if let Some(reused) = &reused {
                command.extend(["-f", reused]);
            }
            say(match &tested {
                Some(run) => format!(
                    "Starting the release's checks, reusing the suites main's checks passed: {}",
                    io::text(run, "html_url")
                ),
                None => "Starting the release's checks".into(),
            });
            self.checks_branch_at(commit)?;
            self.command(&command, None, 120)?;
        }
        // A failure stays for inspection; rerunning its failed jobs lets this resume.
        let run = self.checks(commit, "workflow_dispatch", Some(&self.checks_branch), true)?;
        require(
            self.full_run(&run, commit)?,
            "The dispatched checks skipped suites main's checks did not pass for the commit",
        )?;
        Ok(io::text(&run, "html_url"))
    }
    fn checks_ref(&self) -> Result<Option<String>> {
        let name = format!("refs/heads/{}", self.checks_branch);
        let refs = self.api(
            &format!("git/matching-refs/heads/{}", self.checks_branch),
            &[],
        )?;
        Ok(refs
            .as_array()
            .ok_or("Invalid branch refs")?
            .iter()
            .find(|r| r["ref"] == name)
            .map(|r| io::text(&r["object"], "sha")))
    }
    fn checks_branch_at(&self, commit: &str) -> Result<()> {
        match self.checks_ref()? {
            Some(sha) if sha == commit => {}
            Some(_) => {
                self.api(
                    &format!("git/refs/heads/{}", self.checks_branch),
                    &[
                        "--method",
                        "PATCH",
                        "-f",
                        &format!("sha={commit}"),
                        "-F",
                        "force=true",
                    ],
                )?;
            }
            None => {
                self.api(
                    "git/refs",
                    &[
                        "-f",
                        &format!("ref=refs/heads/{}", self.checks_branch),
                        "-f",
                        &format!("sha={commit}"),
                    ],
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn remove_checks_branch(&self) -> Result<()> {
        if self.checks_ref()?.is_some() {
            self.delete(&format!("git/refs/heads/{}", self.checks_branch))?;
        }
        Ok(())
    }
    pub(super) fn clean(&self) -> Result<()> {
        self.remove_checks_branch()?;
        // Preserve custom notes and any edits made after their published copy.
        if self.notes == self.directory.join(format!("{}-notes.md", self.tag))
            && self.notes.try_exists()?
            && self.output.join("notes.md").try_exists()?
            && fs::read(&self.notes)? == fs::read(self.output.join("notes.md"))?
        {
            fs::remove_file(&self.notes)?;
        }
        Ok(())
    }
    /// Sets aside a release that was never published, so another can take its place. Its
    /// GitHub draft is deleted: a draft has no tag, and Production never sees one. Its notes,
    /// prepared directory and journal move to `superseded/` rather than being deleted.
    pub(super) fn supersede(&self) -> Result<()> {
        if let Some(release) = self.listed_release()? {
            require(
                release["draft"] == true,
                "A published release is never superseded",
            )?;
            self.delete(&format!("releases/{}", release["id"]))?;
        }
        self.remove_checks_branch()?;
        let pinned = self
            .journal()?
            .commit
            .map_or("unpinned".into(), |c| c[..8].to_owned());
        let superseded = self.directory.join("superseded");
        let mut target = superseded.join(format!("{}-{pinned}", self.tag));
        for n in 2.. {
            if !target.try_exists()? {
                break;
            }
            target = superseded.join(format!("{}-{pinned}-{n}", self.tag));
        }
        io::private_directory(&target)?;
        // Only the directory and journal mark a release unpublished, so they move last: a
        // run interrupted here leaves this release for the next one to set aside.
        for path in [self.notes.clone(), self.output.clone(), self.journal_path()] {
            if path.try_exists()? {
                fs::rename(&path, target.join(path.file_name().ok_or("Invalid path")?))?;
            }
        }
        say(format!(
            "Set aside the unpublished {} in {}",
            self.tag,
            target.display()
        ));
        Ok(())
    }
}
