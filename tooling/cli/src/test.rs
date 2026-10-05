//! `dispatchdev test [--all] [--keep]`: the tests a change needs, built in a folder of the run's
//! own and deleted when it ends, so a worktree keeps no test build and several can test at
//! once. Each command runs below Dev (`workspace::logged`).
use crate::{Result, Runner, check, workspace};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Where a run builds: a folder per run inside the checkout, so the compiler's path policy
/// covers it, ignored by Git.
const BUILDS: &str = ".test-build";

/// One command of a run: in the run's build folder, or, to package the runtime for the browser
/// tests, through the release build cache in the checkout.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub command: String,
    pub isolated: bool,
}
/// The steps of `commands`. Packaging stays in the checkout: a release build anywhere else can't
/// use the build cache, and would compile from scratch on every run.
pub fn steps(commands: &[String]) -> Vec<Step> {
    commands
        .iter()
        .flat_map(|command| match command.strip_prefix("npm run build && ") {
            Some(rest) => vec![
                Step {
                    command: "npm run build".into(),
                    isolated: false,
                },
                Step {
                    command: rest.into(),
                    isolated: true,
                },
            ],
            None => vec![Step {
                command: command.clone(),
                isolated: true,
            }],
        })
        .collect()
}
/// Every file the checkout changed from where it left `origin/main`, committed or not, deleted
/// files aside.
pub fn changed(root: &Path, runner: &dyn Runner) -> Result<Vec<String>> {
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(root));
    let base = git(&["merge-base", "origin/main", "HEAD"])?;
    let mut files: Vec<String> = git(&["diff", "--name-only", "--diff-filter=d", &base])?
        .lines()
        .chain(git(&["ls-files", "--others", "--exclude-standard"])?.lines())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}
