//! `dispatchdev ci`: why the checks on the checkout's PR head failed. Each failed job with the
//! end of its failing step's log, as soon as the job ended, and whether `main`'s latest finished
//! run fails the same job: then the fix is `main`'s, in its own PR, and the change's job runs
//! again once it merged. A run GitHub created and never gave a job is told apart from one that
//! goes on. `dispatchdev landed`: how `main`'s own run of a merged change ended.
use crate::{REPOSITORY, Result, Runner, github, merge::short, review, workspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The workflow whose jobs a PR needs.
const WORKFLOW: &str = "checks.yml";
/// How much of a failed job's log is shown: its last lines.
const LINES: usize = 40;
/// How long `landed` waits for `main`'s run of a merge to end: a run takes five to thirteen
/// minutes, and a command in the background is cut off at thirty, so it stops on its own well
/// before, saying to run it again. How often it asks GitHub meanwhile, and how often it says
/// how the run is getting on.
const LANDS: Duration = Duration::from_secs(25 * 60);
const ASKS: Duration = Duration::from_secs(20);
const PROGRESS: Duration = Duration::from_secs(2 * 60);
/// How long a run may stay queued without a job before GitHub is taken to have dropped it: a
/// run's first job starts within seconds, and within three minutes at the slowest.
const STARTS: Duration = Duration::from_secs(10 * 60);

/// A run of the checks, and its failed jobs by name and id.
struct Run {
    id: u64,
    head: String,
    /// How GitHub says it is getting on: `queued`, `in_progress`, `completed`.
    status: String,
    /// When GitHub started it, as it writes the time.
    started: String,
    /// Whether it has a job yet: GitHub gives a run it started its jobs at once.
    jobs: bool,
    done: bool,
    /// What it came to once done: a run can fail before it has a job to fail in.
    conclusion: String,
    failed: Vec<Failed>,
}
/// A job that ended without passing: failed, timed out or cancelled.
struct Failed {
    name: String,
    id: u64,
    conclusion: String,
    /// Its first step that didn't pass.
    step: Option<Step>,
}
struct Step {
    name: String,
    /// When it started and ended, as GitHub tells the second.
    span: (String, String),
}
impl Failed {
    /// The job as a failure is headed: its name, how it ended, and the step it ended in, which
    /// tells a setup that failed before the tests from a test that failed.
    fn title(&self) -> String {
        let step = self.step.as_ref();
        format!(
            "{} ({}{})",
            self.name,
            self.conclusion,
            step.map_or_else(String::new, |step| format!(" in `{}`", step.name))
        )
    }
    /// The lines of its log that its failed step wrote.
    fn lines(&self, log: &str) -> Vec<String> {
        lines(log, self.step.as_ref().map(|step| &step.span))
    }
}
/// How a run of the checks that hasn't ended is getting on.
#[derive(Debug, PartialEq, Eq)]
pub enum Pending {
    /// GitHub has left it queued without a job for this long, longer than any run takes to
    /// start: it never starts such a run, and refuses to cancel it or run it again.
    Dropped { run: u64, waited: Duration },
    /// It goes on, for this long so far.
    Running(Duration),
}
impl Run {
    /// How it is getting on at `now`, while it hasn't ended.
    fn pending(&self, now: SystemTime) -> Pending {
        let now = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let started = seconds(&self.started).map_or(now, Duration::from_secs);
        let waited = now.saturating_sub(started);
        if !self.jobs && waited >= STARTS {
            Pending::Dropped {
                run: self.id,
                waited,
            }
        } else {
            Pending::Running(waited)
        }
    }
    /// The command that runs its failed jobs again, which GitHub refuses while the run goes on.
    fn rerun(&self) -> String {
        format!(
            "`gh run rerun {} --failed`{}",
            self.id,
            if self.done {
                ""
            } else {
                ", once the run ended: GitHub refuses it before"
            }
        )
    }
}
/// The seconds since 1970 of a time as GitHub writes it, `2026-11-11T20:49:10Z`.
fn seconds(time: &str) -> Option<u64> {
    let number = |at: usize, length: usize| time.get(at..at + length)?.parse::<u64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    // The days to it, in years that start in March, so a leap day ends one.
    let year = year - u64::from(month <= 2);
    let days =
        year * 365 + year / 4 - year / 100 + year / 400 + (153 * ((month + 9) % 12) + 2) / 5 + day
            - 719_469;
    Some(days * 86_400 + number(11, 2)? * 3_600 + number(14, 2)? * 60 + number(17, 2)?)
}
/// What starts the checks GitHub dropped on `branch`'s PR again: a new event on the PR, on the
/// same commit, so its review stands.
pub fn restart(branch: &str) -> String {
    format!(
        "GitHub refuses to cancel that run or run it again, and it never starts: `gh pr close \
         {branch} && gh pr reopen {branch}` starts the checks again on the same commit, once"
    )
}
/// How the checks on `head`, the pushed head of `branch`'s PR, are getting on at `now`, while
/// they haven't ended.
pub fn pending(
    root: &Path,
    branch: &str,
    head: &str,
    now: SystemTime,
    runner: &dyn Runner,
) -> Result<Option<Pending>> {
    Ok(latest(root, branch, "pull_request", None, false, runner)?
        .filter(|run| run.head == head && !run.done)
        .map(|run| run.pending(now)))
}
fn gh(root: &Path, args: &[&str], runner: &dyn Runner) -> Result<String> {
    github::call(runner, root, &[args, &["--repo", REPOSITORY]].concat())
}
/// The newest run of the checks on `branch` for `event` that `filters` of `gh run list` keep,
/// with its `fields`.
fn newest(
    root: &Path,
    branch: &str,
    event: &str,
    filters: &[&str],
    fields: &str,
    runner: &dyn Runner,
) -> Result<Option<Value>> {
    let list = [
        "run",
        "list",
        "--workflow",
        WORKFLOW,
        "--branch",
        branch,
        "--event",
        event,
        "--limit",
        "1",
        "--json",
        fields,
    ];
    let runs: Vec<Value> =
        serde_json::from_str(&gh(root, &[&list[..], filters].concat(), runner)?)?;
    Ok(runs.into_iter().next())
}
/// The newest run of the checks on `branch` for `event`, or the newest `finished` one: of
/// `commit` alone when one is named.
fn latest(
    root: &Path,
    branch: &str,
    event: &str,
    commit: Option<&str>,
    finished: bool,
    runner: &dyn Runner,
) -> Result<Option<Run>> {
    let mut filters = vec![];
    if finished {
        filters.extend(["--status", "completed"]);
    }
    if let Some(commit) = commit {
        filters.extend(["--commit", commit]);
    }
    let fields = "databaseId,headSha,status,conclusion,startedAt";
    let Some(run) = newest(root, branch, event, &filters, fields, runner)? else {
        return Ok(None);
    };
    let id = run["databaseId"].as_u64().ok_or("A run without an id")?;
    let jobs: Value = serde_json::from_str(&gh(
        root,
        &["run", "view", &id.to_string(), "--json", "jobs"],
        runner,
    )?)?;
    let jobs = jobs["jobs"].as_array().map_or(&[][..], Vec::as_slice);
    let failed = jobs
        .iter()
        .filter(|job| {
            matches!(
                job["conclusion"].as_str(),
                Some("failure" | "timed_out" | "cancelled" | "startup_failure")
            )
        })
        .filter_map(|job| {
            let step = job["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|step| !matches!(step["conclusion"].as_str(), Some("success" | "skipped")))
                .find_map(|step| {
                    Some(Step {
                        name: step["name"].as_str()?.to_owned(),
                        span: (
                            step["startedAt"].as_str()?.to_owned(),
                            step["completedAt"].as_str()?.to_owned(),
                        ),
                    })
                });
            Some(Failed {
                name: job["name"].as_str()?.to_owned(),
                id: job["databaseId"].as_u64()?,
                conclusion: job["conclusion"].as_str()?.to_owned(),
                step,
            })
        })
        .collect();
    Ok(Some(Run {
        id,
        head: run["headSha"].as_str().unwrap_or_default().to_owned(),
        status: run["status"].as_str().unwrap_or_default().to_owned(),
        started: run["startedAt"].as_str().unwrap_or_default().to_owned(),
        jobs: !jobs.is_empty(),
        done: run["status"] == "completed",
        conclusion: run["conclusion"].as_str().unwrap_or_default().to_owned(),
        failed,
    }))
}

/// A run of the checks on a commit the PR's head replaced, or the commit alone when it had none.
struct Replaced {
    run: Option<u64>,
    commit: String,
}
impl Replaced {
    /// The line that says so before the head's `run` is told, as it stands at `now`.
    fn line(&self, head: &str, run: Option<&Run>, now: SystemTime) -> String {
        let standing = match run {
            None => "not started yet".to_owned(),
            Some(run) if !run.done => match run.pending(now) {
                Pending::Dropped { .. } => "never started".to_owned(),
                Pending::Running(waited) => {
                    format!("running for {}", workspace::duration(waited))
                }
            },
            Some(run) => match run.conclusion.as_str() {
                "success" => "passed".to_owned(),
                "failure" => "failed".to_owned(),
                conclusion => conclusion.replace('_', " "),
            },
        };
        let (old, new) = (short(&self.commit), short(head));
        match self.run {
            Some(id) => format!(
                "Run {id} is on {old}, which {new} replaced: its results no longer count; the \
                 head's run is {standing}."
            ),
            None => format!(
                "{old} is no longer the PR's head: {new} replaced it, and the head's run is \
                 {standing}."
            ),
        }
    }
}
/// The run on `commit`, when one is named, else `branch`'s newest that failed, when either is on
/// a commit other than `head`, the PR's: GitHub tells a run's results however old its head.
fn replaced(
    root: &Path,
    branch: &str,
    head: &str,
    commit: Option<&str>,
    runner: &dyn Runner,
) -> Result<Option<Replaced>> {
    let id = |run: &Value| run["databaseId"].as_u64();
    let Some(commit) = commit else {
        let failed = newest(
            root,
            branch,
            "pull_request",
            &["--status", "failure"],
            "databaseId,headSha",
            runner,
        )?;
        return Ok(failed
            .filter(|run| run["headSha"] != head)
            .map(|run| Replaced {
                run: id(&run),
                commit: run["headSha"].as_str().unwrap_or_default().to_owned(),
            }));
    };
    let commit = workspace::git(
        runner,
        root,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{commit}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("{commit} names no commit in this checkout."))?;
    if commit == head {
        return Ok(None);
    }
    let on = newest(
        root,
        branch,
        "pull_request",
        &["--commit", &commit],
        "databaseId",
        runner,
    )?;
    Ok(Some(Replaced {
        run: on.as_ref().and_then(id),
        commit,
    }))
}

/// `dispatchdev ci [<commit>]`: why the checks on the PR's head failed, said first when the
/// commit named, or the newest run that failed, is on a head since replaced.
pub fn run(
    root: &Path,
    commit: Option<&str>,
    now: SystemTime,
    runner: &dyn Runner,
    out: &mut dyn Write,
) -> Result<()> {
    let branch = workspace::text(runner, &["git", "branch", "--show-current"], Some(root))?;
    if branch == "main" {
        if commit.is_some() {
            return Err(
                "On main, `dispatchdev ci` reads main's latest run: name no commit.".into(),
            );
        }
        return main(root, now, runner);
    }
    let (branch, head) = review::pushed(root, runner)?;
    let run =
        latest(root, &branch, "pull_request", None, false, runner)?.filter(|run| run.head == head);
    // Unnamed, an old head's failure is told only while the head's own run hasn't passed:
    // after that it is history.
    let passed = run
        .as_ref()
        .is_some_and(|run| run.done && run.conclusion == "success");
    if (commit.is_some() || !passed)
        && let Some(replaced) = replaced(root, &branch, &head, commit, runner)?
    {
        writeln!(out, "{}", replaced.line(&head, run.as_ref(), now))?;
    }
    let Some(run) = run else {
        writeln!(out, "No checks have started on {} yet.", short(&head))?;
        return Ok(());
    };
    let pending = (!run.done).then(|| run.pending(now));
    if let Some(Pending::Dropped { run, waited }) = pending {
        writeln!(
            out,
            "GitHub never started the checks on {}: run {run} has been queued without a job for {}.",
            short(&head),
            workspace::duration(waited)
        )?;
        writeln!(out, "{}", workspace::next(&[restart(&branch)]))?;
        return Ok(());
    }
    if run.failed.is_empty() {
        writeln!(
            out,
            "Nothing failed on {}{}.",
            short(&head),
            so_far(pending)
        )?;
        return Ok(());
    }
    let main = latest(root, "main", "push", None, true, runner)?;
    let log = |job: u64| log(root, job, runner);
    let others = run
        .failed
        .iter()
        .any(|job| job.name != crate::merge::REQUIRED);
    let (mut own, mut shared, mut unsure) = (0, 0, 0);
    for job in &run.failed {
        // The gate fails whenever another job does; alone, its own steps failed.
        if job.name == crate::merge::REQUIRED && others {
            writeln!(
                out,
                "FAIL {}: it fails because the jobs above did",
                job.name
            )?;
            continue;
        }
        let failed = job.lines(&log(job.id));
        let verdict = match main
            .as_ref()
            .and_then(|main| main.failed.iter().find(|failed| failed.name == job.name))
        {
            None if main.is_none() => {
                unsure += 1;
                "main has no finished run to compare with"
            }
            None => {
                own += 1;
                "the change's own: main's latest run passes it"
            }
            Some(theirs) => {
                let theirs = failing(&theirs.lines(&log(theirs.id)));
                let ours = failing(&failed);
                // Main's only when everything that fails here fails there too.
                if ours.is_empty() || theirs.is_empty() {
                    unsure += 1;
                    "main's latest run fails it too; its log doesn't say whether the same way"
                } else if ours.iter().all(|test| theirs.contains(test)) {
                    shared += 1;
                    "main's latest run fails it the same way"
                } else {
                    own += 1;
                    "the change's own: main's latest run fails it, but not the same way"
                }
            }
        };
        writeln!(out, "FAIL {}: {verdict}", job.title())?;
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            writeln!(out, "    {line}")?;
        }
    }
    let mut next = vec![];
    if own > 0 {
        next.push(format!(
            "Fix what is the change's own, commit, and push with `dispatchdev pr`; a flake \
                 unrelated to the diff runs again once, {}",
            run.rerun()
        ));
    }
    if unsure > 0 {
        next.push(
            "Compare the logs where it couldn't: failing the same way as main is main's to \
             fix, otherwise the change's own"
                .to_owned(),
        );
    }
    if shared > 0 {
        next.push(format!(
            "What main fails the same way is main's to fix, at once, in its own PR; once it \
             merged, {}",
            run.rerun()
        ));
    }
    writeln!(out, "{}", workspace::next(&next))?;
    Ok(())
}

/// `dispatchdev ci` on `main`: why main's latest run of the checks failed, which is main's to
/// fix at once.
fn main(root: &Path, now: SystemTime, runner: &dyn Runner) -> Result<()> {
    let Some(run) = latest(root, "main", "push", None, false, runner)? else {
        println!("main has no run of the checks yet.");
        return Ok(());
    };
    if run.failed.is_empty() {
        println!(
            "Nothing failed in main's latest run, on {}{}.",
            short(&run.head),
            so_far((!run.done).then(|| run.pending(now)))
        );
        return Ok(());
    }
    report(root, &run, runner);
    println!(
        "{}",
        workspace::next(&[format!(
            "It blocks every PR: fix it at once in its own PR off main, merged with no review; \
             a flake unrelated to its commit runs again once, {}",
            run.rerun()
        )])
    );
    Ok(())
}
/// What follows "Nothing failed" of a run that hasn't ended.
fn so_far(pending: Option<Pending>) -> String {
    match pending {
        None => String::new(),
        Some(Pending::Running(waited)) => format!(
            " so far: the run has gone on for {}",
            workspace::duration(waited)
        ),
        Some(Pending::Dropped { run, waited }) => format!(
            " so far: GitHub has left run {run} queued without a job for {}",
            workspace::duration(waited)
        ),
    }
}
/// Each job of `run` that failed, with the end of what its failed step wrote.
fn report(root: &Path, run: &Run, runner: &dyn Runner) {
    let others = run
        .failed
        .iter()
        .any(|job| job.name != crate::merge::REQUIRED);
    for job in &run.failed {
        if job.name == crate::merge::REQUIRED && others {
            println!("FAIL {}: it fails because the jobs above did", job.name);
            continue;
        }
        println!("FAIL {} on {}", job.title(), short(&run.head));
        let failed = job.lines(&log(root, job.id, runner));
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            println!("    {line}");
        }
    }
}

