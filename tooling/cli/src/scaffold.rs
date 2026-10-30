//! `dispatchdev new`, `generate`, `list` and `explain`: making a feature, a collector, an agent
//! tool or a kind of slot, writing what is generated from what the owners declare, and reading
//! what the product has. The checkout's own `tooling/scaffold/cli.ts` runs each, with its own
//! options; what writes needs a branch, since `main` changes only by a merged PR.
use crate::{Native, Result, require, workspace};
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

/// Runs `command` in the checkout `root`, its scaffolder taking `args` as they were given, with
/// its temporary files, and those of the tests `generate` runs, in the checkout's scratch
/// folder. It answers whether it passed.
pub fn run(root: &Path, command: &str, args: &[String]) -> Result<bool> {
    if writes(command, args) {
        let branch = workspace::text(
            &Native,
            &["git", "rev-parse", "--abbrev-ref", "HEAD"],
            Some(root),
        )?;
        require(
            !matches!(branch.as_str(), "main" | "HEAD"),
            "Make it on a branch of its own: git switch -c <name>, then run this again.",
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
        .env("TMPDIR", workspace::checkout_scratch(root)?)
        .status()?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_what_writes_needs_a_branch() {
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
