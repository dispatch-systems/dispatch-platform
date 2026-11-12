//! `dispatchdev ci`: why the checks on the checkout's PR head failed. Each failed job with the
//! end of its failing step's log, as soon as the job ended, and whether `main`'s latest finished
//! run fails the same job: then the fix is `main`'s, in its own PR, and the change's job runs
//! again once it merged. A run GitHub created and never gave a job is told apart from one that
//! goes on. `dispatchdev landed`: how `main`'s own run of a merged change ended.
use crate::{REPOSITORY, Result, Runner, merge::short, require, review, workspace};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The workflow whose jobs a PR needs.
const WORKFLOW: &str = "checks.yml";
/// How much of a failed job's log is shown: its last lines.
const LINES: usize = 40;
/// How long `landed` waits for `main`'s run of a merge to end, which its jobs' own limits do
/// well before, and how often it asks GitHub meanwhile.
const LANDS: Duration = Duration::from_secs(45 * 60);
const ASKS: Duration = Duration::from_secs(20);
/// How long a run may stay queued without a job before GitHub is taken to have dropped it: a
/// run's first job starts within seconds, and within three minutes at the slowest.
const STARTS: Duration = Duration::from_secs(10 * 60);

/// A run of the checks, and its failed jobs by name and id.
struct Run {
    id: u64,
    head: String,
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
    let mut command = vec!["gh"];
    command.extend_from_slice(args);
    command.extend_from_slice(&["--repo", REPOSITORY]);
    workspace::text(runner, &command, Some(root))
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
    let mut list = vec![
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
        "databaseId,headSha,status,conclusion,startedAt",
    ];
    if finished {
        list.extend(["--status", "completed"]);
    }
    if let Some(commit) = commit {
        list.extend(["--commit", commit]);
    }
    let runs: Vec<Value> = serde_json::from_str(&gh(root, &list, runner)?)?;
    let Some(run) = runs.first() else {
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
        started: run["startedAt"].as_str().unwrap_or_default().to_owned(),
        jobs: !jobs.is_empty(),
        done: run["status"] == "completed",
        conclusion: run["conclusion"].as_str().unwrap_or_default().to_owned(),
        failed,
    }))
}

