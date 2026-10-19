//! `dispatchdev ci <name>`: the full checks on a change's branch while its preview is tried,
//! so what they find is fixed before the PR is queued. It pushes the committed branch, starts
//! `checks.yml` on it without the gate, and follows the run, stopping at its first failed job.
//! A head that has a run going or passed already is followed or reported, not run again. A newer
//! run of the branch replaces an older one; `ship` stops on a head whose run failed and cancels
//! one still going, since the queue runs the checks again on the squash commit.
//! `--removability <feature>` runs and follows the weekly `removability.yml` the same way, for
//! that feature alone: its runs are told apart by the name the workflow gives them.
use crate::{
    REPOSITORY, Result, Runner, require,
    ship::{ATTEMPTS, PAUSE, failed_jobs, rest},
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::path::Path;

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
/// removability, and follow them; returns the verdict.
pub fn run(
    ws: &Workspace,
    name: &str,
    removability: Option<&str>,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<String> {
    workspace::valid_name(name)?;
    let path = ws.worktree(name);
    require(path.is_dir(), &format!("worktrees/{name} doesn't exist."))?;
    match removability {
        Some(feature) => self::removability(&path, name, feature, runner, pause, say),
        None => follow(&path, name, &Workflow::checks(), runner, pause, say),
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
        runner,
        pause,
        say,
    )
}
fn follow(
    path: &Path,
    name: &str,
    workflow: &Workflow,
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
    git(&["push", "-q", "-u", "origin", name])?;
    let earlier = runs(runner, workflow, name, &head)?;
    let reused = earlier
        .iter()
        .find(|run| run["status"] != "completed" || run["conclusion"] == "success");
    let run = match reused {
        Some(run) if run["status"] == "completed" => {
            return Ok(format!(
                "{what} passed on {short} of {name} already: {}",
                run["html_url"].as_str().unwrap_or("")
            ));
        }
        Some(run) => run.clone(),
        None => {
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
            let mut started = None;
            for _ in 0..APPEAR_LOOKS {
                pause(PAUSE);
                started = runs(runner, workflow, name, &head)?
                    .into_iter()
                    .find(|run| run["id"].as_u64().unwrap_or(0) > newest);
                if started.is_some() {
                    break;
                }
            }
            started.ok_or_else(|| {
                format!(
                    "GitHub started no run of {} in two minutes; run it again.",
                    workflow.named()
                )
            })?
        }
    };
    let id = run["id"].as_u64().ok_or("The run has no id")?;
    say(&format!(
        "{} on {short} of {name}: {}",
        workflow.heading(),
        run["html_url"].as_str().unwrap_or("")
    ));
    let mut trouble = 0;
    for _ in 0..RUN_LOOKS {
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
        let done = run["status"] == "completed";
        match run["conclusion"].as_str() {
            Some("success") if done => {
                return Ok(format!("{what} passed on {short} of {name}."));
            }
            Some("cancelled") if done => {
                return Err(format!(
                    "{what} on {short} of {name} were cancelled, by a newer run of the branch or by hand."
                )
                .into());
            }
            _ => {}
        }
        let failed = failed_jobs(runner, &run, !done);
        if !failed.is_empty() {
            return Err(format!(
                "{what} failed on {short} of {name}. Fix them, commit and run them again. Failed jobs{}:\n- {}",
                if done { "" } else { " so far" },
                failed.join("\n- ")
            )
            .into());
        }
        if done {
            return Err(format!(
                "{what} on {short} of {name} ended {}: {}",
                run["conclusion"].as_str().unwrap_or("without a conclusion"),
                run["html_url"].as_str().unwrap_or("")
            )
            .into());
        }
        pause(PAUSE);
    }
    Err(format!("{what} on {short} of {name} have not ended after an hour.").into())
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
}