/// The tests the change touches, as `dispatchdev check --plan` names them, without the lints.
pub fn affected(root: &Path, runner: &dyn Runner) -> Result<Vec<String>> {
    let plan = fs::read_to_string(root.join("tooling/ci/test-plan.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let workspace = check::Workspace::read(root).unwrap_or_default();
    Ok(check::affected(&changed(root, runner)?, &plan, &workspace)
        .into_iter()
        .filter(|command| !command.starts_with("cargo clippy"))
        .collect())
}
/// Every Rust test, and every Node test the CI api job runs: the browser and collector suites
/// stay in the merge queue.
pub fn everything(root: &Path, runner: &dyn Runner) -> Result<Vec<String>> {
    let listed: Vec<Value> = serde_json::from_str(&workspace::text(
        runner,
        &[
            "node",
            "node_modules/tsx/dist/cli.mjs",
            "tooling/ci/checks.ts",
            "api",
            "--list",
        ],
        Some(root),
    )?)?;
    let node = listed
        .iter()
        .find(|command| command["name"] == "core tests")
        .ok_or("CI's api job lists no core tests")?;
    let quoted: Vec<String> = [&node["command"]]
        .into_iter()
        .chain(node["args"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(|arg| format!("'{}'", arg.replace('\'', r"'\''")))
        .collect();
    Ok(vec![
        "cargo test --locked --workspace".into(),
        format!("tooling/cli/dispatchdev build && {}", quoted.join(" ")),
    ])
}
/// Removes the build folders of runs that ended without cleaning up, killed or interrupted.
fn sweep(builds: &Path) {
    let Ok(entries) = fs::read_dir(builds) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let alive = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .is_some_and(|pid| Path::new(&format!("/proc/{pid}")).exists());
        if !alive {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}
/// Runs `commands` in `root` one at a time, a line each, in a build folder of the run's own. It
/// stops at the first that fails, with the lines saying why. Then everything the run made goes:
/// its build folder, and the release build and package it made for the browser tests, unless
/// `keep`.
pub fn run_commands(root: &Path, commands: &[String], keep: bool) -> Result<bool> {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("A checkout without a folder name")?;
    let scratch = workspace::scratch_dir(name)?;
    let logs = scratch.join("test");
    fs::create_dir_all(&logs)?;
    let builds = root.join(BUILDS);
    sweep(&builds);
    let build = builds.join(std::process::id().to_string());
    fs::create_dir_all(&build)?;
    // What the browser tests' packaging makes in the checkout, if it isn't there already.
    let made: Vec<PathBuf> = ["target/release", ".build"]
        .iter()
        .map(|path| root.join(path))
        .filter(|path| !path.exists())
        .collect();
    let binary = build.join("debug/dispatch-backend");
    let isolated: [(&str, &OsStr); 4] = [
        ("CARGO_TARGET_DIR", build.as_os_str()),
        ("CARGO_INCREMENTAL", OsStr::new("0")),
        ("DISPATCH_TEST_BINARY", binary.as_os_str()),
        ("TMPDIR", scratch.as_os_str()),
    ];
    let mut passed = true;
    for (index, step) in steps(commands).iter().enumerate() {
        let log = logs.join(format!("{}.log", index + 1));
        let env = if step.isolated {
            &isolated[..]
        } else {
            &isolated[3..]
        };
        let (ok, took) = workspace::logged(&step.command, root, &log, env)?;
        let took = workspace::duration(took);
        if ok {
            println!("ok    {took:>7}  {}", step.command);
            continue;
        }
        println!("FAIL  {took:>7}  {}", step.command);
        for line in check::failure(&fs::read_to_string(&log).unwrap_or_default()) {
            println!("      {line}");
        }
        println!("      Whole output: {}", log.display());
        passed = false;
        break;
    }
    if keep {
        println!("Kept the build in {}.", build.display());
        return Ok(passed);
    }
    let mut freed: u64 = 0;
    for path in std::iter::once(build).chain(made.into_iter().filter(|path| path.exists())) {
        freed += workspace::size(&crate::Native, &path).unwrap_or(0);
        fs::remove_dir_all(&path)?;
    }
    let _ = fs::remove_dir(&builds);
    println!("Deleted the run's builds: {}.", workspace::human(freed));
    Ok(passed)
}
/// `dispatchdev test`.
pub fn run(root: &Path, all: bool, keep: bool, runner: &dyn Runner) -> Result<bool> {
    let commands = if all {
        everything(root, runner)?
    } else {
        affected(root, runner)?
    };
    if commands.is_empty() {
        println!("Nothing the diff touches has tests beyond npm run check:rules.");
        return Ok(true);
    }
    let passed = run_commands(root, &commands, keep)?;
    if passed {
        println!("All {} passed.", commands.len());
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_packaging_the_runtime_builds_in_the_checkout() {
        let commands: Vec<String> = [
            "cargo test --locked -p dispatch-core",
            "npm run build && npm run test:ui -- app/tests/browser/a.spec.ts",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            steps(&commands),
            [
                Step {
                    command: "cargo test --locked -p dispatch-core".into(),
                    isolated: true
                },
                Step {
                    command: "npm run build".into(),
                    isolated: false
                },
                Step {
                    command: "npm run test:ui -- app/tests/browser/a.spec.ts".into(),
                    isolated: true
                },
            ]
        );
    }
    #[test]
    fn a_run_builds_in_its_own_folder_and_deletes_it_unless_kept() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("run-test-checkout");
        fs::create_dir_all(&root).unwrap();
        let command = |text: &str| vec![text.to_owned()];
        // Each command sees its run's folder and no incremental builds; the folder goes after.
        let wrote = "echo \"$CARGO_TARGET_DIR $CARGO_INCREMENTAL\" > seen && touch \"$CARGO_TARGET_DIR/built\"";
        assert!(run_commands(&root, &command(wrote), false).unwrap());
        let seen = fs::read_to_string(root.join("seen")).unwrap();
        let (folder, incremental) = seen.trim().split_once(' ').unwrap();
        assert!(
            folder.starts_with(root.join(BUILDS).to_str().unwrap()),
            "{folder}"
        );
        assert_eq!(incremental, "0");
        assert!(!root.join(BUILDS).exists(), "the run's build is deleted");
        // A failure stops the run, and still deletes what it built.
        assert!(
            !run_commands(
                &root,
                &[wrote.to_owned(), "exit 1".into(), "touch never".into()],
                false
            )
            .unwrap()
        );
        assert!(!root.join("never").exists() && !root.join(BUILDS).exists());
        // Kept, it stays for the next run of the same change; a dead run's is swept then.
        assert!(run_commands(&root, &command("true"), true).unwrap());
        let kept = root.join(BUILDS).join(std::process::id().to_string());
        assert!(kept.is_dir());
        let dead = root.join(BUILDS).join("4194999");
        fs::create_dir_all(&dead).unwrap();
        assert!(run_commands(&root, &command("true"), false).unwrap());
        assert!(!dead.exists() && !root.join(BUILDS).exists());
        let _ = fs::remove_dir_all(workspace::scratch("run-test-checkout"));
    }
}