/// What `landed` leaves in the change's scratch folder once `main`'s run of its merge ended, so
/// `owed` tells a merge whose run is yet to be waited for from one ready to finish.
pub const LANDED: &str = "landed.json";
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Landed {
    /// The commit the PR merged as.
    pub commit: String,
    /// `main`'s run of the checks on it.
    pub run: u64,
    pub result: Outcome,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Passed,
    Failed,
}
/// What `landed` last left for the change named `name`, if anything, in its scratch folder as it
/// is now.
pub fn landing(name: &str) -> Result<Option<Landed>> {
    match fs::read_to_string(workspace::current_scratch(name)?.join(LANDED)) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// `dispatchdev landed`: `main`'s run of the checks on the commit the branch's PR merged as,
/// waited for to its end, and an error when it failed. Whoever merged a change answers for that
/// run: once a later merge's passes, nothing else reads it. While it waits it says how the run
/// is getting on every `PROGRESS` on `out`, waits on through GitHub not answering, and stops
/// after `LANDS`, saying to run it again.
pub fn landed(root: &Path, runner: &dyn Runner, out: &mut dyn Write) -> Result<()> {
    // Every call to GitHub, its tries and what it reports of a failed run included, is held to
    // the same limit, so a slow GitHub can't keep it past it.
    github::within(runner.now() + LANDS, || land(root, runner, out))
}
fn land(root: &Path, runner: &dyn Runner, out: &mut dyn Write) -> Result<()> {
    let branch = workspace::text(runner, &["git", "branch", "--show-current"], Some(root))?;
    let mut wait = Wait {
        runner,
        out,
        started: runner.now(),
        said: Duration::ZERO,
        last: None,
    };
    let (commit, number) = loop {
        match merged(root, &branch, runner) {
            Ok(Some(found)) => break found,
            Ok(None) => {
                return Err(
                    format!("{branch} has no merged PR: `dispatchdev merge` merges it.").into(),
                );
            }
            Err(error) => wait.through(error, &format!("{branch}'s merge"))?,
        }
    };
    let merge = format!("{}, the commit #{number} merged as", short(&commit));
    writeln!(
        wait.out,
        "Waiting up to {} for main's run of the checks on {merge}.",
        workspace::duration(LANDS)
    )?;
    let run = loop {
        match latest(root, "main", "push", Some(&commit), false, runner) {
            Ok(Some(run)) if run.done => break run,
            Ok(Some(run)) => wait.pause(&merge, &format!("run {} is {}", run.id, run.status))?,
            Ok(None) => wait.pause(&merge, "it hasn't started")?,
            Err(error) => wait.through(error, &merge)?,
        }
    };
    let landed = Landed {
        commit: commit.to_owned(),
        run: run.id,
        result: match run.conclusion.as_str() {
            "success" => Outcome::Passed,
            _ => Outcome::Failed,
        },
    };
    workspace::write_private(
        &workspace::checkout_scratch(root)?.join(LANDED),
        &serde_json::to_string(&landed)?,
    )?;
    if landed.result == Outcome::Passed {
        println!("main's checks passed on {merge}.");
        return Ok(());
    }
    report(root, &run, runner);
    println!(
        "{}",
        workspace::next(&[format!(
            "The merge is yours until this run passes: a flake unrelated to it runs again once, \
             {}, then `dispatchdev landed` again; what it broke is fixed at once in its own PR \
             off main, merged with no review",
            run.rerun()
        )])
    );
    Err(format!("main's checks ended in {} on {merge}.", run.conclusion).into())
}
/// The commit `branch`'s merged PR merged as, and its number; none when it has none.
fn merged(root: &Path, branch: &str, runner: &dyn Runner) -> Result<Option<(String, u64)>> {
    let merged: Vec<Value> = serde_json::from_str(&gh(
        root,
        &[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "merged",
            "--limit",
            "1",
            "--json",
            "number,mergeCommit",
        ],
        runner,
    )?)?;
    let Some(pull) = merged.first() else {
        return Ok(None);
    };
    let commit = pull["mergeCommit"]["oid"]
        .as_str()
        .ok_or("A merged PR without its commit")?;
    let number = pull["number"].as_u64().ok_or("A PR without a number")?;
    Ok(Some((commit.to_owned(), number)))
}
/// `landed`'s wait: when it started, by the runner's clock, and how long into it it last said
/// how the run is getting on.
struct Wait<'a> {
    runner: &'a dyn Runner,
    out: &'a mut dyn Write,
    started: Instant,
    said: Duration,
    /// How the run was last found, which the wait ends on if GitHub's time ran out first.
    last: Option<String>,
}
impl Wait<'_> {
    /// Waits to ask again, having found the run of `merge` as `now` says: once every
    /// `PROGRESS` it says so, and after `LANDS` it stops, saying to run `landed` again.
    fn pause(&mut self, merge: &str, now: &str) -> Result<()> {
        self.last = Some(now.to_owned());
        let waited = self.runner.now().saturating_duration_since(self.started);
        if waited >= LANDS {
            return Err(format!(
                "main's run of the checks on {merge} hasn't ended after {} ({now}): `dispatchdev \
                 landed` again waits on, or look at the run on GitHub.",
                workspace::duration(LANDS)
            )
            .into());
        }
        if waited >= self.said + PROGRESS {
            writeln!(
                self.out,
                "Still waiting for main's run of the checks on {merge}, {} so far: {now}.",
                workspace::duration(waited)
            )?;
            self.said = waited;
        }
        self.runner.pause(ASKS);
        Ok(())
    }
    /// Waits on through `error` when it is GitHub not answering, which passes; any other is
    /// the wait's.
    fn through(
        &mut self,
        error: Box<dyn std::error::Error + Send + Sync>,
        merge: &str,
    ) -> Result<()> {
        if !error.is::<github::Unreachable>() {
            return Err(error);
        }
        // A call cut short by the limit itself says nothing new of the run.
        let waited = self.runner.now().saturating_duration_since(self.started);
        if waited >= LANDS
            && let Some(last) = self.last.clone()
        {
            return self.pause(merge, &last);
        }
        self.pause(merge, &format!("GitHub didn't answer: {error}"))
    }
}

