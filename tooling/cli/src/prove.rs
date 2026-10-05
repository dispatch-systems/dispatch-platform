//! `dispatchdev prove`: that a change's tests fail without it and pass with it. They run twice,
//! each time in a build of its own deleted when it ends: first in a throwaway checkout of the
//! commit the branch started from, then in the branch. When the branch changes code, the
//! throwaway checkout takes the branch's tests, so they meet the old code; when it changes only
//! tests, as a fix to a flaky one does, it keeps its own. Nothing in the branch's checkout is
//! touched, and the throwaway checkout goes when the proof ends; the runs' output stays in the
//! change's scratch folder, as the failures it shows point there.
use crate::{
    Result, Runner, check, require,
    test::{self, Run},
    workspace,
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// The throwaway checkout's packages and builds take about 4 GB at their peak.
const NEEDS: u64 = 6_000_000_000;

pub struct Options<'a> {
    /// The test files to prove; none for those the branch changes.
    pub tests: &'a [String],
    /// Only the tests whose names match.
    pub grep: Option<&'a str>,
    /// How many times each side runs each command.
    pub repeat: u32,
    /// Slows the browser tests' pages by this factor (`DISPATCH_CPU_THROTTLE`).
    pub cpu: Option<f64>,
    /// Runs the browser tests' animation frames this many milliseconds late
    /// (`DISPATCH_LATE_FRAMES`).
    pub late_frames: Option<u32>,
}

