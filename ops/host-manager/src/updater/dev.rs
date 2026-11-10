use super::{Environment, Updater};
use crate::{Result, artifact, io, management, releases, require};
use serde_json::{Value, json};
use std::time::Duration;

/// How many attempts in a row, ten seconds apart on Dev's timer, fail before waiting for Dev
/// stops: past one that a moment's network trouble explains.
const FAILED_ATTEMPTS: u64 = 3;
/// How many of the branch's newest push runs a tick reads: more than the commits that land
/// while one run decides.
const RUNS: usize = 40;
/// What the checks say about a commit Dev could install.
enum Checked {
    /// A passed run and the build it published.
    Passed(Value, Value),
    /// The run still deciding, or the one that failed, if there is one.
    Pending(Value),
}

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
        if status["status"] != "ready"
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
        let current = self.git(&["rev-parse", "HEAD"])?;
        let ahead = self.git(&[
            "rev-list",
            "--first-parent",
            &format!("{current}..{tracked}"),
        ])?;
        if ahead.is_empty() {
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
        // The newest commit whose run passed installs, without waiting for those merged after
        // it: each has a run of its own, and one still deciding would hold back every commit
        // before it. Until one passes, the status is the newest's.
        let runs = self.push_runs(branch)?;
        let mut newest = None;
        let mut passed = None;
        for commit in ahead.lines() {
            match self.validated_build(&runs, branch, commit)? {
                Checked::Passed(run, record) => {
                    passed = Some((commit, run, record));
                    break;
                }
                Checked::Pending(run) => {
                    newest.get_or_insert(run);
                }
            }
        }
        let Some((commit, run, record)) = passed else {
            return waiting(&newest.unwrap_or(Value::Null));
        };
        let temp = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(&self.runtime)?;
        releases::download_run(self.system, &record, temp.path(), commit, None)?;
        self.git(&["fetch", "origin", branch])?;
        if self
            .git(&["merge-base", "--is-ancestor", commit, &tracked])
            .is_err()
        {
            return waiting(&Value::Null);
        }
        // A rerun can revoke validation during a download.
        match self.validated_build(&self.push_runs(branch)?, branch, commit)? {
            Checked::Passed(again, _) if again == run => {}
            _ => return waiting(&Value::Null),
        }
        self.activate(&temp.path().join("candidate"), commit)?;
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
    /// The newest push runs of `branch`.
    fn push_runs(&self, branch: &str) -> Result<Value> {
        io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=push&branch={branch}&per_page={RUNS}"
            ),
        )
    }
    /// The passed push run of `commit` on `branch` among `runs` and the build it published, or
    /// the run still deciding. The push run tests exactly the commit the branch moved to, so its
    /// run decides; a commit without one waits.
    fn validated_build(&self, runs: &Value, branch: &str, commit: &str) -> Result<Checked> {
        let Some(run) =
            releases::latest_run(&runs["workflow_runs"], commit, "push", Some(branch), false)
        else {
            return Ok(Checked::Pending(Value::Null));
        };
        if !releases::passed(run) {
            return Ok(Checked::Pending(run.clone()));
        }
        let record = self
            .build(run, &format!("dispatch-{branch}-{commit}"))?
            .ok_or("Verified Dev build unavailable")?;
        Ok(Checked::Passed(run.clone(), record))
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
