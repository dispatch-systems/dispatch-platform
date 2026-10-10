//! `dispatchdev new feature <name> [options]`, `new collector <site> [options]` and `new tool
//! <name> [options]`: a feature, a collector or an agent tool in the MCP that works as
//! written, made in a worktree and listed in the app, with what is generated from it written too. The checkout's own scaffolder makes it and says what each
//! option adds (`tooling/scaffold/`); then the catalog, the API types and the snapshots are
//! written for it, a line each, as `dispatchdev check` runs its steps.
use crate::{Native, Result, require, test, workspace::Workspace};
use std::{path::Path, process::Command};

pub const USAGE: &str = "Usage: dispatchdev new feature <name> [options] | new collector <site> [options] | new tool <name> [options]. Each with no name lists its options.";
/// What `new` makes, each with its scaffolder.
const KINDS: &[(&str, &str)] = &[
    ("feature", "tooling/scaffold/new-feature.ts"),
    ("collector", "tooling/scaffold/new-collector.ts"),
    ("tool", "tooling/scaffold/new-tool.ts"),
];
/// What the scaffolder leaves to commands, run once it has written its files.
const GENERATE: &[&str] = &["npm run contracts:generate", "npm run snapshots:update"];

/// Makes a `kind` in the worktree `root`, the scaffolder taking `args` as they were given. It
/// answers whether every step passed.
pub fn run(root: &Path, kind: &str, args: &[String]) -> Result<bool> {
    let script = KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, script)| *script)
        .ok_or(USAGE)?;
    // Dev's checkout stays as main has it; a change starts with `dispatchdev start`.
    let ws = Workspace::find(root, &Native)?;
    require(
        root.starts_with(ws.root.join("worktrees")),
        "Make it in a change's worktree: dispatchdev start <name>, then run this there.",
    )?;
    let made = Command::new("node")
        .args(["node_modules/tsx/dist/cli.mjs", script])
        .args(args)
        .current_dir(root)
        .status()?;
    if !made.success() {
        return Ok(false);
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        return Ok(true);
    }
    let commands: Vec<String> = GENERATE.iter().map(|&command| command.to_owned()).collect();
    let passed = test::run_commands(root, &commands, false)?;
    if passed {
        println!(
            "Made it, with its catalog, API types and snapshots written: git status lists every file."
        );
    }
    Ok(passed)
}