/// Whether a file is a test's or its support's: anything in a `tests` folder.
pub fn is_test(file: &str) -> bool {
    file.starts_with("tests/") || file.contains("/tests/")
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}
/// `command` running only the tests whose names match `pattern`, for each kind of test.
pub fn narrowed(command: &str, pattern: &str) -> Result<String> {
    let pattern = quote(pattern);
    if command.contains("npm run test:ui -- ") {
        Ok(format!("{command} --grep {pattern}"))
    } else if let Some((head, files)) = command.split_once("npx tsx --test ") {
        Ok(format!(
            "{head}npx tsx --test --test-name-pattern={pattern} {files}"
        ))
    } else if command.starts_with("cargo test ") {
        Ok(format!("{command} -- {pattern}"))
    } else {
        Err(format!("--grep can't narrow {command}.").into())
    }
}
/// The commands that run `tests`, as `dispatchdev check` names them but without the lints or
/// the tests that watch them, and with the rule and dashboard tests, which `check:rules` runs,
/// as the plain Node tests they are.
pub fn commands(
    tests: &[String],
    plan: &Value,
    workspace: &check::Workspace,
    grep: Option<&str>,
) -> Result<Vec<String>> {
    let rules: Vec<&str> = [&plan["dashboard"], &plan["rules"]]
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let (plain, others): (Vec<String>, Vec<String>) = tests
        .iter()
        .cloned()
        .partition(|test| rules.contains(&test.as_str()));
    let mut unwatched = plan.clone();
    if let Some(plan) = unwatched.as_object_mut() {
        plan.remove("watch");
    }
    let mut commands: Vec<String> = check::affected(&others, &unwatched, workspace)
        .into_iter()
        .filter(|command| !command.starts_with("cargo clippy"))
        .collect();
    if !plain.is_empty() {
        commands.push(format!("npx tsx --test {}", plain.join(" ")));
    }
    match grep {
        Some(pattern) => commands
            .iter()
            .map(|command| narrowed(command, pattern))
            .collect(),
        None => Ok(commands),
    }
}
/// How one command did on one side: of `runs`, how many failed, and where the first failure's
/// output is.
struct Outcome {
    failed: u32,
    took: Duration,
    first: Option<PathBuf>,
}
/// Runs each command `repeat` times in `checkout`, packaging the browser tests once, and answers
/// how each did. A command that can't be packaged stops the proof.
fn side(
    checkout: &Path,
    tmp: &Path,
    logs: &Path,
    commands: &[String],
    repeat: u32,
    env: &[(&str, &OsStr)],
) -> Result<Vec<Outcome>> {
    fs::create_dir_all(logs)?;
    let run = Run::start(checkout, tmp)?;
    let mut outcomes = vec![];
    for (index, command) in commands.iter().enumerate() {
        let mut outcome = Outcome {
            failed: 0,
            took: Duration::ZERO,
            first: None,
        };
        for step in test::steps(std::slice::from_ref(command)) {
            if !step.isolated {
                let log = logs.join(format!("{}-package.log", index + 1));
                let (ok, _) = run.step(&step, &log, &[])?;
                if !ok {
                    println!("  {} failed:", step.command);
                    test::explain(&log);
                    return Err("The tests couldn't be packaged.".into());
                }
                continue;
            }
            for attempt in 1..=repeat {
                let log = logs.join(format!("{}-{attempt}.log", index + 1));
                let (ok, took) = run.step(&step, &log, env)?;
                outcome.took += took;
                if !ok {
                    outcome.failed += 1;
                    outcome.first.get_or_insert(log);
                }
            }
        }
        outcomes.push(outcome);
    }
    run.end()?;
    Ok(outcomes)
}
fn report(outcomes: &[Outcome], commands: &[String], repeat: u32) {
    for (outcome, command) in outcomes.iter().zip(commands) {
        let result = match outcome.failed {
            0 => format!("passed {repeat} of {repeat}"),
            failed => format!("failed {failed} of {repeat}"),
        };
        println!(
            "  {result:<16} {:>7}  {}",
            workspace::duration(outcome.took),
            test::label(command)
        );
        if let Some(log) = &outcome.first {
            test::explain(log);
        }
    }
}
/// The commit the branch started from, checked out inside the branch's own build folder, where
/// an interrupted proof's is swept up later. It goes, with Git's record of it, when dropped.
struct Throwaway<'a> {
    origin: PathBuf,
    holder: PathBuf,
    path: PathBuf,
    runner: &'a dyn Runner,
}
impl<'a> Throwaway<'a> {
    fn add(origin: &Path, base: &str, runner: &'a dyn Runner) -> Result<Self> {
        let holder = origin
            .join(".test-build")
            .join(std::process::id().to_string());
        let path = holder.join("before");
        fs::create_dir_all(&holder)?;
        let this = Self {
            origin: origin.to_owned(),
            holder,
            path,
            runner,
        };
        let path = this.path.to_str().ok_or("Non-UTF8 path")?;
        workspace::text(
            runner,
            &["git", "worktree", "add", "-q", "--detach", path, base],
            Some(origin),
        )?;
        Ok(this)
    }
}
impl Drop for Throwaway<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.path.to_str() {
            let _ = self.runner.command(
                &["git", "worktree", "remove", "--force", path],
                Some(&self.origin),
                120,
            );
        }
        let _ = fs::remove_dir_all(&self.holder);
        let _ = self
            .runner
            .command(&["git", "worktree", "prune"], Some(&self.origin), 60);
    }
}
/// `dispatchdev prove`: that every command failed at least once without the change and passed
/// every time with it, or why not.
pub fn run(root: &Path, options: &Options, runner: &dyn Runner) -> Result<()> {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("A checkout without a folder name")?;
    let changed = test::changed(root, runner)?;
    let named: Vec<String> = if options.tests.is_empty() {
        changed
            .iter()
            .filter(|file| is_test(file))
            .cloned()
            .collect()
    } else {
        options.tests.to_vec()
    };
    require(
        !named.is_empty(),
        "The branch changes no tests. Name them: dispatchdev prove <test file>...",
    )?;
    for file in &named {
        require(
            root.join(file).is_file(),
            &format!("{file} isn't a file in this checkout."),
        )?;
    }
    let plan = fs::read_to_string(root.join("tooling/ci/test-plan.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let commands = commands(
        &named,
        &plan,
        &check::Workspace::read(root).unwrap_or_default(),
        options.grep,
    )?;
    require(
        !commands.is_empty(),
        &format!("Nothing here runs {} as a test.", named.join(", ")),
    )?;
    let free = workspace::free_bytes(root)?;
    require(
        free >= NEEDS,
        &format!(
            "Only {} free; the throwaway checkout's builds need about 4 GB.",
            workspace::human(free)
        ),
    )?;
    let cpu = options.cpu.map(|rate| rate.to_string());
    let late = options.late_frames.map(|ms| ms.to_string());
    let env: Vec<(&str, &OsStr)> = [
        ("DISPATCH_CPU_THROTTLE", cpu.as_deref()),
        ("DISPATCH_LATE_FRAMES", late.as_deref()),
    ]
    .into_iter()
    .filter_map(|(key, value)| Some((key, OsStr::new(value?))))
    .collect();
    let scratch = workspace::scratch_dir(name)?;
    let logs = scratch.join("prove").join(std::process::id().to_string());
    let base = workspace::text(
        runner,
        &["git", "merge-base", "origin/main", "HEAD"],
        Some(root),
    )?;
    let code = changed.iter().any(|file| !is_test(file));
    let before = {
        let throwaway = Throwaway::add(root, &base, runner)?;
        if code {
            for file in changed.iter().filter(|file| is_test(file)) {
                let target = throwaway.path.join(file);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(root.join(file), target)?;
            }
        }
        let log = logs.join("npm-ci.log");
        fs::create_dir_all(&logs)?;
        let (installed, _) = workspace::logged(
            "npm ci --silent",
            &throwaway.path,
            &log,
            &[("TMPDIR", scratch.as_os_str())],
        )?;
        require(installed, &format!("npm ci failed; see {}", log.display()))?;
        println!(
            "Before: {}, where the branch started, {}:",
            &base[..8.min(base.len())],
            if code {
                "with the branch's tests"
            } else {
                "with its own tests, as the branch changes only tests"
            }
        );
        let outcomes = side(
            &throwaway.path,
            &scratch,
            &logs.join("before"),
            &commands,
            options.repeat,
            &env,
        )?;
        report(&outcomes, &commands, options.repeat);
        outcomes
    };
    println!("After: the branch:");
    let after = side(
        root,
        &scratch,
        &logs.join("after"),
        &commands,
        options.repeat,
        &env,
    )?;
    report(&after, &commands, options.repeat);
    let caught = before.iter().all(|outcome| outcome.failed > 0);
    let fixed = after.iter().all(|outcome| outcome.failed == 0);
    match (caught, fixed) {
        (true, true) => {
            println!("Proved: the tests fail without the change and pass with it.");
            Ok(())
        }
        (false, _) if code => Err(
            "Not proved: a command passed without the change, so its tests don't show what the change fixes."
                .into(),
        ),
        (false, _) => Err(
            "Not proved: the old tests passed too. Try more --repeat, or --cpu or --late-frames to run as a busy runner does."
                .into(),
        ),
        (true, false) => Err("Not proved: the tests fail with the change too.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tests_and_their_support_live_in_tests_folders() {
        for test in [
            "app/tests/browser/a.spec.ts",
            "core/shell/tests/support/fixtures.ts",
            "tests/x.rs",
        ] {
            assert!(is_test(test), "{test}");
        }
        for code in [
            "app/frontend/main.tsx",
            "core/tests.rs",
            "tooling/cli/src/test.rs",
        ] {
            assert!(!is_test(code), "{code}");
        }
    }
    #[test]
    fn a_pattern_narrows_each_kind_of_test_its_own_way() {
        assert_eq!(
            narrowed("npm run build && npm run test:ui -- a.spec.ts", "it's open").unwrap(),
            r"npm run build && npm run test:ui -- a.spec.ts --grep 'it'\''s open'"
        );
        assert_eq!(
            narrowed(
                "tooling/cli/dispatchdev build && npx tsx --test a.test.ts",
                "open"
            )
            .unwrap(),
            "tooling/cli/dispatchdev build && npx tsx --test --test-name-pattern='open' a.test.ts"
        );
        assert_eq!(
            narrowed("cargo test --locked -p dispatch-core", "schedules::").unwrap(),
            "cargo test --locked -p dispatch-core -- 'schedules::'"
        );
        assert!(narrowed("npm run test:browseros -- --shard paycom", "x").is_err());
    }
    #[test]
    fn only_the_named_tests_run_without_lints_or_watchers() {
        let plan = json!({
            "dashboard": ["app/tests/frontend/a.test.ts"],
            "rules": ["app/tests/rules/b.test.ts"],
            "watch": [{"sources": ["app/tests/api/c.test.ts"], "tests": ["app/tests/browser/d.spec.ts"]}],
        });
        let tests = [
            "app/tests/api/c.test.ts",
            "app/tests/frontend/a.test.ts",
            "app/tests/browser/e.spec.ts",
        ]
        .map(str::to_owned);
        assert_eq!(
            commands(&tests, &plan, &check::Workspace::default(), None).unwrap(),
            [
                "tooling/cli/dispatchdev build && npx tsx --test app/tests/api/c.test.ts",
                "npm run build && npm run test:ui -- app/tests/browser/e.spec.ts",
                "npx tsx --test app/tests/frontend/a.test.ts",
            ]
        );
        assert_eq!(
            commands(
                &tests[2..],
                &plan,
                &check::Workspace::default(),
                Some("pinned")
            )
            .unwrap(),
            ["npm run build && npm run test:ui -- app/tests/browser/e.spec.ts --grep 'pinned'"]
        );
    }
}
