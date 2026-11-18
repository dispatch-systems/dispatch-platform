//! `dispatchdev merge`: the checkout's PR, squash-merged once nothing stops it. Its required
//! check passed on the pushed head, where its repository has one, the body on GitHub is the
//! one written, and its review is done at that head, or at one that head only merges `main`
//! into, or says why it had none. Then it prints what to tell the user.
use crate::{Result, Runner, change::Repository, github, pr, require, review, workspace};
use serde_json::Value;
use std::path::Path;

/// The one check a PR to the platform needs to pass to merge.
pub const REQUIRED: &str = "platform";
/// The check a PR to `repository` needs to pass to merge: the agent docs run none.
pub fn required(repository: Repository) -> Option<&'static str> {
    (repository == Repository::Platform).then_some(REQUIRED)
}

/// A required check's state on a PR's head.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    Passed,
    Running,
    Failed,
    /// It hasn't started on this head.
    Missing,
}
/// The open PR of a branch, as GitHub has it.
pub struct Pull {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    pub head: String,
    /// Its required check, where its repository has one.
    pub required: Option<Check>,
}
/// The branch's open PR in `repository`, if it has one.
pub fn pull(
    root: &Path,
    repository: Repository,
    branch: &str,
    runner: &dyn Runner,
) -> Result<Option<Pull>> {
    let text = github::call(
        runner,
        root,
        &[
            "pr",
            "list",
            "--repo",
            repository.name(),
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number,url,title,body,headRefOid,statusCheckRollup",
        ],
    )?;
    parsed(&text, required(repository))
}
/// The first PR of `gh pr list --json` output, with the state of its check named `required`.
/// A commit keeps the checks of every run it had, as when its PR was closed and reopened: the
/// check counts in the newest run of its workflow alone, as it does for GitHub.
fn parsed(text: &str, required: Option<&str>) -> Result<Option<Pull>> {
    let pulls: Vec<Value> = serde_json::from_str(text)?;
    let Some(found) = pulls.first() else {
        return Ok(None);
    };
    let text = |key: &str| found[key].as_str().unwrap_or_default().to_owned();
    let checks = found["statusCheckRollup"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    // The run a check belongs to, from the address of its job.
    let run = |check: &Value| -> Option<u64> {
        let after = check["detailsUrl"].as_str()?.split("/runs/").nth(1)?;
        after.split('/').next()?.parse().ok()
    };
    let required = required.map(|required| {
        let named = || checks.iter().filter(|check| check["name"] == required);
        let workflow = named().next().map(|check| &check["workflowName"]);
        let newest = checks
            .iter()
            .filter(|check| Some(&check["workflowName"]) == workflow)
            .filter_map(run)
            .max();
        named()
            .find(|check| run(check) == newest)
            .map_or(Check::Missing, |check| {
                match (check["status"].as_str(), check["conclusion"].as_str()) {
                    (Some("COMPLETED"), Some("SUCCESS")) => Check::Passed,
                    (Some("COMPLETED"), _) => Check::Failed,
                    _ => Check::Running,
                }
            })
    });
    Ok(Some(Pull {
        number: found["number"].as_u64().ok_or("A PR without a number")?,
        url: text("url"),
        title: text("title"),
        body: text("body"),
        head: text("headRefOid"),
        required,
    }))
}

/// Why the review of the change at `head` doesn't let it merge, if anything: from the Review
/// line its body opens with.
pub fn unreviewed(
    root: &Path,
    body: &str,
    head: &str,
    runner: &dyn Runner,
) -> Result<Option<String>> {
    let line = pr::review_line(body).unwrap_or_default();
    if line.starts_with("Not reviewed:") {
        return Ok(None);
    }
    if line == pr::LIGHT {
        let judged = review::judged(root, runner)?;
        return Ok((judged.tier != review::tier::Tier::Light).then(|| {
            format!(
                "The change is {} now, not light: `dispatchdev review prepare --pass 1`, or say \
                 `Not reviewed:` and why.",
                judged.tier
            )
        }));
    }
    Ok(match review::progress(root)? {
        review::Progress::Recorded {
            done: true,
            head: at,
            ..
        } if !at.is_empty()
            && review::later(root, &at, head, runner)? == review::tier::Later::Nothing =>
        {
            None
        }
        review::Progress::Recorded {
            done: true, pass, ..
        } => Some(format!(
            "Pass {pass} was recorded before the latest commits: `dispatchdev review prepare \
             --pass {}` reviews them.",
            pass + 1
        )),
        review::Progress::Recorded { pass, .. } => Some(format!(
            "Pass {pass} asks for another: `dispatchdev review prepare --pass {}`.",
            pass + 1
        )),
        review::Progress::Answering(pass) => Some(format!(
            "Pass {pass}'s findings aren't answered: `dispatchdev review take --pass {pass}`, \
             then `review record`."
        )),
        review::Progress::Prepared(pass) => Some(format!(
            "Pass {pass}'s reviewers haven't answered: wait for them, then `dispatchdev review \
             take --pass {pass}`."
        )),
        review::Progress::NotStarted => Some(
            "It isn't reviewed: `dispatchdev review prepare --pass 1`, or say `Not reviewed:` \
             and why."
                .to_owned(),
        ),
    })
}

/// `dispatchdev merge`.
pub fn run(root: &Path, runner: &dyn Runner) -> Result<()> {
    let repository = Repository::of(root, runner)?;
    let (branch, head) = review::pushed(root, runner)?;
    let pull = pull(root, repository, &branch, runner)?
        .ok_or("The branch has no open PR: `dispatchdev pr` opens it.")?;
    require(
        pull.head == head,
        "GitHub's PR head isn't this commit yet: wait a moment, or `dispatchdev pr`.",
    )?;
    let written = std::fs::read_to_string(pr::body_path(root)?)?;
    let (title, body) = pr::titled(&written);
    require(
        title == Some(pull.title.as_str()) && body.trim() == pull.body.trim(),
        "The PR's title or body on GitHub isn't the one in pr-body.md: `dispatchdev pr` updates it.",
    )?;
    match pull.required {
        None | Some(Check::Passed) => {}
        Some(Check::Running | Check::Missing) => {
            return Err(format!(
                "`{REQUIRED}` hasn't finished on {}: wait for it.",
                short(&head)
            )
            .into());
        }
        Some(Check::Failed) => {
            return Err(format!(
                "`{REQUIRED}` failed on {}: `dispatchdev ci` says why.",
                short(&head)
            )
            .into());
        }
    }
    if let Some(why) = unreviewed(root, body, &head, runner)? {
        return Err(why.into());
    }
    squash(root, repository, pull.number, &head, runner)?;
    println!("**Merged: {}** (#{})", pull.title, pull.number);
    if let Some(line) = pr::review_line(body) {
        println!("Reviewed: {line}");
    }
    for outcome in review::left(root)? {
        println!("{}: {}", outcome.tag, outcome.what);
    }
    let finish =
        format!("`dispatchdev finish {branch} --plan`, then `dispatchdev finish {branch}`");
    let next = match repository {
        Repository::Platform => vec![
            "Tell the user what merged; Dev installs it once main's run publishes its build"
                .to_owned(),
            "`dispatchdev landed`, in the background, waits for main's run of the merge: it is \
             yours until that run passes"
                .to_owned(),
            format!("Once it passed, from outside the worktree, {finish}"),
        ],
        // Nothing runs on the docs' `main`: `finish` brings the clone, where every session reads
        // them, up to it.
        Repository::AgentDocs => vec![
            "Tell the user what merged".to_owned(),
            format!(
                "From outside the worktree, {finish}, which brings the clone every session \
                 reads the docs from up to it"
            ),
        ],
    };
    println!("{}", workspace::next(&next));
    Ok(())
}

/// Squash-merges PR `number` of `repository` at `head`. A try whose answer was lost may have
/// merged it, and GitHub refuses to merge it twice: before each try after the first, GitHub is
/// asked whether it merged at `head`.
fn squash(
    root: &Path,
    repository: Repository,
    number: u64,
    head: &str,
    runner: &dyn Runner,
) -> Result<()> {
    let number = number.to_string();
    let repo = repository.name();
    let merged = || -> Result<Option<String>> {
        let state = github::call(
            runner,
            root,
            &[
                "pr",
                "view",
                &number,
                "--repo",
                repo,
                "--json",
                "state,headRefOid",
                "--jq",
                r#".state + " " + .headRefOid"#,
            ],
        )?;
        Ok((state == format!("MERGED {head}")).then(String::new))
    };
    github::once(
        runner,
        root,
        &[
            "pr",
            "merge",
            &number,
            "--repo",
            repo,
            "--squash",
            "--match-head-commit",
            head,
        ],
        &merged,
    )?;
    Ok(())
}

pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// GitHub as a merge asks it: the merge's answer, then that of each merge tried again, and
    /// the PR's state and head as `gh pr view` tells them.
    struct Merging {
        merges: RefCell<Vec<&'static str>>,
        state: &'static str,
        asked: RefCell<Vec<String>>,
    }
    impl Runner for Merging {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            self.asked.borrow_mut().push(args[1..3].join(" "));
            match args[..3] {
                ["gh", "pr", "merge"] => match self.merges.borrow_mut().pop() {
                    Some("") | None => Ok(vec![]),
                    Some(failure) => Err(failure.into()),
                },
                ["gh", "pr", "view"] => Ok(self.state.as_bytes().to_vec()),
                _ => unreachable!("{args:?}"),
            }
        }
        fn pause(&self, _: std::time::Duration) {}
    }
    #[test]
    fn a_merge_whose_answer_was_lost_is_tried_again_only_if_it_did_not_merge() {
        let lost = "gh pr merge failed: HTTP 502: Bad Gateway";
        let merge = |merges: &[&'static str], state| {
            let github = Merging {
                merges: RefCell::new(merges.iter().rev().copied().collect()),
                state,
                asked: RefCell::new(vec![]),
            };
            let result = squash(Path::new("."), Repository::Platform, 7, "abc", &github);
            (
                result.map_err(|error| error.to_string()),
                github.asked.take(),
            )
        };
        // GitHub merged it before the answer was lost: it isn't merged again, which GitHub
        // would refuse.
        assert_eq!(
            merge(&[lost], "MERGED abc"),
            (Ok(()), vec!["pr merge".into(), "pr view".into()])
        );
        // It didn't: the merge is tried again.
        assert_eq!(
            merge(&[lost, ""], "OPEN abc"),
            (
                Ok(()),
                vec!["pr merge".into(), "pr view".into(), "pr merge".into()]
            )
        );
        // Merged at another head isn't this merge.
        assert_eq!(merge(&[lost, ""], "MERGED def").1.len(), 3);
        // A refusal, such as a conflict, is never tried again, nor asked about.
        let conflict = "gh pr merge failed: Pull request #7 is not mergeable";
        assert_eq!(
            merge(&[conflict], "OPEN abc"),
            (Err(conflict.into()), vec!["pr merge".into()])
        );
    }
    #[test]
    fn the_required_check_is_read_from_the_heads_rollup() {
        let rollup = |checks: &str| {
            format!(
                r#"[{{"number":833,"url":"u","title":"t","body":"b","headRefOid":"abc",
                    "statusCheckRollup":[{{"name":"changes","status":"COMPLETED","conclusion":"FAILURE"}}{checks}]}}]"#
            )
        };
        let pull = |checks: &str| {
            parsed(&rollup(checks), required(Repository::Platform))
                .unwrap()
                .unwrap()
                .required
        };
        assert_eq!(
            pull(""),
            Some(Check::Missing),
            "another job failing isn't the gate"
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"IN_PROGRESS","conclusion":""}"#),
            Some(Check::Running)
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"COMPLETED","conclusion":"SUCCESS"}"#),
            Some(Check::Passed)
        );
        assert_eq!(
            pull(r#",{"name":"platform","status":"COMPLETED","conclusion":"CANCELLED"}"#),
            Some(Check::Failed)
        );
        // A commit whose PR was reopened has two runs: the older one's verdict, a gate that
        // failed because the reopening cancelled its jobs, no longer counts, nor does a newer
        // run of another workflow.
        let check = |workflow: &str, run: u64, name: &str, state: &str| {
            format!(
                r#",{{"name":"{name}","workflowName":"{workflow}",{state},
                    "detailsUrl":"https://github.com/o/r/actions/runs/{run}/job/1"}}"#
            )
        };
        let failed = r#""status":"COMPLETED","conclusion":"FAILURE""#;
        let passed = r#""status":"COMPLETED","conclusion":"SUCCESS""#;
        let older = check("Platform checks", 5, "platform", failed);
        let caches = check("PR caches", 6, "delete", passed);
        let newer = |name, state| check("Platform checks", 7, name, state);
        assert_eq!(pull(&older), Some(Check::Failed));
        assert_eq!(
            pull(&format!("{older}{caches}{}", newer("changes", passed))),
            Some(Check::Missing),
            "the newer run hasn't reached its gate"
        );
        assert_eq!(
            pull(&format!("{older}{caches}{}", newer("platform", passed))),
            Some(Check::Passed)
        );
        assert_eq!(
            pull(&format!(
                "{}{}",
                newer("platform", failed),
                check("Platform checks", 5, "platform", passed)
            )),
            Some(Check::Failed),
            "whichever GitHub lists first"
        );
        // The agent docs run no checks, so none is waited for.
        let docs = parsed(&rollup(""), required(Repository::AgentDocs))
            .unwrap()
            .unwrap();
        assert_eq!(docs.required, None);
        assert!(parsed("[]", Some(REQUIRED)).unwrap().is_none());
    }
}
