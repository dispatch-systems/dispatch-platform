//! `dispatchdev start <name>`: a worktree for one change, on a branch of the same name from
//! `origin/main` (or `--from`), with its packages installed and its scratch folder made.
use crate::{
    Result, Runner, require,
    workspace::{self, Workspace},
};

/// Below this, a new worktree's build could fill the disk: each takes 13–17 GB.
pub const LOW_DISK: u64 = 20_000_000_000;

pub fn run(ws: &Workspace, name: &str, from: &str, runner: &dyn Runner) -> Result<()> {
    workspace::valid_name(name)?;
    let path = ws.worktree(name);
    require(!path.exists(), &format!("worktrees/{name} exists already."))?;
    let dev = ws.dev();
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&dev));
    require(
        git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ])
        .is_err(),
        &format!("A branch {name} exists already."),
    )?;
    git(&["fetch", "-q", "origin"])?;
    require(
        git(&["ls-remote", "--heads", "origin", name])?.is_empty(),
        &format!("origin has a branch {name} already."),
    )?;
    let base = git(&["rev-parse", "--short", from])?;
    git(&[
        "worktree",
        "add",
        "-q",
        path.to_str().ok_or("Non-UTF8 path")?,
        "-b",
        name,
        from,
    ])?;
    let scratch = workspace::scratch_dir(name)?;
    let log = scratch.join("npm-ci.log");
    let (installed, took) = workspace::logged("npm ci --silent", &path, &log, &scratch)?;
    require(installed, &format!("npm ci failed; see {}", log.display()))?;
    println!(
        "Started {name}: worktrees/{name} on branch {name} from {from} at {base}; packages installed in {}.",
        workspace::duration(took)
    );
    println!("Scratch: {}", scratch.display());
    disk(ws, runner)
}
/// The free disk space, and when it's low, what each worktree's build takes.
pub fn disk(ws: &Workspace, runner: &dyn Runner) -> Result<()> {
    let free = workspace::free_bytes(&ws.root)?;
    if free >= LOW_DISK {
        println!("Disk: {} free.", workspace::human(free));
        return Ok(());
    }
    println!(
        "Disk: only {} free. Builds can take 13–17 GB each; a merged change's go with dispatchdev finish:",
        workspace::human(free)
    );
    for name in ws.worktrees()? {
        if let Some(bytes) = workspace::size(runner, &ws.worktree(&name).join("target")) {
            println!("  worktrees/{name}/target  {}", workspace::human(bytes));
        }
    }
    Ok(())
}
