use super::{Environment, Updater};
use crate::{Result, artifact, io, management, releases, require};
use serde_json::{Value, json};
use std::time::Duration;

/// How many attempts in a row, ten seconds apart on Dev's timer, fail before waiting for Dev
/// stops: past one that a moment's network trouble explains.
const FAILED_ATTEMPTS: u64 = 3;

impl Updater<'_> {
    /// Wait until Dev runs `commit`, or a later commit of its branch that contains it, and
    /// confirm it is live: recorded ready, the checkout clean on it, the recorded runtime and
    /// its host tool installed, the service active and healthy.
    pub fn wait(&self, commit: &str, timeout: u64) -> Result<Value> {
        require(
            self.environment == Environment::Dev,
            "Only Dev follows commits",
        )?;
        require(artifact::hex(commit, 40), "Full commit required")?;
        let deadline = self.system.monotonic() + Duration::from_secs(timeout);
        loop {
            let last = match self.live(commit) {
                Ok(Some(live)) => return Ok(live),
                Ok(None) => io::read_json(&self.status_file)
                    .map_or_else(|error| error.to_string(), |status| status.to_string()),
                Err(error) => error.to_string(),
            };
            // An update that fails attempt after attempt won't get through by waiting.
            if let Ok(failure) = io::read_json(&self.failure_file)
                && failure["attempts"].as_u64().unwrap_or(0) >= FAILED_ATTEMPTS
            {
                return Err(format!(
                    "Dev cannot install {commit}: its updates have failed since {}: {}",
                    io::text(&failure, "since"),
                    io::text(&failure, "error")
                )
                .into());
            }
            if self.system.monotonic() >= deadline {
                return Err(
                    format!("Dev did not install {commit} within {timeout}s: {last}").into(),
                );
            }
            self.system.sleep(Duration::from_secs(5));
        }
    }
    fn live(&self, commit: &str) -> Result<Option<Value>> {
        let status = io::read_json(&self.status_file)?;
        let installed = io::text(&status, "commit");
        // Waiting on a later commit's checks, Dev still serves the commit its status names.
        let serving = ["ready", "waiting_for_checks", "checks_failed"]
            .iter()
            .any(|state| status["status"] == *state);
        if !serving
            || !artifact::hex(&installed, 40)
            || self
                .git(&["merge-base", "--is-ancestor", commit, &installed])
                .is_err()
        {
            return Ok(None);
        }
        let manifest = self.verify()?;
        require(
            self.git(&["rev-parse", "HEAD"])? == installed,
            "Checkout differs from the installed commit",
        )?;
        require(
            status["digest"] == manifest.digest,
            "Runtime differs from the one the updater installed",
        )?;
        // Dev swaps in the runtime's host tool a moment after it reports ready, and what runs
        // next, such as a release, would otherwise run the previous one.
        require(
            !management::drift(self)?,
            "Host tool differs from the installed runtime",
        )?;
        self.service("is-active")?;
        require(
            self.healthy(&manifest.digest, 10),
            "Dev is not healthy on its runtime",
        )?;
        Ok(Some(
            json!({"commit":installed,"digest":manifest.digest,"status":"live"}),
        ))
    }
    pub(super) fn update_dev(&self) -> Result<()> {
        self.clean_checkout()?;
        let branch = self.tracked_branch()?;
        let tracked = format!("origin/{branch}");
        self.git(&["fetch", "origin", branch])?;
        let head = self.git(&["rev-parse", &tracked])?;
        let current = self.git(&["rev-parse", "HEAD"])?;
        if head == current {
            return Ok(());
        }
        let current_manifest = artifact::verify(&self.active, Some(&current))?;
        let waiting = |run: &Value| {
            self.status(
                if run["status"] == "completed" {
                    "checks_failed"
                } else {
                    "waiting_for_checks"
                },
                &current_manifest,
                json!({"commit":current}),
            )
        };
        let runs = self.push_runs(branch)?;
        let Some((commit, run, record)) = self.newest_build(&runs, branch, &current)? else {
            let head_run =
                releases::latest_run(&runs["workflow_runs"], &head, "push", Some(branch), false);
            return waiting(head_run.unwrap_or(&Value::Null));
        };
        let temp = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(&self.runtime)?;
        releases::download_run(self.system, &record, temp.path(), &commit, None)?;
        // The branch may have moved on during the download; one rewritten to leave the commit
        // out is what activation refuses.
        self.git(&["fetch", "origin", branch])?;
        // A run can fail, or be run again, during a download, and its build goes with it.
        match self.published(&self.push_runs(branch)?, branch, &commit)? {
            Some((_, again)) if again == record => {}
            _ => return waiting(&Value::Null),
        }
        self.activate(&temp.path().join("candidate"), &commit)?;
        // Only after an install: hashing the manager and the runtime on every tick cost
        // more than the rest of the check together.
        management::refresh(self)?;
        eprintln!(
            "Installed {commit} from the {} run {}",
            io::text(&run, "event"),
            run["id"]
        );
        Ok(())
    }
    /// The branch's latest push runs of the checks, newest first, whatever they have come to:
    /// one request, however many commits Dev has yet to install. A commit whose run is older
    /// than these waits for a newer one to install.
    fn push_runs(&self, branch: &str) -> Result<Value> {
        io::github(
            self.system,
            &format!("actions/workflows/checks.yml/runs?event=push&branch={branch}&per_page=20"),
        )
    }
    /// The newest commit of `branch` after `current` with a published build, its run and that
    /// build: the branch's newest need not be it, so a merge doesn't wait for the checks of
    /// those that follow it.
    fn newest_build(
        &self,
        runs: &Value,
        branch: &str,
        current: &str,
    ) -> Result<Option<(String, Value, Value)>> {
        let newest = self.git(&["rev-list", "--first-parent", &format!("origin/{branch}")])?;
        for commit in newest.lines().take_while(|commit| *commit != current) {
            if let Some((run, record)) = self.published(runs, branch, commit)? {
                return Ok(Some((commit.to_owned(), run, record)));
            }
        }
        Ok(None)
    }
    /// The latest push run of `commit` on `branch` among `runs` and the build it published. The
    /// push run tests exactly the commit the branch moved to, and its `publish` job publishes
    /// the build once every suite but the browser one passed, so Dev installs it while that
    /// suite still runs; a commit without a run, or a build yet, waits, and a failed run's
    /// build stays where it is.
    fn published(
        &self,
        runs: &Value,
        branch: &str,
        commit: &str,
    ) -> Result<Option<(Value, Value)>> {
        let Some(run) =
            releases::latest_run(&runs["workflow_runs"], commit, "push", Some(branch), false)
        else {
            return Ok(None);
        };
        let passed = releases::passed(run);
        // A run going again still holds the build of the attempt before it, the one that may
        // have failed: only once it passed is that build its own.
        let first = run["status"] != "completed" && run["run_attempt"].as_u64().unwrap_or(1) == 1;
        if !passed && !first {
            return Ok(None);
        }
        let Some(record) = self.build(run, &format!("dispatch-{branch}-{commit}"))? else {
            // A passed run published its build: nothing else can supply one that is gone.
            require(!passed, "Verified Dev build unavailable")?;
            return Ok(None);
        };
        // In a run still going the build's name says nothing of who uploaded it. Its publish
        // job's success does: the job fails when another took the name first.
        if !passed && !self.job_passed(run, "publish")? {
            return Ok(None);
        }
        Ok(Some((run.clone(), record)))
    }
    /// Whether the job `name` of `run` ended in success.
    fn job_passed(&self, run: &Value, name: &str) -> Result<bool> {
        let id = run["id"].as_u64().ok_or("Invalid workflow id")?;
        let jobs = io::github(self.system, &format!("actions/runs/{id}/jobs?per_page=100"))?;
        Ok(jobs["jobs"]
            .as_array()
            .ok_or("Missing jobs")?
            .iter()
            .any(|job| job["name"] == name && job["conclusion"] == "success"))
    }
    /// The unexpired build `name` that `run` published.
    fn build(&self, run: &Value, name: &str) -> Result<Option<Value>> {
        let id = run["id"].as_u64().ok_or("Invalid workflow id")?;
        let artifacts = io::github(self.system, &format!("actions/runs/{id}/artifacts"))?;
        let matches: Vec<_> = artifacts["artifacts"]
            .as_array()
            .ok_or("Missing artifacts")?
            .iter()
            .filter(|a| a["name"] == name && a["expired"] == false)
            .collect();
        require(matches.len() <= 1, "Verified Dev artifact is ambiguous")?;
        Ok(matches.first().map(|record| (*record).clone()))
    }
}
