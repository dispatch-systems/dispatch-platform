use super::{Environment, Updater};
use crate::{Result, artifact, io, management, releases, require};
use serde_json::{Value, json};
use std::time::Duration;

/// How many attempts in a row, ten seconds apart on Dev's timer, fail before waiting for Dev
/// stops: past one that a moment's network trouble explains.
const FAILED_ATTEMPTS: u64 = 3;
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
        let (commit, run, record) = match self.validated_build(branch, &head)? {
            Checked::Passed(run, record) => (head, run, record),
            // An earlier merge whose checks passed doesn't wait for those of the ones after it.
            Checked::Pending(head_run) => match self.passed_since(branch, &current)? {
                Some(passed) => passed,
                None => return waiting(&head_run),
            },
        };
        let temp = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(&self.runtime)?;
        releases::download_run(self.system, &record, temp.path(), &commit, None)?;
        // The branch may have moved on during the download; one rewritten to leave the commit
        // out is what activation refuses.
        self.git(&["fetch", "origin", branch])?;
        // A rerun can revoke validation during a download.
        match self.validated_build(branch, &commit)? {
            Checked::Passed(again, _) if again == run => {}
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
    /// The newest commit of `branch` after `current` whose push run passed, with that run and
    /// the build it published. GitHub lists the branch's passed runs newest first, so one
    /// request finds it however many commits ahead of it are still deciding or failed.
    fn passed_since(&self, branch: &str, current: &str) -> Result<Option<(String, Value, Value)>> {
        let passed = io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=push&branch={branch}&status=success&per_page=20"
            ),
        )?;
        let ahead = self.git(&[
            "rev-list",
            "--first-parent",
            &format!("{current}..origin/{branch}"),
        ])?;
        let Some(commit) = ahead.lines().find(|commit| {
            releases::latest_run(
                &passed["workflow_runs"],
                commit,
                "push",
                Some(branch),
                false,
            )
            .is_some()
        }) else {
            return Ok(None);
        };
        // The listing only names the commit: its own runs decide, the latest of them.
        Ok(match self.validated_build(branch, commit)? {
            Checked::Passed(run, record) => Some((commit.to_owned(), run, record)),
            Checked::Pending(_) => None,
        })
    }
    /// The passed push run of `commit` on `branch` and the build it published, or the run still
    /// deciding. The push run tests exactly the commit the branch moved to, so its run decides;
    /// a commit without one waits.
    fn validated_build(&self, branch: &str, commit: &str) -> Result<Checked> {
        let runs = io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=push&branch={branch}&head_sha={commit}&per_page=20"
            ),
        )?;
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
