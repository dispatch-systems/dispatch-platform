//! `dispatchdev owed`: every change in progress, in the platform's worktrees and the agent
//! docs', or the ones named, each with where it stands, its next step and whose move that is, as
//! `status` tells it, and on past its merge: `landed` to wait for, then `finish`. Each change is
//! looked up on its own and at once, and what fails for one is told in its own entry.
use crate::{
    Result, Runner,
    change::{Clone, Repository, valid_name},
    ci::{self, Landed, Outcome},
    merge::short,
    preview,
    status::{self, Whose},
    workspace,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// A change in progress, and what it's owed.
#[derive(Debug, Serialize)]
pub struct Owed {
    pub name: String,
    pub path: PathBuf,
    pub repository: Repository,
    pub branch: String,
    pub head: String,
    pub pr: Option<Pr>,
    /// The dev server `preview` started for it, while it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<preview::Server>,
    /// Its next step, the first of `steps`.
    pub next: String,
    pub whose: Whose,
    /// What comes next, in order, as `status` prints it.
    #[serde(skip)]
    pub steps: Vec<String>,
}
/// A change's PR, open or not.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Pr {
    pub number: u64,
    pub url: String,
    pub state: PrState,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all(serialize = "lowercase", deserialize = "UPPERCASE"))]
pub enum PrState {
    Open,
    Merged,
    Closed,
}
/// A PR as `gh pr list --json` gives it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    number: u64,
    url: String,
    state: PrState,
    head_ref_name: String,
    is_cross_repository: bool,
    merge_commit: Option<Commit>,
}
#[derive(Deserialize)]
struct Commit {
    oid: String,
}

/// A change's worktree, in its clone's folder of them.
#[derive(Debug)]
struct Found {
    name: String,
    path: PathBuf,
    repository: Repository,
}

/// The clones whose changes are looked for, each with its repository: the platform's, which
/// `from` or the CLI's own source is a checkout of, and the agent docs', where `./setup` keeps
/// them.
fn clones(
    from: &Path,
    env: &BTreeMap<String, String>,
    runner: &dyn Runner,
) -> Vec<(Clone, Repository)> {
    let agents = env.get("DISPATCH_AGENTS").map(PathBuf::from).or_else(|| {
        env.get("HOME")
            .map(|home| Path::new(home).join("dispatch-agents"))
    });
    let source = env.get("DISPATCHDEV_SOURCE").map(PathBuf::from);
    let mut clones: Vec<(Clone, Repository)> = vec![];
    for place in [Some(from.to_path_buf()), source, agents]
        .into_iter()
        .flatten()
    {
        let Ok(clone) = Clone::of(&place, runner) else {
            continue;
        };
        if let Ok(repository) = Repository::of(&clone.root, runner)
            && clones.iter().all(|(known, _)| known.root != clone.root)
        {
            clones.push((clone, repository));
        }
    }
    clones
}
/// The changes in progress in `clones`, by their worktrees: the ones `start` made. A clone with
/// no worktrees folder has none; one whose folder can't be read is an error, never an empty
/// list that would read as nothing owed.
fn changes(clones: &[(Clone, Repository)]) -> Result<Vec<Found>> {
    let mut found = vec![];
    for (clone, repository) in clones {
        let entries = match fs::read_dir(&clone.worktrees) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            entries => entries
                .map_err(|error| format!("Couldn't read {}: {error}", clone.worktrees.display()))?,
        };
        let mut names = vec![];
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Couldn't read {}: {error}", clone.worktrees.display()))?;
            if entry.path().join(".git").exists()
                && let Ok(name) = entry.file_name().into_string()
                && valid_name(&name).is_ok()
            {
                names.push(name);
            }
        }
        names.sort();
        found.extend(names.into_iter().map(|name| Found {
            path: clone.worktrees.join(&name),
            name,
            repository: *repository,
        }));
    }
    Ok(found)
}

/// What each change in progress is owed, or each named one in progress, side by side.
pub fn owed(
    from: &Path,
    names: &[&str],
    env: &BTreeMap<String, String>,
    now: SystemTime,
    runner: &(dyn Runner + Sync),
) -> Result<Vec<Owed>> {
    for name in names {
        valid_name(name)?;
    }
    let mut found = changes(&clones(from, env, runner))?;
    if !names.is_empty() {
        found.retain(|change| names.contains(&change.name.as_str()));
    }
    Ok(std::thread::scope(|scope| {
        let looking: Vec<_> = found
            .iter()
            .map(|change| scope.spawn(move || entry(change, now, runner)))
            .collect();
        looking
            .into_iter()
            .map(|looked| looked.join().expect("a change's lookup"))
            .collect()
    }))
}

