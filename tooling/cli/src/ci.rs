//! `dispatchdev ci <name>`: the full checks on a change's branch while its preview is tried,
//! so what they find is fixed before the PR is queued. It pushes the committed branch at once,
//! starts `checks.yml` on it without the gate, and follows the run to its end, listing every
//! failed job, while the rule checks run here beside it: their failure cancels the run. Jobs
//! GitHub stopped itself, past their time limit or on a runner that never started, run again
//! once. A head that has a run going or passed already is followed or reported, not run again.
//! A newer run of the branch replaces an older one; `ship` stops on a head whose run failed and
//! cancels one still going, since the queue runs the checks again on the squash commit.
//! `--rerun-failed` runs the failed jobs of the head's run again, for a failure that isn't the
//! change's own. `--removability <feature>` runs and follows the weekly `removability.yml` the
//! same way, for that feature alone: its runs are told apart by the name the workflow gives them.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    check, require,
    ship::{ATTEMPTS, PAUSE, failed_jobs, report, rest},
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::{path::Path, thread::ScopedJoinHandle};

/// Looks that bound the wait for a dispatched run to appear: two minutes.
const APPEAR_LOOKS: u32 = 12;
/// Looks that bound the whole run: an hour.
const RUN_LOOKS: u32 = 360;

/// A workflow `ci` starts on a branch and follows.
struct Workflow {
    file: &'static str,
    /// The input it is started with, `name=value`.
    input: String,
    /// The name its runs take, when the file's other runs aren't its own.
    title: Option<String>,
    /// How the lines about it name it, as "The checks".
    what: String,
}
impl Workflow {
    fn checks() -> Self {
        Self {
            file: "checks.yml",
            input: "gate=false".into(),
            title: None,
            what: "The checks".into(),
        }
    }
    /// One feature's removability: the run named for it in `removability.yml`'s `run-name`.
    fn removability(feature: &str) -> Self {
        Self {
            file: "removability.yml",
            input: format!("feature={feature}"),
            title: Some(format!("Removability without {feature}")),
            what: format!("The removability checks of {feature}"),
        }
    }
    /// `what` within a sentence: "the checks".
    fn named(&self) -> String {
        let mut chars = self.what.chars();
        chars
            .next()
            .map(|first| first.to_lowercase().chain(chars).collect())
            .unwrap_or_default()
    }
    /// `what` heading a line: "Checks".
    fn heading(&self) -> String {
        let named = self.named();
        let rest = named.strip_prefix("the ").unwrap_or(&named);
        let mut chars = rest.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    }
}

/// The branch's runs of `workflow` on `head`, newest first.
fn runs(runner: &dyn Runner, workflow: &Workflow, branch: &str, head: &str) -> Result<Vec<Value>> {
    let runs = rest(
        runner,
        &format!(
            "actions/workflows/{}/runs?event=workflow_dispatch&branch={branch}&head_sha={head}&per_page=20",
            workflow.file
        ),
    )?;
    let mut runs: Vec<Value> = runs["workflow_runs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|run| {
            workflow
                .title
                .as_ref()
                .is_none_or(|title| run["display_title"] == title.as_str())
        })
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run["id"].as_u64().unwrap_or(0)));
    Ok(runs)
}