pub fn run(root: &Path, now: SystemTime, runner: &dyn Runner) -> Result<()> {
    let branch = workspace::text(runner, &["git", "branch", "--show-current"], Some(root))?;
    if branch == "main" {
        return main(root, now, runner);
    }
    let (branch, head) = review::pushed(root, runner)?;
    let Some(run) =
        latest(root, &branch, "pull_request", None, false, runner)?.filter(|run| run.head == head)
    else {
        println!("No checks have started on {} yet.", short(&head));
        return Ok(());
    };
    let pending = (!run.done).then(|| run.pending(now));
    if let Some(Pending::Dropped { run, waited }) = pending {
        println!(
            "GitHub never started the checks on {}: run {run} has been queued without a job for {}.",
            short(&head),
            workspace::duration(waited)
        );
        println!("{}", workspace::next(&[restart(&branch)]));
        return Ok(());
    }
    if run.failed.is_empty() {
        println!("Nothing failed on {}{}.", short(&head), so_far(pending));
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
            println!("FAIL {}: it fails because the jobs above did", job.name);
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
        println!("FAIL {}: {verdict}", job.title());
        for line in &failed[failed.len().saturating_sub(LINES)..] {
            println!("    {line}");
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
    println!("{}", workspace::next(&next));
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

/// `dispatchdev landed`: `main`'s run of the checks on the commit the branch's PR merged as,
/// waited for to its end, and an error when it failed. Whoever merged a change answers for that
/// run: once a later merge's run passes, nothing else reads it. `pause` waits between asks.
pub fn landed(root: &Path, runner: &dyn Runner, pause: &dyn Fn(Duration)) -> Result<()> {
    let branch = workspace::text(runner, &["git", "branch", "--show-current"], Some(root))?;
    let merged: Vec<Value> = serde_json::from_str(&gh(
        root,
        &[
            "pr",
            "list",
            "--head",
            &branch,
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
        return Err(format!("{branch} has no merged PR: `dispatchdev merge` merges it.").into());
    };
    let commit = pull["mergeCommit"]["oid"]
        .as_str()
        .ok_or("A merged PR without its commit")?;
    let merge = format!(
        "{}, the commit #{} merged as",
        short(commit),
        pull["number"]
    );
    let mut waited = Duration::ZERO;
    let run = loop {
        if let Some(run) = latest(root, "main", "push", Some(commit), true, runner)? {
            break run;
        }
        require(
            waited < LANDS,
            &format!(
                "main's run of the checks on {merge} hasn't ended after {}: look at it on GitHub.",
                workspace::duration(LANDS)
            ),
        )?;
        pause(ASKS);
        waited += ASKS;
    };
    if run.conclusion == "success" {
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

/// A job's whole log, which GitHub hands out once the job ended: `gh run view --log-failed`
/// has none until every job of the run did. Empty when it can't be had.
fn log(root: &Path, job: u64, runner: &dyn Runner) -> String {
    workspace::text(
        runner,
        &[
            "gh",
            "api",
            "--allow-escape-sequences",
            &format!("repos/{REPOSITORY}/actions/jobs/{job}/logs"),
        ],
        Some(root),
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

    /// GitHub as `landed` asks it: the branch's merged PR, then main's finished run of its
    /// commit, which isn't there until the wait paused `after` times, and that run's jobs.
    struct Github {
        merged: &'static str,
        conclusion: &'static str,
        jobs: String,
        after: u32,
        paused: Cell<u32>,
        asked: RefCell<Vec<Vec<String>>>,
    }
    impl Runner for Github {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            self.asked
                .borrow_mut()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            let run = format!(
                r#"[{{"databaseId":5,"headSha":"abc","status":"completed","conclusion":"{}"}}]"#,
                self.conclusion
            );
            let answer = match args {
                ["git", ..] => "change",
                ["gh", "pr", ..] => self.merged,
                ["gh", "run", "list", ..] if self.paused.get() < self.after => "[]",
                ["gh", "run", "list", ..] => &run,
                ["gh", "run", "view", ..] => &self.jobs,
                // A job's log.
                _ => "",
            };
            Ok(answer.as_bytes().to_vec())
        }
    }
    #[test]
    fn landed_waits_for_mains_run_of_the_merge_and_fails_when_that_run_failed() {
        let merged = r#"[{"number":7,"mergeCommit":{"oid":"abc"}}]"#;
        let jobs = |conclusion: &'static str| {
            let job = format!(r#"{{"name":"core","databaseId":9,"conclusion":"{conclusion}"}}"#);
            (conclusion, format!(r#"{{"jobs":[{job}]}}"#))
        };
        let landed = |merged, (conclusion, jobs): (&'static str, String), after| {
            let github = Github {
                merged,
                conclusion,
                jobs,
                after,
                paused: Cell::new(0),
                asked: RefCell::new(vec![]),
            };
            let result = landed(Path::new("."), &github, &|_| {
                github.paused.set(github.paused.get() + 1)
            });
            (result.map_err(|error| error.to_string()), github)
        };
        // It asks for main's run of the commit the PR merged as, and waits until that ended.
        let (result, github) = landed(merged, jobs("success"), 2);
        assert_eq!(result, Ok(()));
        assert_eq!(github.paused.get(), 2);
        assert!(github.asked.borrow().iter().any(|args| {
            args[..3] == ["gh", "run", "list"]
                && args.windows(2).any(|pair| pair == ["--commit", "abc"])
        }));
        // A run that failed is an error, so what waits on the command hears of it: one that
        // failed before it had a job to fail in too.
        let (result, _) = landed(merged, jobs("failure"), 0);
        assert_eq!(
            result.unwrap_err(),
            "main's checks ended in failure on abc, the commit #7 merged as."
        );
        let (result, _) = landed(merged, ("startup_failure", r#"{"jobs":[]}"#.to_owned()), 0);
        assert_eq!(
            result.unwrap_err(),
            "main's checks ended in startup_failure on abc, the commit #7 merged as."
        );
        // One that never ends stops the wait, by the clock the pauses move.
        let (result, github) = landed(merged, jobs("success"), u32::MAX);
        assert!(result.unwrap_err().contains("hasn't ended after 45m 00s"));
        assert_eq!(github.paused.get(), 135);
        // A branch whose PR hasn't merged has no such run to wait for.
        let (result, github) = landed("[]", jobs("success"), 0);
        assert!(result.unwrap_err().contains("change has no merged PR"));
        assert_eq!(github.paused.get(), 0);
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