/// What the change is owed, or the error that kept it from being known.
fn entry(change: &Found, now: SystemTime, runner: &dyn Runner) -> Owed {
    let git = |args: &[&str]| workspace::git(runner, &change.path, args).unwrap_or_default();
    let mut owed = Owed {
        name: change.name.clone(),
        path: change.path.clone(),
        repository: change.repository,
        branch: git(&["branch", "--show-current"]),
        head: git(&["rev-parse", "HEAD"]),
        pr: None,
        server: workspace::current_scratch(&change.name)
            .ok()
            .and_then(|scratch| preview::server(&scratch, runner)),
        next: String::new(),
        whose: Whose::Agent,
        steps: vec![],
    };
    match standing(&mut owed, now, runner) {
        Ok((steps, whose)) => {
            owed.steps = steps;
            owed.whose = whose;
        }
        Err(error) => owed.steps = vec![error.to_string()],
    }
    owed.next = owed.steps.first().cloned().unwrap_or_default();
    owed
}
/// What comes next for the change, and whose move it is, from its PR in any state.
fn standing(owed: &mut Owed, now: SystemTime, runner: &dyn Runner) -> Result<(Vec<String>, Whose)> {
    let merged = match owed.branch.as_str() {
        "" => None,
        branch => {
            let (pr, merged) = pull(&owed.path, owed.repository, branch, runner)?;
            owed.pr = pr;
            merged
        }
    };
    let settled = match &owed.pr {
        Some(pr) if pr.state != PrState::Open => Some(settled(
            &owed.name,
            owed.repository,
            pr.state,
            merged.as_deref(),
            ci::landing(&owed.name)?.as_ref(),
        )),
        _ => None,
    };
    Ok(match settled {
        Some(next) => (vec![next], Whose::Agent),
        None => {
            let state = status::state(&owed.path, now, runner)?;
            (state.next(), state.whose())
        }
    })
}
/// The branch's PR in `repository`, the open one first, else its latest, and the commit it
/// merged as.
fn pull(
    root: &Path,
    repository: Repository,
    branch: &str,
    runner: &dyn Runner,
) -> Result<(Option<Pr>, Option<String>)> {
    let listed: Vec<Listed> = serde_json::from_str(&workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            repository.name(),
            "--head",
            branch,
            "--state",
            "all",
            "--json",
            "number,url,state,headRefName,isCrossRepository,mergeCommit",
        ],
        Some(root),
    )?)?;
    let mut own: Vec<Listed> = listed
        .into_iter()
        .filter(|pull| pull.head_ref_name == branch && !pull.is_cross_repository)
        .collect();
    // GitHub lists the newest first.
    own.sort_by_key(|pull| pull.state != PrState::Open);
    let latest = own.into_iter().next();
    Ok(latest.map_or((None, None), |pull| {
        let pr = Pr {
            number: pull.number,
            url: pull.url,
            state: pull.state,
        };
        (Some(pr), pull.merge_commit.map(|commit| commit.oid))
    }))
}
/// What comes next for the change named `name` whose PR is no longer open: it merged as
/// `merged`, and `landed` left `landing`, or it closed unmerged.
fn settled(
    name: &str,
    repository: Repository,
    state: PrState,
    merged: Option<&str>,
    landing: Option<&Landed>,
) -> String {
    let finish =
        |flag: &str| format!("From outside the worktree, `dispatchdev finish {name}{flag}`");
    if state != PrState::Merged {
        return finish(" --abandon");
    }
    // Nothing runs on the agent docs' `main`.
    if repository == Repository::AgentDocs {
        return finish("");
    }
    match landing.filter(|landed| Some(landed.commit.as_str()) == merged) {
        Some(Landed {
            result: Outcome::Passed,
            ..
        }) => finish(""),
        Some(Landed { run, .. }) => format!(
            "main's run {run} of the merge failed: a flake unrelated to it runs again once, `gh \
             run rerun {run} --failed`, then `dispatchdev landed` again; what it broke is fixed \
             in its own PR off main, and once that one's `landed` passed, `dispatchdev finish \
             {name}`"
        ),
        None => {
            "`dispatchdev landed`, in the background, waits for main's run of the merge".to_owned()
        }
    }
}

/// `dispatchdev owed`.
pub fn run(
    from: &Path,
    names: &[&str],
    json: bool,
    env: &BTreeMap<String, String>,
    runner: &(dyn Runner + Sync),
) -> Result<()> {
    let owed = owed(from, names, env, SystemTime::now(), runner)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&owed)?);
        return Ok(());
    }
    let mut blocks: Vec<String> = owed.iter().map(block).collect();
    for name in names {
        if owed.iter().all(|change| change.name != *name) {
            blocks.push(format!("No change named {name} is in progress."));
        }
    }
    if blocks.is_empty() {
        blocks.push("No changes in progress.".to_owned());
    }
    println!("{}", blocks.join("\n\n"));
    Ok(())
}
/// A change's lines: where it is, its PR, what comes next and whose move that is.
fn block(owed: &Owed) -> String {
    let repository = match owed.repository {
        Repository::Platform => "platform",
        Repository::AgentDocs => "agent docs",
    };
    let pr = match &owed.pr {
        None => "PR: none".to_owned(),
        Some(pr) => format!(
            "PR: #{} {}, {}",
            pr.number,
            pr.url,
            match pr.state {
                PrState::Open => "open",
                PrState::Merged => "merged",
                PrState::Closed => "closed, unmerged",
            }
        ),
    };
    let whose = match owed.whose {
        Whose::Agent => "Whose move: yours",
        Whose::Owner => "Whose move: the user's",
        Whose::Github => "Whose move: GitHub's",
        Whose::Reviewers => "Whose move: its reviewers'",
        Whose::Ready => "Ready to merge",
    };
    [
        format!(
            "{} ({repository}): {} at {}, {}",
            owed.name,
            match owed.branch.as_str() {
                "" => "no branch",
                branch => branch,
            },
            short(&owed.head),
            owed.path.display()
        ),
        pr,
    ]
    .into_iter()
    .chain(owed.server.as_ref().map(preview::Server::line))
    .chain([workspace::next(&owed.steps), whose.to_owned()])
    .collect::<Vec<_>>()
    .join("\n")
}

#[cfg(test)]
mod tests;