/// Push `name`'s branch, run the checks on its head, or with `removability` that feature's
/// removability, and follow them; with `again`, run the failed jobs of the head's run again.
/// Returns the verdict.
pub fn run(
    ws: &Workspace,
    name: &str,
    removability: Option<&str>,
    again: bool,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    let path = Change::existing(ws, name)?.path;
    match (removability, again) {
        (Some(feature), _) => self::removability(&path, name, feature, runner, pause, say),
        (None, true) => self::again(&path, name, runner, pause, say),
        (None, false) => follow(
            &path,
            name,
            &Workflow::checks(),
            &|| check::rules(&path),
            runner,
            pause,
            say,
        ),
    }
}
/// Push the checkout at `path`, on its branch `name`, and run and follow `feature`'s
/// removability on its head; returns the verdict.
pub fn removability(
    path: &Path,
    name: &str,
    feature: &str,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    require(
        workspace::has_feature(path, feature),
        &format!("{name} has no feature named {feature}."),
    )?;
    follow(
        path,
        name,
        &Workflow::removability(feature),
        &|| Ok(()),
        runner,
        pause,
        say,
    )
}
/// The failed jobs of the checks' last run on the head of `name` at `path`, run again and
/// followed; returns the verdict.
fn again(
    path: &Path,
    name: &str,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    let workflow = Workflow::checks();
    let head = workspace::text(runner, &["git", "rev-parse", "HEAD"], Some(path))?;
    let short = &head[..head.len().min(7)];
    let run = runs(runner, &workflow, name, &head)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            format!("No run of the checks on {short} of {name}: dispatchdev ci {name} starts one.")
        })?;
    require(
        run["status"] == "completed",
        &format!("The checks on {short} of {name} are still running."),
    )?;
    require(
        run["conclusion"] != "success",
        &format!("The checks passed on {short} of {name}: nothing to run again."),
    )?;
    let id = run["id"].as_u64().ok_or("The run has no id")?;
    rerun_failed(runner, id)?;
    let attempt = run["run_attempt"].as_u64().unwrap_or(1) + 1;
    watch(
        &run, attempt, false, &workflow, name, short, &mut None, runner, pause, say,
    )
}
fn follow(
    path: &Path,
    name: &str,
    workflow: &Workflow,
    local: &(dyn Fn() -> Result<()> + Sync),
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    let what = &workflow.what;
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(path));
    require(
        git(&["status", "--porcelain"])?.is_empty(),
        "Commit the change first: the checks run on a commit.",
    )?;
    require(
        git(&["rev-parse", "--abbrev-ref", "HEAD"])? == name,
        &format!("worktrees/{name} isn't on its branch."),
    )?;
    let head = git(&["rev-parse", "HEAD"])?;
    let short = &head[..head.len().min(7)];
    // The branch is the change's own: a rebase replaces what was pushed before, unless
    // something else pushed to it meanwhile.
    git(&["push", "-q", "--force-with-lease", "-u", "origin", name])?;
    let earlier = runs(runner, workflow, name, &head)?;
    let reused = earlier
        .iter()
        .find(|run| run["status"] != "completed" || run["conclusion"] == "success");
    if let Some(run) = reused.filter(|run| run["status"] == "completed") {
        return Ok(format!(
            "{what} passed on {short} of {name} already: {}",
            run["html_url"].as_str().unwrap_or("")
        ));
    }
    std::thread::scope(|scope| {
        let mut local = Some(scope.spawn(local));
        let run = match reused {
            Some(run) => run.clone(),
            None => start(runner, workflow, name, &head, &earlier, pause)?,
        };
        let attempt = run["run_attempt"].as_u64().unwrap_or(1);
        watch(
            &run, attempt, true, workflow, name, short, &mut local, runner, pause, say,
        )
    })
}
/// Starts `workflow` on the branch `name`, at `head`, and answers its run once GitHub lists it:
/// one newer than any of the `earlier` runs.
fn start(
    runner: &dyn Runner,
    workflow: &Workflow,
    name: &str,
    head: &str,
    earlier: &[Value],
    pause: &dyn Fn(u64),
) -> Result<Value> {
    let newest = earlier
        .first()
        .and_then(|run| run["id"].as_u64())
        .unwrap_or(0);
    workspace::text(
        runner,
        &[
            "gh",
            "workflow",
            "run",
            workflow.file,
            "--repo",
            REPOSITORY,
            "--ref",
            name,
            "-f",
            &workflow.input,
        ],
        None,
    )?;
    for _ in 0..APPEAR_LOOKS {
        pause(PAUSE);
        let started = runs(runner, workflow, name, head)?
            .into_iter()
            .find(|run| run["id"].as_u64().unwrap_or(0) > newest);
        if let Some(run) = started {
            return Ok(run);
        }
    }
    Err(format!(
        "GitHub started no run of {} in two minutes; run it again.",
        workflow.named()
    )
    .into())
}
/// Follows `run` from its attempt `attempt` to its end, the check here, `local`, beside it:
/// the verdict, with every failed job. With `retry`, jobs GitHub stopped itself run again once.
#[allow(clippy::too_many_arguments)]
fn watch(
    run: &Value,
    mut attempt: u64,
    mut retry: bool,
    workflow: &Workflow,
    name: &str,
    short: &str,
    local: &mut Option<ScopedJoinHandle<Result<()>>>,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    let what = &workflow.what;
    let id = run["id"].as_u64().ok_or("The run has no id")?;
    say(&format!(
        "{} on {short} of {name}: {}",
        workflow.heading(),
        run["html_url"].as_str().unwrap_or("")
    ));
    let mut trouble = 0;
    for _ in 0..RUN_LOOKS {
        // The check here ends first, and its failure would fail the run the same way.
        if local.as_ref().is_some_and(ScopedJoinHandle::is_finished)
            && let Err(error) = joined(local.take())
        {
            cancel(runner, id);
            return Err(format!(
                "{error} Fix it, commit and run ci again; the checks on GitHub were cancelled."
            )
            .into());
        }
        let run = match rest(runner, &format!("actions/runs/{id}")) {
            Ok(run) => run,
            Err(error) => {
                trouble += 1;
                if trouble >= ATTEMPTS {
                    return Err(error);
                }
                pause(PAUSE);
                continue;
            }
        };
        trouble = 0;
        // Until the attempt asked for starts, the run still shows how the last one ended.
        if run["run_attempt"].as_u64().unwrap_or(1) < attempt || run["status"] != "completed" {
            pause(PAUSE);
            continue;
        }
        joined(local.take())?;
        if run["conclusion"] == "success" {
            return Ok(format!("{what} passed on {short} of {name}."));
        }
        let failed = failed_jobs(runner, &run, false);
        let gone: Vec<&Value> = failed.iter().filter(|job| stopped(runner, job)).collect();
        if retry && !gone.is_empty() && gone.len() == failed.len() {
            rerun_failed(runner, id)?;
            (retry, attempt) = (false, attempt + 1);
            say(&format!(
                "GitHub stopped {} on {short} before a verdict; running them again.",
                gone.iter()
                    .map(|job| job["name"].as_str().unwrap_or("job"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            pause(PAUSE);
            continue;
        }
        if run["conclusion"] == "cancelled" && gone.is_empty() {
            return Err(format!(
                "{what} on {short} of {name} were cancelled, by a newer run of the branch or by hand."
            )
            .into());
        }
        if failed.is_empty() {
            return Err(format!(
                "{what} on {short} of {name} ended {}: {}",
                run["conclusion"].as_str().unwrap_or("without a conclusion"),
                run["html_url"].as_str().unwrap_or("")
            )
            .into());
        }
        return Err(format!(
            "{what} failed on {short} of {name}. Fix them, commit and run them again; a failure \
             that isn't the change's own, as main fails the same way, runs again with \
             dispatchdev ci {name} --rerun-failed. Failed jobs:\n- {}",
            report(runner, &failed).join("\n- ")
        )
        .into());
    }
    Err(format!("{what} on {short} of {name} have not ended after an hour.").into())
}
/// The check here once it ended: what it found, or nothing when there was none.
pub(crate) fn joined(local: Option<ScopedJoinHandle<Result<()>>>) -> Result<()> {
    match local.map(ScopedJoinHandle::join) {
        Some(Ok(result)) => result,
        Some(Err(_)) => Err("The check here stopped unexpectedly.".into()),
        None => Ok(()),
    }
}
/// Whether GitHub stopped `job` itself, with no verdict on the change: past its time limit, or
/// on a runner that never started. A job cancelled by hand or by a newer run isn't.
fn stopped(runner: &dyn Runner, job: &Value) -> bool {
    match job["conclusion"].as_str() {
        Some("timed_out" | "startup_failure") => true,
        Some("cancelled") => rest(runner, &format!("check-runs/{}/annotations", job["id"]))
            .ok()
            .and_then(|notes| notes.as_array().cloned())
            .into_iter()
            .flatten()
            .any(|note| {
                note["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("exceeded the maximum execution time"))
            }),
        _ => false,
    }
}
fn rerun_failed(runner: &dyn Runner, id: u64) -> Result<()> {
    workspace::text(
        runner,
        &[
            "gh",
            "run",
            "rerun",
            &id.to_string(),
            "--failed",
            "--repo",
            REPOSITORY,
        ],
        None,
    )?;
    Ok(())
}
/// Cancels run `id`; one that ended meanwhile needs nothing.
fn cancel(runner: &dyn Runner, id: u64) {
    let endpoint = format!("repos/{REPOSITORY}/actions/runs/{id}/cancel");
    let _ = runner.command(&["gh", "api", "-X", "POST", &endpoint], None, 60);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    /// A branch already pushed, and GitHub's runs of the workflows on its head, as a script:
    /// what `gh workflow run` was asked, and the runs each list answers.
    struct GitHub {
        dispatched: RefCell<Vec<String>>,
        listed: Value,
        run: Value,
    }
    impl Runner for GitHub {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            Ok(match args {
                ["git", "status", ..] | ["git", "push", ..] => vec![],
                ["git", "rev-parse", "--abbrev-ref", "HEAD"] => b"lean".to_vec(),
                ["git", "rev-parse", "HEAD"] => b"2222222222222222".to_vec(),
                ["gh", "workflow", "run", rest @ ..] => {
                    self.dispatched.borrow_mut().push(rest.join(" "));
                    vec![]
                }
                ["gh", "api", endpoint] if endpoint.contains("/runs?") => {
                    let mut listed = self.listed.clone();
                    // The run started by the dispatch appears once it was asked for.
                    if !self.dispatched.borrow().is_empty() {
                        listed["workflow_runs"]
                            .as_array_mut()
                            .unwrap()
                            .push(self.run.clone());
                    }
                    listed.to_string().into_bytes()
                }
                ["gh", "api", endpoint] if endpoint.ends_with("/actions/runs/9") => {
                    json!({"id": 9, "status": "completed", "conclusion": "success"})
                        .to_string()
                        .into_bytes()
                }
                other => panic!("unexpected {other:?}"),
            })
        }
    }
    #[test]
    fn one_features_removability_is_its_own_run_of_the_weekly_workflow() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("features/uniforms")).unwrap();
        let removal = |listed: Value| GitHub {
            dispatched: RefCell::default(),
            listed,
            run: json!({"id": 9, "display_title": "Removability without uniforms",
                "status": "queued", "html_url": "https://github.test/runs/9"}),
        };
        let follow = |github: &GitHub| {
            removability(
                temp.path(),
                "lean",
                "uniforms",
                github,
                &|_| {},
                &mut |_| {},
            )
        };
        // Another feature's run, and the weekly one of every feature, are not its own: it starts
        // one for the feature alone and follows it.
        let others = json!({"workflow_runs": [
            {"id": 7, "display_title": "Removability without dvic", "status": "completed",
                "conclusion": "success"},
            {"id": 8, "display_title": "Removability", "status": "completed",
                "conclusion": "success"},
        ]});
        let github = removal(others);
        assert_eq!(
            follow(&github).unwrap(),
            "The removability checks of uniforms passed on 2222222 of lean."
        );
        assert_eq!(
            *github.dispatched.borrow(),
            [format!(
                "removability.yml --repo {REPOSITORY} --ref lean -f feature=uniforms"
            )]
        );
        // Its own that passed on this head is reported, not run again.
        let passed = removal(json!({"workflow_runs": [
            {"id": 6, "display_title": "Removability without uniforms", "status": "completed",
                "conclusion": "success", "html_url": "https://github.test/runs/6"},
        ]}));
        assert!(
            follow(&passed)
                .unwrap()
                .ends_with("already: https://github.test/runs/6")
        );
        assert!(passed.dispatched.borrow().is_empty());
        // Its runs are found by the name the workflow gives a run of one feature.
        let workflow = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/removability.yml"),
        )
        .unwrap();
        let title = Workflow::removability("{0}").title.unwrap();
        assert!(
            workflow.contains(&format!(
                "run-name: ${{{{ inputs.feature && format('{title}', inputs.feature)"
            )),
            "removability.yml names a run of one feature otherwise than {title}"
        );
        // A feature the checkout doesn't have is refused before anything runs.
        let github = removal(json!({"workflow_runs": []}));
        assert!(
            removability(
                temp.path(),
                "lean",
                "parking",
                &github,
                &|_| {},
                &mut |_| {}
            )
            .is_err()
        );
        assert!(github.dispatched.borrow().is_empty());
    }

    /// The checks on a branch's head as GitHub answers them: each look at the run takes the
    /// next of `looks`, the last one staying; what was asked of it is in `asked`.
    struct Checks {
        looks: RefCell<Vec<Value>>,
        jobs: Value,
        asked: RefCell<Vec<String>>,
    }
    impl Checks {
        fn new(looks: Vec<Value>, jobs: Value) -> Self {
            Self {
                looks: RefCell::new(looks),
                jobs,
                asked: RefCell::default(),
            }
        }
        fn follow(&self, local: &(dyn Fn() -> Result<()> + Sync)) -> Result<String> {
            let pause = |_| std::thread::sleep(std::time::Duration::from_millis(5));
            follow(
                Path::new("/lean"),
                "lean",
                &Workflow::checks(),
                local,
                self,
                &pause,
                &mut |_| {},
            )
        }
    }
    impl Runner for Checks {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            let asked = |what: String| {
                self.asked.borrow_mut().push(what);
                vec![]
            };
            Ok(match args {
                ["git", "status", ..] | ["git", "push", ..] => vec![],
                ["git", "rev-parse", "--abbrev-ref", "HEAD"] => b"lean".to_vec(),
                ["git", "rev-parse", "HEAD"] => b"2222222222222222".to_vec(),
                ["gh", "workflow", "run", ..] => asked("run".into()),
                ["gh", "run", "rerun", id, "--failed", ..] => asked(format!("rerun {id}")),
                ["gh", "api", "-X", "POST", endpoint] => asked(format!("POST {endpoint}")),
                ["gh", "api", "--allow-escape-sequences", _] => vec![],
                ["gh", "api", endpoint] if endpoint.contains("/runs?") => {
                    let started = self.asked.borrow().iter().any(|what| what == "run");
                    let runs = match started {
                        true => vec![json!({"id": 9, "status": "queued", "run_attempt": 1})],
                        false => vec![],
                    };
                    json!({ "workflow_runs": runs }).to_string().into_bytes()
                }
                ["gh", "api", endpoint] if endpoint.ends_with("/actions/runs/9") => {
                    let mut looks = self.looks.borrow_mut();
                    let look = match looks.len() {
                        1 => looks[0].clone(),
                        _ => looks.remove(0),
                    };
                    look.to_string().into_bytes()
                }
                ["gh", "api", endpoint] if endpoint.contains("/jobs") => {
                    self.jobs.to_string().into_bytes()
                }
                ["gh", "api", endpoint] if endpoint.ends_with("/check-runs/5/annotations") => {
                    json!([{"message": "The job has exceeded the maximum execution time of 15m0s"}])
                        .to_string()
                        .into_bytes()
                }
                ["gh", "api", endpoint] if endpoint.ends_with("/annotations") => b"[]".to_vec(),
                other => panic!("unexpected {other:?}"),
            })
        }
    }
    fn ended(conclusion: &str, attempt: u64) -> Value {
        json!({"id": 9, "status": "completed", "conclusion": conclusion, "run_attempt": attempt})
    }
    #[test]
    fn a_run_is_followed_to_its_end_and_what_github_stopped_itself_runs_again_once() {
        let passed = || Ok(());
        // The job past its time limit runs again; until the new attempt starts, the run still
        // shows the old one's end.
        let core = json!({"jobs": [{"id": 5, "name": "core", "conclusion": "cancelled"}]});
        let checks = Checks::new(
            vec![
                ended("cancelled", 1),
                ended("cancelled", 1),
                ended("success", 2),
            ],
            core.clone(),
        );
        assert_eq!(
            checks.follow(&passed).unwrap(),
            "The checks passed on 2222222 of lean."
        );
        assert_eq!(*checks.asked.borrow(), ["run", "rerun 9"]);
        // Once only.
        let checks = Checks::new(vec![ended("cancelled", 1), ended("cancelled", 2)], core);
        assert!(checks.follow(&passed).is_err());
        assert_eq!(*checks.asked.borrow(), ["run", "rerun 9"]);
        // One cancelled by hand isn't run again, and every failed job is listed.
        let jobs = json!({"jobs": [
            {"id": 6, "name": "api", "conclusion": "failure"},
            {"id": 7, "name": "browser (3)", "conclusion": "failure"},
            {"id": 8, "name": "smoke", "conclusion": "success"},
        ]});
        let checks = Checks::new(vec![ended("failure", 1)], jobs);
        let error = checks.follow(&passed).unwrap_err().to_string();
        assert!(error.contains("\n- api: failure") && error.contains("\n- browser (3): failure"));
        assert!(error.contains("dispatchdev ci lean --rerun-failed"));
        assert!(!error.contains("smoke"));
        assert_eq!(*checks.asked.borrow(), ["run"]);
    }
    #[test]
    fn the_rule_checks_failing_here_cancel_the_run_on_github() {
        let checks = Checks::new(
            vec![json!({"id": 9, "status": "in_progress", "run_attempt": 1})],
            json!({"jobs": []}),
        );
        let error = checks
            .follow(&|| Err("The rule checks failed.".into()))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("The rule checks failed. Fix it"));
        assert_eq!(
            *checks.asked.borrow(),
            [
                "run".to_owned(),
                format!("POST repos/{REPOSITORY}/actions/runs/9/cancel")
            ]
        );
    }
}
