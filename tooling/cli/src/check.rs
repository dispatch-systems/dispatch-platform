use crate::{
    Result, Runner,
    build::{self, Environment},
    slot::{self, Slot},
    test_selection::{self, Workspace},
    workspace,
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
/// Problems that stop a push: the checks judge a committed branch, and its PR's checks on GitHub
/// judge it merged into `main` as `main` is then.
pub fn blockers(branch: &str, dirty: bool) -> Vec<String> {
    let mut problems = vec![];
    if matches!(branch, "main" | "HEAD") {
        problems.push("Use a branch of its own: git switch -c <name>.".into());
    }
    if dirty {
        problems.push("Commit the changes first: the checks judge what is committed.".into());
    }
    problems
}
/// What a branch needs before it is pushed: the lints its diff touches, against `origin/main`.
/// The tests run on its PR, in parallel on GitHub's runners, faster than here; `dispatchdev
/// test` runs them here while fixing one.
pub struct Plan {
    pub commands: Vec<String>,
    pub base: String,
}
/// The branch's plan, or what stops it from being pushed.
pub fn plan(root: &Path, runner: &dyn Runner) -> Result<Plan> {
    let command = |args: &[&str]| -> Result<String> {
        Ok(String::from_utf8(runner.command(args, Some(root), 120)?)?
            .trim()
            .to_owned())
    };
    let branch = command(&["git", "rev-parse", "--abbrev-ref", "HEAD"])?;
    let dirty = !command(&["git", "status", "--porcelain"])?.is_empty();
    let problems = blockers(&branch, dirty);
    if !problems.is_empty() {
        return Err(format!(
            "The branch needs attention before it is pushed:\n- {}",
            problems.join("\n- ")
        )
        .into());
    }
    let changed: Vec<String> = command(&[
        "git",
        "diff",
        "--name-only",
        "--diff-filter=d",
        "origin/main...HEAD",
    ])?
    .lines()
    .map(str::to_owned)
    .collect();
    // Without a readable plan, the changed tests and crates alone are named.
    let plan = std::fs::read_to_string(root.join("tooling/ci/test-plan.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    // Without a readable workspace, as in a checkout with no Rust, no crate is named.
    let workspace = Workspace::read(root).unwrap_or_default();
    let commands = test_selection::affected(&changed, &plan, &workspace)
        .into_iter()
        .filter(|command| command.starts_with("cargo clippy"))
        .collect();
    Ok(Plan {
        commands,
        base: command(&["git", "rev-parse", "--short", "origin/main"])?,
    })
}
/// What follows a passing check.
pub const NEXT: &str = "Next, dispatchdev pr opens its PR from the scratch folder's \
     pr-body.md, which dispatchdev pr --skeleton --title starts; once its checks on GitHub \
     pass and its review is done, dispatchdev merge merges it.";
/// `dispatchdev check --plan`: what to run, without running it.
pub fn print(plan: &Plan) {
    println!(
        "Run npm run check:rules and npm run lint{} before pushing, against {}. The tests run on \
         the PR.",
        plan.commands
            .iter()
            .map(|command| format!(" and {command}"))
            .collect::<String>(),
        plan.base
    );
    println!("{NEXT}");
}
/// Refuses `dispatchdev <command>`, the local check or tests, at once on a machine whose
/// `python3` can't run them, as `tooling/ci/python.py` judges: `check`, `test` and `verify` ask
/// it before they run a check or a test.
pub fn here(root: &Path, command: &str, runner: &dyn Runner) -> Result<()> {
    runner
        .command(&["python3", "tooling/ci/python.py"], Some(root), 30)
        .map(drop)
        .map_err(|error| format!("dispatchdev {command} can't start: {error}").into())
}
/// The rule checks: everything a push can fail on without a build, side by side, in about a
/// minute. What `dispatchdev check` adds, the TypeScript linter and clippy, the PR's checks run
/// too.
pub const RULES: &str = "npm run check:rules";
/// Everything `dispatchdev check` runs for `plan`.
fn planned(plan: &Plan) -> Vec<String> {
    [RULES.to_owned(), "npm run lint".to_owned()]
        .into_iter()
        .chain(plan.commands.iter().cloned())
        .collect()
}
/// Whether `dispatchdev check` passed on the committed tree as it is: None where it keeps no
/// passes, as `Pass::of` says.
pub fn passed(root: &Path, env: &Environment, runner: &dyn Runner) -> Result<Option<bool>> {
    let plan = plan(root, runner)?;
    Ok(Pass::of(root, &planned(&plan), env, runner)?.map(|pass| pass.passed()))
}
/// `dispatchdev check`: the rule checks and the lints the plan names, side by side, a line for
/// each as it ends; clippy builds in the checkout's own `target`, where its last run left it.
/// A committed tree they all passed on passes again without running, and so does clippy on
/// identical Rust inputs.
pub fn execute(root: &Path, plan: &Plan, env: &Environment, runner: &dyn Runner) -> Result<bool> {
    let planned = planned(plan);
    let pass = Pass::of(root, &planned, env, runner)?;
    if pass.as_ref().is_some_and(Pass::passed) {
        println!("ok     reused  all of them (passed on this commit's tree before)");
        println!("All {} passed against {}.", planned.len(), plan.base);
        return Ok(true);
    }
    let clippy = plan
        .commands
        .iter()
        .find(|command| command.starts_with("cargo clippy"));
    let lints = match clippy {
        Some(command) => Lints::of(root, command, env, runner)?,
        None => None,
    };
    let reused = lints.as_ref().is_some_and(Lints::passed);
    if let (true, Some(command)) = (reused, clippy) {
        println!("ok     reused  {command} (passed on these Rust inputs before)");
    }
    let commands: Vec<String> = planned
        .iter()
        .filter(|command| !reused || Some(*command) != clippy)
        .cloned()
        .collect();
    let passed = side_by_side(root, &commands, env)?;
    if passed {
        if let (false, Some(lints)) = (reused, &lints) {
            lints.keep(root, env, runner)?;
        }
        if let Some(pass) = &pass {
            pass.keep(root, runner)?;
        }
        println!("All {} passed against {}.", planned.len(), plan.base);
    }
    Ok(passed)
}

/// Runs `commands` in `root` side by side, each with its output in the change's scratch folder:
/// a line for each as it ends, with the lines saying why under one that failed. A command that
/// builds waits for a build slot, and passes it on to what it starts. Whether all passed.
fn side_by_side(root: &Path, commands: &[String], env: &Environment) -> Result<bool> {
    let scratch = workspace::checkout_scratch(root)?;
    // Each run's logs apart, so runs side by side never write over each other's.
    let logs = scratch.join("check").join(std::process::id().to_string());
    fs::create_dir_all(&logs)?;
    let _signal_log = crate::signals::run_log(logs.join("1.log"));
    let said = Mutex::new(());
    let passed = std::thread::scope(|scope| {
        let runs: Vec<_> = commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let log = logs.join(format!("{}.log", index + 1));
                let (scratch, said) = (&scratch, &said);
                scope.spawn(move || -> Result<bool> {
                    let slot = match command.starts_with("cargo ") {
                        true => Slot::take(root, command, env, &crate::Native)?,
                        false => None,
                    };
                    let mut own: Vec<(&str, &OsStr)> = vec![("TMPDIR", scratch.as_os_str())];
                    if let Some(slot) = &slot {
                        own.push((slot::HELD, slot.path.as_os_str()));
                    }
                    let (ok, took) = workspace::logged(command, root, &log, &own)?;
                    // A failure's lines stay together under its own.
                    let _said = said.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let took = workspace::duration(took);
                    println!("{}  {took:>7}  {command}", if ok { "ok  " } else { "FAIL" });
                    if !ok {
                        crate::test::explain(&log);
                    }
                    Ok(ok)
                })
            })
            .collect();
        runs.into_iter()
            .map(|run| {
                run.join()
                    .unwrap_or_else(|_| Err("A check stopped unexpectedly.".into()))
            })
            .collect::<Result<Vec<bool>>>()
    })?
    .into_iter()
    .all(|ok| ok);
    crate::signals::finish();
    if passed {
        fs::remove_dir_all(&logs)?;
    }
    Ok(passed)
}

/// Where the check's whole passes are kept: a file for each committed tree it passed on, named
/// by the tree's hash, in the repository's shared Git folder beside clippy's.
const PASSES: &str = "dispatch-checks";
/// A whole check's pass on the committed tree as it is. A check runs only on a committed tree,
/// and its commands judge only what the tree holds, so the same tree passes again without
/// running. What else decides their verdict, the commands themselves and the settings clippy
/// reads, is what the pass's file holds.
pub struct Pass {
    file: PathBuf,
    tree: String,
    says: String,
}
impl Pass {
    /// None when clippy's inputs can't be fingerprinted reliably (`Lints::of`): a target
    /// folder of one's own or `RUSTFLAGS` judge by more than the tree.
    pub fn of(
        root: &Path,
        commands: &[String],
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        if !build::eligible(root, env, false)? {
            return Ok(None);
        }
        let tree = tree(root, runner)?;
        Ok(Some(Self {
            file: shared(root, runner)?.join(PASSES).join(&tree),
            tree,
            says: format!(
                "{}\n{}",
                commands.join("\n"),
                clippy_settings(env).join("\n")
            ),
        }))
    }
    /// Whether the check passed on this tree before.
    pub fn passed(&self) -> bool {
        fs::read_to_string(&self.file).is_ok_and(|text| text == self.says)
    }
    /// Keeps the pass, unless a commit moved the tree while the check ran.
    pub fn keep(&self, root: &Path, runner: &dyn Runner) -> Result<()> {
        if tree(root, runner)? != self.tree {
            return Ok(());
        }
        workspace::write_private(&self.file, &self.says)?;
        keep_newest(&self.file)
    }
}
/// The hash of the tree `root`'s checkout has committed.
fn tree(root: &Path, runner: &dyn Runner) -> Result<String> {
    workspace::text(runner, &["git", "rev-parse", "HEAD^{tree}"], Some(root))
}
/// The repository's Git folder that every worktree shares.
fn shared(root: &Path, runner: &dyn Runner) -> Result<PathBuf> {
    Ok(PathBuf::from(workspace::text(
        runner,
        &[
            "git",
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ],
        Some(root),
    )?))
}
/// The `CLIPPY` settings in `env`, which change what clippy says.
fn clippy_settings(env: &Environment) -> Vec<String> {
    env.iter()
        .filter(|(name, _)| name.starts_with("CLIPPY"))
        .map(|(name, value)| format!("{name}={value}"))
        .collect()
}
/// Lets go of all but the most recent passes in `file`'s folder.
fn keep_newest(file: &Path) -> Result<()> {
    let folder = file.parent().ok_or("No folder for the passes")?;
    let mut kept: Vec<_> = fs::read_dir(folder)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    kept.sort_by(|a, b| b.cmp(a));
    for (_, old) in kept.into_iter().skip(KEPT) {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

/// Where clippy's passes are kept: a file for each set of Rust inputs one passed on, named by
/// their fingerprint, in the repository's shared Git folder, so every worktree reuses them.
const LINTS: &str = "dispatch-clippy";
/// How many passes of clippy, and of the whole check, are kept: the most recent.
const KEPT: usize = 20;
/// A clippy pass over the Rust inputs as they are, by their whole fingerprint: test code and
/// this tool's own source too, which the release binary's key leaves out. What else decides
/// clippy's verdict, its command, a configuration at the top and `CLIPPY` settings, is what
/// the pass's file holds.
pub struct Lints {
    file: PathBuf,
    key: String,
    says: String,
}
impl Lints {
    /// None when the inputs can't be fingerprinted reliably, as with a target folder of one's
    /// own or `RUSTFLAGS`: clippy then always runs.
    pub fn of(
        root: &Path,
        command: &str,
        env: &Environment,
        runner: &dyn Runner,
    ) -> Result<Option<Self>> {
        if !build::eligible(root, env, false)? {
            return Ok(None);
        }
        let key = build::key(root, LINTS, env, runner)?;
        let config: String = ["clippy.toml", ".clippy.toml"]
            .iter()
            .filter_map(|name| fs::read_to_string(root.join(name)).ok())
            .collect();
        Ok(Some(Self {
            file: shared(root, runner)?.join(LINTS).join(&key),
            key,
            says: format!("{command}\n{}\n{config}", clippy_settings(env).join("\n")),
        }))
    }
    /// Whether this clippy passed on these inputs before.
    pub fn passed(&self) -> bool {
        fs::read_to_string(&self.file).is_ok_and(|text| text == self.says)
    }
    /// Keeps the pass, unless the inputs changed while clippy ran, and lets go of the oldest.
    pub fn keep(&self, root: &Path, env: &Environment, runner: &dyn Runner) -> Result<()> {
        if build::key(root, LINTS, env, runner)? != self.key {
            return Ok(());
        }
        workspace::write_private(&self.file, &self.says)?;
        keep_newest(&self.file)
    }
}
/// The lines of a failed command's output that say what failed: failing tests, panics,
/// compiler errors and assertions; or else its last lines.
pub fn failure(log: &str) -> Vec<String> {
    const SIGNS: &[&str] = &[
        "FAILED",
        "panicked at",
        "error[",
        "error:",
        "Error:",
        "  --> ",
        "left:",
        "right:",
        "not ok",
        "\u{2716}",
        "\u{2718}",
        "AssertionError",
        "Expected",
        "Received",
        "Failed rules",
        "Failed checks",
        "check failed",
        "[warn]",
    ];
    let lines: Vec<String> = log.lines().map(plain).collect();
    let mut found: Vec<String> = vec![];
    for line in &lines {
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && (SIGNS.iter().any(|sign| line.contains(sign)) || located(trimmed))
            && found.last().is_none_or(|last| last != trimmed)
        {
            found.push(trimmed.to_owned());
        }
    }
    if found.is_empty() {
        let tail = lines.len().saturating_sub(25);
        return lines[tail..]
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect();
    }
    found.truncate(30);
    found
}
/// Whether a line names a finding by its place, as linters and scanners print one:
/// `path/to/file.rs:12: rule` or `file.ts:3:9: message`. A warning Python prints in the same
/// shape, `mock.py:2259: ResourceWarning: …`, isn't one.
fn located(line: &str) -> bool {
    let Some((place, said)) = line.split_once(": ") else {
        return false;
    };
    if said
        .split_once(": ")
        .is_some_and(|(category, _)| category.ends_with("Warning") && !category.contains(' '))
    {
        return false;
    }
    let mut parts = place.split(':');
    let file = parts.next().unwrap_or("");
    let numbers: Vec<&str> = parts.collect();
    file.contains('.')
        && !file.contains(' ')
        && !said.trim().is_empty()
        && matches!(numbers.len(), 1 | 2)
        && numbers
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}
/// A line without its terminal colour codes.
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    /// A repository whose toolchain and Git folder are stood in for.
    struct Repo(tempfile::TempDir);
    impl Repo {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            fs::create_dir_all(root.join("app/backend")).unwrap();
            fs::create_dir_all(root.join("app/tests/backend")).unwrap();
            fs::create_dir_all(root.join(".git")).unwrap();
            fs::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"app/backend\"]\n",
            )
            .unwrap();
            fs::write(
                root.join("app/backend/Cargo.toml"),
                "[package]\nname = \"fixture\"\n",
            )
            .unwrap();
            fs::write(root.join("app/backend/main.rs"), "fn main() {}").unwrap();
            fs::write(
                root.join("app/tests/backend/case.rs"),
                "#[test] fn case() {}",
            )
            .unwrap();
            Self(temp)
        }
        fn root(&self) -> &Path {
            self.0.path()
        }
        fn lints(&self, command: &str, env: &Environment) -> Lints {
            Lints::of(self.root(), command, env, self).unwrap().unwrap()
        }
    }
    impl Runner for Repo {
        fn command(&self, args: &[&str], _cwd: Option<&Path>, _timeout: u64) -> Result<Vec<u8>> {
            Ok(match args[0] {
                "git" => self.root().join(".git").to_str().unwrap().into(),
                tool => format!("pinned {tool}").into_bytes(),
            })
        }
    }
    #[test]
    fn clippy_passes_again_only_on_the_exact_rust_inputs_and_command_it_passed_on() {
        const CLIPPY: &str = "cargo clippy --locked --all-targets -- -D warnings";
        let repo = Repo::new();
        let env = Environment::from([(
            "CARGO_HOME".into(),
            repo.root().join("cargo-home").to_str().unwrap().into(),
        )]);
        let lints = repo.lints(CLIPPY, &env);
        assert!(!lints.passed());
        lints.keep(repo.root(), &env, &repo).unwrap();
        assert!(repo.lints(CLIPPY, &env).passed());
        // Another command, or clippy settings, judge differently.
        assert!(
            !repo
                .lints("cargo clippy --locked -- -D warnings", &env)
                .passed()
        );
        let mut configured = env.clone();
        configured.insert("CLIPPY_CONF_DIR".into(), "/elsewhere".into());
        assert!(!repo.lints(CLIPPY, &configured).passed());
        fs::write(repo.root().join("clippy.toml"), "msrv = \"1.80\"").unwrap();
        assert!(!repo.lints(CLIPPY, &env).passed());
        fs::remove_file(repo.root().join("clippy.toml")).unwrap();
        // Test code counts, though the release binary's key leaves it out.
        fs::write(
            repo.root().join("app/tests/backend/case.rs"),
            "#[test] fn other() {}",
        )
        .unwrap();
        assert!(!repo.lints(CLIPPY, &env).passed());
        // Inputs that change while clippy runs leave no pass behind.
        let lints = repo.lints(CLIPPY, &env);
        fs::write(repo.root().join("app/backend/main.rs"), "fn main() { () }").unwrap();
        lints.keep(repo.root(), &env, &repo).unwrap();
        assert!(!repo.lints(CLIPPY, &env).passed());
        // A target folder of one's own can't be fingerprinted: clippy always runs.
        let mut own = env.clone();
        own.insert("CARGO_TARGET_DIR".into(), "/tmp/elsewhere".into());
        assert!(
            Lints::of(repo.root(), CLIPPY, &own, &repo)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn a_check_passes_again_only_on_the_tree_and_commands_it_passed_on() {
        /// The repository with its committed tree's hash stood in for.
        struct Tree(Repo, std::cell::RefCell<&'static str>);
        impl Runner for Tree {
            fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
                if args.contains(&"HEAD^{tree}") {
                    return Ok(self.1.borrow().as_bytes().to_vec());
                }
                self.0.command(args, cwd, timeout)
            }
        }
        let tree = Tree(Repo::new(), "aaa".into());
        let root = tree.0.root().to_owned();
        let env = Environment::from([(
            "CARGO_HOME".into(),
            root.join("cargo-home").to_str().unwrap().into(),
        )]);
        let pass = |commands: &[String], env: &Environment| {
            Pass::of(&root, commands, env, &tree).unwrap().unwrap()
        };
        let commands = vec!["npm run check:rules".to_owned(), "npm run lint".to_owned()];
        assert!(!pass(&commands, &env).passed());
        pass(&commands, &env).keep(&root, &tree).unwrap();
        assert!(pass(&commands, &env).passed());
        // Another commit's tree, other commands, or clippy settings judge again.
        *tree.1.borrow_mut() = "bbb";
        assert!(!pass(&commands, &env).passed());
        *tree.1.borrow_mut() = "aaa";
        let linted = [
            commands.clone(),
            vec!["cargo clippy --locked --all-targets -- -D warnings".to_owned()],
        ]
        .concat();
        assert!(!pass(&linted, &env).passed());
        let mut configured = env.clone();
        configured.insert("CLIPPY_CONF_DIR".into(), "/elsewhere".into());
        assert!(!pass(&commands, &configured).passed());
        // A commit while it ran leaves no pass behind.
        let running = pass(&linted, &env);
        *tree.1.borrow_mut() = "ccc";
        running.keep(&root, &tree).unwrap();
        *tree.1.borrow_mut() = "aaa";
        assert!(!pass(&linted, &env).passed());
        // Nor is one kept where clippy's inputs can't be fingerprinted.
        let mut own = env.clone();
        own.insert("CARGO_TARGET_DIR".into(), "/tmp/elsewhere".into());
        assert!(Pass::of(&root, &commands, &own, &tree).unwrap().is_none());
    }
    #[test]
    fn a_check_needs_a_committed_branch_of_its_own() {
        assert!(blockers("feature", false).is_empty());
        for branch in ["main", "HEAD"] {
            assert!(!blockers(branch, false).is_empty());
        }
        assert!(!blockers("feature", true).is_empty());
    }
    #[test]
    fn a_failure_shows_the_lines_that_say_what_failed() {
        let cargo = "running 3 tests\ntest a ... ok\ntest b ... FAILED\n\n---- b stdout ----\n\
            thread 'b' panicked at src/lib.rs:4:5:\nassertion `left == right` failed\n  left: 1\n right: 2\n\
            test result: FAILED. 1 passed; 1 failed\nerror: test failed, to rerun pass `--lib`\n";
        assert_eq!(
            failure(cargo),
            [
                "test b ... FAILED",
                "thread 'b' panicked at src/lib.rs:4:5:",
                "left: 1",
                "right: 2",
                "test result: FAILED. 1 passed; 1 failed",
                "error: test failed, to rerun pass `--lib`",
            ]
        );
        let coloured = "\u{1b}[31m\u{2716} the audit log refuses nothing\u{1b}[39m\n  AssertionError [ERR_ASSERTION]: Got unwanted exception\n";
        assert_eq!(
            failure(coloured),
            [
                "\u{2716} the audit log refuses nothing",
                "AssertionError [ERR_ASSERTION]: Got unwanted exception"
            ]
        );
        let scan = "Scanning 812 files\ntooling/cli/src/pr.rs:259: fixed-network-address\n\
            Privacy check failed; matching values are withheld.\nsee: the docs\n";
        assert_eq!(
            failure(scan),
            [
                "tooling/cli/src/pr.rs:259: fixed-network-address",
                "Privacy check failed; matching values are withheld."
            ]
        );
        // However many warnings come first, what failed is shown.
        let warned =
            "/usr/lib/python3.14/unittest/mock.py:2259: ResourceWarning: unclosed database\n"
                .repeat(40)
                + "[warn] features/dvic/frontend/feature.ts\nFailed rules: format\n";
        assert_eq!(
            failure(&warned),
            [
                "[warn] features/dvic/frontend/feature.ts",
                "Failed rules: format"
            ]
        );
        // A CI suite names the checks that failed once its tests have run.
        let suite = "[start] dependency audit\n1 high severity vulnerability\n[fail] dependency audit (1.1s)\n"
            .to_owned()
            + &"ok 1 - a test that passed\n".repeat(30)
            + "Failed checks: dependency audit\n";
        assert_eq!(failure(&suite), ["Failed checks: dependency audit"]);
        let quiet: String = (1..=40).map(|n| format!("line {n}\n")).collect();
        let tail = failure(&quiet);
        assert_eq!(
            (tail.len(), tail[0].as_str(), tail[24].as_str()),
            (25, "line 16", "line 40")
        );
    }
    #[test]
    fn an_old_python3_stops_the_local_check_with_what_to_do() {
        struct Old;
        impl Runner for Old {
            fn command(&self, args: &[&str], cwd: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                assert_eq!(
                    (args, cwd),
                    (
                        &["python3", "tooling/ci/python.py"][..],
                        Some(Path::new("/checkout"))
                    )
                );
                Err("python3 tooling/ci/python.py failed: python3 is Python 3.9.6".into())
            }
        }
        assert_eq!(
            here(Path::new("/checkout"), "check", &Old)
                .unwrap_err()
                .to_string(),
            "dispatchdev check can't start: python3 tooling/ci/python.py failed: python3 is \
             Python 3.9.6"
        );
    }
}
