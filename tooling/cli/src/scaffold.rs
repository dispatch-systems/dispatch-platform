//! `dispatchdev new`, `generate`, `list` and `explain`: making a feature, a collector, an agent
//! tool or a kind of slot, writing what is generated from what the owners declare, and reading
//! what the product has. The checkout's own `tooling/scaffold/cli.ts` runs each, with its own
//! options; here a change's worktree is required of what writes, since Dev's checkout stays as
//! main has it.
use crate::{Native, Result, require, workspace::Workspace};
use std::{path::Path, process::Command};

const CLI: &str = "tooling/scaffold/cli.ts";

/// Whether `command` with `args` writes into the checkout: `new` with a kind and a name, without
/// --dry-run, and `generate` without --check; neither when it only asks for its usage.
pub fn writes(command: &str, args: &[String]) -> bool {
    let asks = |flag: &str| args.iter().any(|arg| arg == flag);
    if asks("--help") || asks("-h") {
        return false;
    }
    match command {
        "new" => !asks("--dry-run") && args.iter().filter(|arg| !arg.starts_with('-')).count() >= 2,
        "add" => !asks("--dry-run") && !args.is_empty(),
        "generate" => !asks("--check"),
        _ => false,
    }
}

/// Runs `command` in the checkout `root`, its scaffolder taking `args` as they were given. It
/// answers whether it passed.
pub fn run(root: &Path, command: &str, args: &[String]) -> Result<bool> {
    if writes(command, args) {
        let ws = Workspace::find(root, &Native)?;
        // By their real paths: worktrees/ may link to a disk of its own, where git finds the
        // checkout.
        let worktrees = std::fs::canonicalize(ws.root.join("worktrees"))?;
        require(
            std::fs::canonicalize(root)?.starts_with(worktrees),
            "Make it in a change's worktree: dispatchdev start <name>, then run this there.",
        )?;
    }
    require(
        root.join(CLI).is_file(),
        "This checkout predates dispatchdev's scaffolder: rebase it onto origin/main.",
    )?;
    let status = Command::new("node")
        .args(["node_modules/tsx/dist/cli.mjs", CLI, command])
        .args(args)
        .current_dir(root)
        .status()?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_what_writes_needs_a_worktree() {
        let args = |line: &str| {
            line.split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        assert!(writes("new", &args("feature parking --page")));
        assert!(writes("generate", &args("--json")));
        assert!(writes("add", &args("settings --to uniforms")));
        for (command, line) in [
            ("new", ""),
            ("new", "feature"),
            ("new", "feature parking --dry-run"),
            ("new", "feature parking --help"),
            ("add", "settings --to uniforms --dry-run"),
            ("add", ""),
            ("generate", "--help"),
            ("generate", "--check"),
            ("list", "features"),
            ("explain", "slack"),
        ] {
            assert!(!writes(command, &args(line)), "{command} {line}");
        }
    }
}