/// A job's whole log, which GitHub hands out once the job ended: `gh run view --log-failed`
/// has none until every job of the run did. Empty when it can't be had.
fn log(root: &Path, job: u64, runner: &dyn Runner) -> String {
    github::download(
        runner,
        root,
        &[
            "api",
            "--allow-escape-sequences",
            &format!("repos/{REPOSITORY}/actions/jobs/{job}/logs"),
        ],
    )
    .unwrap_or_default()
}
/// The lines of a job's log its failed `step` wrote: the log is the whole job's, and what
/// follows a failed step is the job going on and cleaning up. They are those stamped within the
/// step's seconds, up to where the next step starts once it said what failed, since a second
/// may hold two steps. All of them where GitHub names no such step. Each without its time and
/// the colours within it.
fn lines(log: &str, step: Option<&(String, String)>) -> Vec<String> {
    // To the second, as a step's times are.
    let second = |time: &str| time.get(..19).unwrap_or(time).to_owned();
    let step = step.map(|(started, ended)| second(started)..=second(ended));
    let mut lines: Vec<String> = log
        .trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let (time, text) = line.split_once(' ').unwrap_or((line, ""));
            step.as_ref()
                .is_none_or(|step| step.contains(&second(time)))
                .then(|| plain(text).trim_end().to_owned())
        })
        .collect();
    let starts = |line: &String| line.starts_with("##[group]Run ") || line == "Post job cleanup.";
    if let Some(failed) = lines.iter().position(|line| line.starts_with("##[error]"))
        && let Some(next) = lines[failed..].iter().position(starts)
    {
        lines.truncate(failed + next);
    }
    lines
}
/// `text` without its terminal colours: each escape sequence up to the letter that ends it.
fn plain(text: &str) -> String {
    let mut plain = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
        } else if characters.next() == Some('[') {
            characters.find(|character| character.is_ascii_alphabetic());
        }
    }
    plain
}
/// What a failed log says failed, each as its own line without what differs between runs (the
/// worker's number, the duration, the thread's id): failing tests, compiler and lint errors by
/// their code, and what `cargo audit` denies. A summary such as `[fail] test:ui`, or a bare
/// error, names nothing that failed, so it isn't one.
fn failing<Line: AsRef<str>>(lines: &[Line]) -> Vec<String> {
    // A worker's number, a duration, a thread's id: a bare count, or a number with a unit or in
    // brackets. A version such as 0.3.41 stays.
    let varies = |token: &str| {
        let inner = token.trim_matches(['(', ')', '[', ']']);
        let number = inner
            .strip_suffix("ms")
            .or_else(|| inner.strip_suffix('s'))
            .unwrap_or(inner);
        let numeric = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit() || b == b'.');
        numeric && (number.bytes().all(|b| b.is_ascii_digit()) || number != inner || inner != token)
    };
    // The advisories `cargo audit denies:` lists, a `- ` line each, until another line.
    let mut denied = false;
    let mut failing: Vec<String> = lines
        .iter()
        .map(|line| line.as_ref().trim())
        .filter(|line| {
            denied = (denied && line.starts_with("- ")) || line.starts_with("cargo audit denies:");
            (denied && line.starts_with("- "))
                || line.starts_with('✘')
                || line.starts_with('✖')
                || line.starts_with("FAIL [")
                || line.starts_with("test at ")
                || line.starts_with("error[")
                || line.contains(" panicked at ")
        })
        .map(|line| {
            line.split_whitespace()
                .filter(|token| !varies(token))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    failing.sort();
    failing.dedup();
    failing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// GitHub as `landed` asks it, on a clock that moves only when it pauses: the branch's
    /// merged PR, then main's run of its commit, which starts at `starts` and ends at `ends` in
    /// `conclusion`, and that run's jobs. Between `down`'s times every call fails as GitHub
    /// does when it's in trouble.
    struct Github {
        merged: &'static str,
        conclusion: &'static str,
        jobs: String,
        starts: Duration,
        ends: Duration,
        down: std::ops::Range<Duration>,
        /// Whether a call while GitHub is down takes its whole timeout before failing, as one
        /// that hangs does, or fails at once.
        hangs: bool,
        started: Instant,
        clock: Cell<Duration>,
        asked: RefCell<Vec<Vec<String>>>,
        /// The checkout's name, its own for each fixture, so the marker `landed` leaves goes to
        /// a scratch folder no other test writes.
        name: String,
    }
    impl Github {
        fn new(merged: &'static str, (conclusion, jobs): (&'static str, String)) -> Self {
            Self {
                merged,
                conclusion,
                jobs,
                starts: Duration::ZERO,
                ends: Duration::ZERO,
                down: Duration::ZERO..Duration::ZERO,
                hangs: false,
                started: Instant::now(),
                clock: Cell::new(Duration::ZERO),
                asked: RefCell::new(vec![]),
                name: {
                    static MADE: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    format!("landed-test-{}-{made}", std::process::id())
                },
            }
        }
        /// What `landed` left in the fixture's scratch folder, taken away.
        fn left(&self) -> Option<String> {
            let marker = workspace::scratch(&self.name).unwrap().join(LANDED);
            let text = fs::read_to_string(&marker).ok();
            let _ = fs::remove_dir_all(workspace::scratch(&self.name).unwrap());
            text
        }
        /// What `landed` answers, and what it said while it waited.
        fn landed(&self) -> (std::result::Result<(), String>, Vec<String>) {
            let mut out = vec![];
            let result =
                landed(Path::new(&self.name), self, &mut out).map_err(|error| error.to_string());
            let said = String::from_utf8(out).unwrap();
            (result, said.lines().map(str::to_owned).collect())
        }
    }
    impl Runner for Github {
        fn command(&self, args: &[&str], _: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
            self.asked
                .borrow_mut()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            let now = self.clock.get();
            if args[0] == "gh" && self.down.contains(&now) {
                if self.hangs {
                    self.clock.set(now + Duration::from_secs(timeout));
                }
                let call = args[..3].join(" ");
                return Err(
                    format!("{call} failed: dial tcp: lookup api.github.com: i/o timeout").into(),
                );
            }
            let (status, conclusion) = match now >= self.ends {
                true => ("completed", self.conclusion),
                false => ("in_progress", ""),
            };
            let run = format!(
                r#"[{{"databaseId":5,"headSha":"abc","status":"{status}","conclusion":"{conclusion}"}}]"#
            );
            let answer = match args {
                ["git", ..] => "change",
                ["gh", "pr", ..] => self.merged,
                ["gh", "run", "list", ..] if now < self.starts => "[]",
                ["gh", "run", "list", ..] => &run,
                ["gh", "run", "view", ..] => &self.jobs,
                // A job's log.
                _ => "",
            };
            Ok(answer.as_bytes().to_vec())
        }
        fn pause(&self, duration: Duration) {
            self.clock.set(self.clock.get() + duration);
        }
        fn now(&self) -> Instant {
            self.started + self.clock.get()
        }
    }
    fn jobs(conclusion: &'static str) -> (&'static str, String) {
        let job = format!(r#"{{"name":"core","databaseId":9,"conclusion":"{conclusion}"}}"#);
        (conclusion, format!(r#"{{"jobs":[{job}]}}"#))
    }
    const MERGED: &str = r#"[{"number":7,"mergeCommit":{"oid":"abc"}}]"#;
    const MINUTE: Duration = Duration::from_secs(60);
    #[test]
    fn landed_waits_for_mains_run_of_the_merge_and_fails_when_that_run_failed() {
        // It asks for main's run of the commit the PR merged as, and waits until that ended.
        let github = Github {
            starts: MINUTE,
            ends: 3 * MINUTE,
            ..Github::new(MERGED, jobs("success"))
        };
        let (result, said) = github.landed();
        assert_eq!(result, Ok(()));
        assert_eq!(github.clock.get(), 3 * MINUTE);
        assert!(github.asked.borrow().iter().any(|args| {
            args[..3] == ["gh", "run", "list"]
                && args.windows(2).any(|pair| pair == ["--commit", "abc"])
        }));
        assert_eq!(
            said[..2],
            [
                "Waiting up to 25m 00s for main's run of the checks on abc, the commit #7 merged as.",
                "Still waiting for main's run of the checks on abc, the commit #7 merged as, \
                 2m 00s so far: run 5 is in_progress."
            ]
        );
        // It leaves how the run ended for `owed`, by the commit the PR merged as.
        let passed = r#"{"commit":"abc","run":5,"result":"passed"}"#;
        assert_eq!(github.left().as_deref(), Some(passed));
        // A run that failed is an error, so what waits on the command hears of it: one that
        // failed before it had a job to fail in too.
        let github = Github::new(MERGED, jobs("failure"));
        let (result, _) = github.landed();
        assert_eq!(
            result.unwrap_err(),
            "main's checks ended in failure on abc, the commit #7 merged as."
        );
        let failed = r#"{"commit":"abc","run":5,"result":"failed"}"#;
        assert_eq!(github.left().as_deref(), Some(failed));
        let startup = ("startup_failure", r#"{"jobs":[]}"#.to_owned());
        let github = Github::new(MERGED, startup);
        let (result, _) = github.landed();
        assert_eq!(
            result.unwrap_err(),
            "main's checks ended in startup_failure on abc, the commit #7 merged as."
        );
        assert_eq!(github.left().as_deref(), Some(failed));
        // A branch whose PR hasn't merged has no such run to wait for, and leaves nothing.
        let github = Github::new("[]", jobs("success"));
        assert!(
            github
                .landed()
                .0
                .unwrap_err()
                .contains("change has no merged PR")
        );
        assert_eq!(github.clock.get(), Duration::ZERO);
        assert_eq!(github.left(), None);
    }
    #[test]
    fn landed_says_how_the_run_goes_waits_through_github_not_answering_and_stops_on_its_own() {
        // GitHub stops answering for five minutes, before it named the merge and after: the
        // wait goes on through it, saying so, and ends with the run.
        for down in [Duration::ZERO..5 * MINUTE, 4 * MINUTE..9 * MINUTE] {
            let github = Github {
                ends: 10 * MINUTE,
                down: down.clone(),
                ..Github::new(MERGED, jobs("success"))
            };
            let (result, said) = github.landed();
            assert_eq!(result, Ok(()), "{down:?}");
            let call = if down.start == Duration::ZERO {
                "pr"
            } else {
                "run"
            };
            let unanswered = format!(
                "GitHub didn't answer: `gh {call} list` failed on all 3 tries: dial tcp: lookup \
                 api.github.com: i/o timeout."
            );
            assert!(
                said.iter().any(|line| line.ends_with(&unanswered)),
                "{said:#?}"
            );
        }
        // A run that never ends stops the wait well before a background command is cut off,
        // having said every two minutes how it went, and saying to run it again.
        let github = Github {
            ends: Duration::MAX,
            ..Github::new(MERGED, jobs("success"))
        };
        let (result, said) = github.landed();
        assert_eq!(
            result.unwrap_err(),
            "main's run of the checks on abc, the commit #7 merged as hasn't ended after 25m 00s \
             (run 5 is in_progress): `dispatchdev landed` again waits on, or look at the run on \
             GitHub."
        );
        assert!(github.clock.get() < 26 * MINUTE);
        assert_eq!(
            github.left(),
            None,
            "a run that hasn't ended leaves nothing"
        );
        let progress: Vec<_> = said
            .iter()
            .filter_map(|line| Some(line.split_once("merged as, ")?.1.split_once(" so far")?.0))
            .map(str::to_owned)
            .collect();
        assert_eq!(
            progress,
            (1..=12)
                .map(|n| format!("{}m 00s", 2 * n))
                .collect::<Vec<_>>()
        );
        // GitHub hanging on every call from the run's start: each try is cut to what is left,
        // so the wait still ends at its limit, not tries' worth of minutes after it.
        let github = Github {
            ends: Duration::MAX,
            down: 30 * Duration::from_secs(1)..Duration::MAX,
            hangs: true,
            ..Github::new(MERGED, jobs("success"))
        };
        let (result, _) = github.landed();
        assert!(result.unwrap_err().contains("hasn't ended after 25m 00s"));
        assert!(
            github.clock.get() <= LANDS + Duration::from_secs(1),
            "{:?}",
            github.clock.get()
        );
    }
    /// GitHub with one run of the checks on commit `abc`, as `gh run list` and `gh run view`
    /// tell it.
    struct Checks {
        run: &'static str,
        jobs: &'static str,
    }
    impl Runner for Checks {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            let run = format!(r#"[{{"databaseId":5,"headSha":"abc",{}}}]"#, self.run);
            Ok(match args {
                ["gh", "run", "list", ..] => run.into_bytes(),
                _ => self.jobs.as_bytes().to_vec(),
            })
        }
    }
    #[test]
    fn a_run_github_never_started_is_told_from_one_that_goes_on() {
        let at = |seconds: u64| UNIX_EPOCH + Duration::from_secs(seconds);
        let minutes = |minutes: u64| Duration::from_secs(minutes * 60);
        let pending = |run, jobs, head, now| {
            pending(Path::new("."), "change", head, now, &Checks { run, jobs }).unwrap()
        };
        // Queued at 20:49:10 on 11 November 2026, which is 1794430150, and given no job.
        let queued = r#""status":"queued","conclusion":"","startedAt":"2026-11-11T20:49:10Z""#;
        let (none, some) = (r#"{"jobs":[]}"#, r#"{"jobs":[{"name":"changes"}]}"#);
        assert_eq!(
            pending(queued, none, "abc", at(1_794_430_150 + 599)),
            Some(Pending::Running(Duration::from_secs(599))),
            "a run may take minutes to start"
        );
        assert_eq!(
            pending(queued, none, "abc", at(1_794_430_150 + 2 * 3_600)),
            Some(Pending::Dropped {
                run: 5,
                waited: minutes(120)
            })
        );
        assert_eq!(
            pending(queued, some, "abc", at(1_794_430_150 + 2 * 3_600)),
            Some(Pending::Running(minutes(120))),
            "a run with a job started, however long it waits for a runner"
        );
        // Across a leap day: queued ten seconds before 1 March 2028, read ten minutes into it.
        let leap = r#""status":"queued","conclusion":"","startedAt":"2028-02-29T23:59:50Z""#;
        assert_eq!(
            pending(leap, none, "abc", at(1_835_482_200)),
            Some(Pending::Dropped {
                run: 5,
                waited: Duration::from_secs(610)
            })
        );
        // Only the pushed head's run counts, and only until it ended.
        assert_eq!(
            pending(queued, none, "def", at(1_794_430_150 + 7_200)),
            None
        );
        let ended =
            r#""status":"completed","conclusion":"failure","startedAt":"2026-11-11T20:49:10Z""#;
        assert_eq!(pending(ended, none, "abc", at(1_794_430_150 + 7_200)), None);
    }
    const OLD: &str = "aaaa111122223333444455556666777788889999";
    const EARLIER: &str = "cccc111122223333444455556666777788889999";
    const HEAD: &str = "bbbb111122223333444455556666777788889999";
    /// The branch `change`, pushed at `HEAD`, which replaced `OLD` and `EARLIER`, and GitHub with
    /// run 4 on `OLD`, which failed, and none on `EARLIER`.
    struct Pushed {
        /// What `gh run list` gives of the newest run.
        newest: String,
        /// What it gives of the newest run that failed.
        failed: String,
        jobs: &'static str,
    }
    /// Run 4, on `OLD`, which failed.
    fn old() -> String {
        format!(
            r#"[{{"databaseId":4,"headSha":"{OLD}","status":"completed","conclusion":"failure"}}]"#
        )
    }
    impl Pushed {
        /// The newest run 5, on `HEAD`, with `fields`, or run 4 when the head's has none yet; the
        /// newest that failed run 4 when `failed`, else none.
        fn new(head: Option<&str>, failed: bool) -> Self {
            Self {
                newest: head.map_or_else(old, |fields| {
                    format!(r#"[{{"databaseId":5,"headSha":"{HEAD}",{fields}}}]"#)
                }),
                failed: if failed { old() } else { "[]".to_owned() },
                jobs: r#"{"jobs":[{"name":"core","databaseId":9,"conclusion":"success"}]}"#,
            }
        }
        /// What `dispatchdev ci` says, with `commit` named, at `now`, line by line.
        fn ci(&self, commit: Option<&str>, now: SystemTime) -> Result<Vec<String>> {
            let mut out = vec![];
            run(Path::new("."), commit, now, self, &mut out)?;
            Ok(String::from_utf8(out)?.lines().map(str::to_owned).collect())
        }
    }
    impl Runner for Pushed {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            let filter = |flag: &str| {
                args.windows(2)
                    .find(|pair| pair[0] == flag)
                    .map(|pair| pair[1])
            };
            let answer = match args {
                ["git", "branch", "--show-current"] => "change".to_owned(),
                ["git", "status", "--porcelain"] => String::new(),
                ["git", "rev-parse", "HEAD"] => HEAD.to_owned(),
                ["git", "rev-parse", "--verify", "-q", rev] => {
                    match rev.strip_suffix("^{commit}") {
                        Some(commit) => [OLD, EARLIER, HEAD]
                            .into_iter()
                            .find(|full| full.starts_with(commit))
                            .ok_or("git rev-parse failed")?
                            .to_owned(),
                        // The branch as pushed.
                        None => HEAD.to_owned(),
                    }
                }
                ["gh", "run", "list", ..] => match (filter("--status"), filter("--commit")) {
                    (Some("failure"), None) => self.failed.clone(),
                    (None, Some(OLD)) => old(),
                    (None, Some(_)) => "[]".to_owned(),
                    (None, None) => self.newest.clone(),
                    _ => return Err(format!("unexpected {args:?}").into()),
                },
                ["gh", "run", "view", ..] => self.jobs.to_owned(),
                _ => return Err(format!("unexpected {args:?}").into()),
            };
            Ok(answer.into_bytes())
        }
    }
    #[test]
    fn a_run_on_a_head_since_replaced_is_said_to_count_no_longer() {
        let now = UNIX_EPOCH + Duration::from_secs(1_794_430_150);
        let passed = Some(r#""status":"completed","conclusion":"success""#);
        let replaced = |standing: &str| {
            format!(
                "Run 4 is on aaaa111122, which bbbb111122 replaced: its results no longer count; \
                 the head's run is {standing}."
            )
        };
        let nothing = "Nothing failed on bbbb111122.".to_owned();
        // T3 names the short commit of the head a run's notice was for.
        let ci = |pushed: Pushed, commit| pushed.ci(commit, now).unwrap();
        assert_eq!(
            ci(Pushed::new(passed, false), Some("aaaa111")),
            [replaced("passed"), nothing.clone()]
        );
        assert_eq!(
            ci(Pushed::new(passed, false), Some("cccc111")),
            [
                "cccc111122 is no longer the PR's head: bbbb111122 replaced it, and the head's \
                 run is passed."
                    .to_owned(),
                nothing.clone()
            ]
        );
        assert_eq!(
            ci(Pushed::new(passed, false), Some("bbbb111")),
            std::slice::from_ref(&nothing)
        );
        assert_eq!(
            Pushed::new(passed, false)
                .ci(Some("dddd111"), now)
                .unwrap_err()
                .to_string(),
            "dddd111 names no commit in this checkout."
        );
        // Unnamed, the newest run that failed is the one T3 told of, until the head's passed.
        assert_eq!(
            ci(Pushed::new(passed, true), None),
            std::slice::from_ref(&nothing)
        );
        assert_eq!(ci(Pushed::new(passed, false), None), [nothing]);
        // The head's run is told as it stands, and then reported as usual.
        let going = r#""status":"in_progress","conclusion":"","startedAt":"2026-11-11T20:49:10Z""#;
        assert_eq!(
            Pushed::new(Some(going), true)
                .ci(None, now + Duration::from_secs(90))
                .unwrap(),
            [
                replaced("running for 1m 30s"),
                "Nothing failed on bbbb111122 so far: the run has gone on for 1m 30s.".to_owned()
            ]
        );
        assert_eq!(
            ci(Pushed::new(None, true), None),
            [
                replaced("not started yet"),
                "No checks have started on bbbb111122 yet.".to_owned()
            ]
        );
    }
    #[test]
    fn ci_says_how_to_start_checks_github_never_started() {
        // Queued at 20:49:10 on 11 November 2026, which is 1794430150, and given no job.
        let queued = r#""status":"queued","conclusion":"","startedAt":"2026-11-11T20:49:10Z""#;
        let dropped = Pushed {
            jobs: r#"{"jobs":[]}"#,
            ..Pushed::new(Some(queued), false)
        };
        let now = UNIX_EPOCH + Duration::from_secs(1_794_430_150 + 2 * 3_600);
        assert_eq!(
            dropped.ci(None, now).unwrap(),
            [
                "GitHub never started the checks on bbbb111122: run 5 has been queued without a \
                 job for 120m 00s."
                    .to_owned(),
                "Next:".to_owned(),
                "  - GitHub refuses to cancel that run or run it again, and it never starts: \
                 `gh pr close change && gh pr reopen change` starts the checks again on the same \
                 commit, once"
                    .to_owned(),
            ]
        );
    }
    #[test]
    fn a_failed_job_names_the_step_it_failed_in() {
        // A job whose sandbox could not be installed never reached its tests: the step says so.
        let jobs = r#"{"jobs":[{"name":"api (1)","databaseId":9,"conclusion":"failure","steps":[
            {"name":"Set up job","conclusion":"success"},
            {"name":"Run ./.github/actions/setup-support-sandbox","conclusion":"failure",
             "startedAt":"2026-11-11T23:07:13Z","completedAt":"2026-11-11T23:07:31Z"},
            {"name":"Run npm run check:ci -- api 1/2","conclusion":"skipped"}]}]}"#;
        let run = |run| {
            latest(
                Path::new("."),
                "change",
                "pull_request",
                None,
                false,
                &Checks { run, jobs },
            )
            .unwrap()
            .unwrap()
        };
        let going =
            run(r#""status":"in_progress","conclusion":"","startedAt":"2026-11-11T23:06:00Z""#);
        assert_eq!(
            going.failed[0].title(),
            "api (1) (failure in `Run ./.github/actions/setup-support-sandbox`)"
        );
        // GitHub runs a failed job again only once its whole run ended.
        let ended = run(
            r#""status":"completed","conclusion":"failure","startedAt":"2026-11-11T23:06:00Z""#,
        );
        assert!(going.rerun().contains("once the run ended"));
        assert_eq!(ended.rerun(), "`gh run rerun 5 --failed`");
    }
    #[test]
    fn a_failure_is_mains_only_when_main_fails_the_same_tests() {
        let log = |tests: &[(&str, &str)]| {
            tests
                .iter()
                .map(|(worker, test)| {
                    format!(
                        "2026-11-07T04:45:05.9Z   ✘  {worker} {test} (7.8s)\n\
                         2026-11-07T04:45:06.6Z     Error: disposed\n\
                         2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n"
                    )
                })
                .collect::<String>()
        };
        let reload = "meal-breaks.spec.ts:594:1 › survives a reload";
        let home = "home.spec.ts:3:1 › opens";
        let ours = failing(&lines(&log(&[("22", reload)]), None));
        assert_eq!(ours, [format!("✘ {reload}")]);
        let shared = |theirs: &[(&str, &str)]| {
            let theirs = failing(&lines(&log(theirs), None));
            ours.iter().all(|test| theirs.contains(test))
        };
        assert!(
            shared(&[("7", reload), ("3", home)]),
            "another worker, another duration"
        );
        assert!(
            !shared(&[("3", home)]),
            "the same wrapper and error lines aren't the same test"
        );
        assert_eq!(
            failing(
                &[
                    "cargo audit denies:",
                    "- RUSTSEC-2026-0012 on time 0.3.41",
                    "- yanked ring 0.17.9",
                    "- not after the list",
                ][..3]
            ),
            ["- RUSTSEC-2026-0012 on time 0.3.41", "- yanked ring 0.17.9"]
        );
        assert_eq!(
            failing(&[
                "error[E0308]: mismatched types",
                "error: could not compile `core`"
            ]),
            ["error[E0308]: mismatched types"]
        );
        assert!(
            failing(&lines(
                "2026-11-07T04:45:07.0Z [fail] test:ui (41.6s)\n",
                None
            ))
            .is_empty()
        );
        // Of a job's whole log, only what its failed step wrote is read, without its colours:
        // an action that fails says so in words of its own, and the steps after it, which may
        // start in the second it ended, aren't its own.
        let step = (
            "2026-11-10T20:22:39Z".to_owned(),
            "2026-11-10T20:22:40Z".to_owned(),
        );
        assert_eq!(
            lines(
                "\u{feff}2026-11-10T20:22:38.9Z Set up job\n\
                 2026-11-10T20:22:39.2Z ##[group]Run actions/cache/restore@55cc834\n\
                 2026-11-10T20:22:39.3Z \u{1b}[31;1mFAIL\u{1b}[0m [ 1.256s] review::tests::x\n\
                 2026-11-10T20:22:40.1Z ##[error]Cache restore failed\n\
                 2026-11-10T20:22:40.2Z ##[group]Run sccache --show-stats\n\
                 2026-11-10T20:22:40.3Z Compile requests 0\n\
                 2026-11-10T20:22:41.0Z Post job cleanup.\n",
                Some(&step)
            ),
            [
                "##[group]Run actions/cache/restore@55cc834",
                "FAIL [ 1.256s] review::tests::x",
                "##[error]Cache restore failed"
            ]
        );
        assert_eq!(
            failing(&["thread 'week' (18381410) panicked at a.rs:3:5:"]),
            ["thread 'week' panicked at a.rs:3:5:"]
        );
    }
}
