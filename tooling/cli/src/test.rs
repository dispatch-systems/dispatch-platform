//! `dispatchdev test [--all | --owner <owner>] [--keep]`: the tests a change needs, or one
//! owner's, built in a folder of the run's own and deleted when it ends, so a worktree keeps no
//! test build and several can test at once. Each command runs below Dev (`workspace::logged`).
use crate::{Result, Runner, check, require, workspace};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Where a run builds: a folder per run inside the checkout, so the compiler's path policy
/// covers it, ignored by Git.
const BUILDS: &str = ".test-build";

/// Packages the runtime for the browser tests with the debug backend, which the run's own Rust
/// builds share or the build cache holds, where a release build takes minutes. The merge queue
/// tests the release build.
pub const PACKAGE: &str = "npm run build -- --debug";
/// What a checkout from before the debug package runs instead: the release build, through the
/// build cache in the checkout.
const RELEASE_PACKAGE: &str = "npm run build";
/// Builds or reuses the browser tests' assessment fixture. It runs as the running dispatchdev, so
/// a checkout whose own copy predates `--fixture` still gets it.
const FIXTURE: &str = "dispatchdev build --fixture";

/// One command of a run, in the run's build folder.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub command: String,
}
/// The steps of `commands`: packaging the browser tests' runtime, then their fixture, before the
/// tests themselves.
pub fn steps(commands: &[String]) -> Vec<Step> {
    let step = |command: &str| Step {
        command: command.into(),
    };
    commands
        .iter()
        .flat_map(|command| {
            if command == PACKAGE {
                return vec![step(PACKAGE), step(FIXTURE)];
            }
            match command.strip_prefix(&format!("{PACKAGE} && ")) {
                Some(rest) => vec![step(PACKAGE), step(FIXTURE), step(rest)],
                None => vec![step(command)],
            }
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
/// The tests the change touches: those it changes and those watching a source it changes.
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
/// The owner `given` names, as its kind and name: `<kind>:<name>`, `mcp`, or a name found by its
/// folder, a feature's, a collector's or a part of core's.
pub fn owner(root: &Path, given: &str) -> Result<(String, String)> {
    if given == "mcp" {
        return Ok(("mcp".into(), String::new()));
    }
    if let Some((kind, name)) = given.split_once(':') {
        require(
            ["feature", "collector", "core"].contains(&kind) && !name.is_empty(),
            "An owner is kind:name with kind feature, collector or core, or mcp.",
        )?;
        return Ok((kind.into(), name.into()));
    }
    require(
        !given.is_empty()
            && given
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "An owner is a feature's, a collector's or a part of core's name, kind:name or mcp.",
    )?;
    let kinds: Vec<&str> = [
        ("feature", "features"),
        ("collector", "collectors"),
        ("core", "core"),
    ]
    .into_iter()
    .filter(|(_, home)| root.join(home).join(given).is_dir())
    .map(|(kind, _)| kind)
    .collect();
    match kinds.as_slice() {
        [kind] => Ok(((*kind).into(), given.into())),
        [] => Err(format!("No feature, collector or part of core is named {given}.").into()),
        [first, ..] => Err(format!(
            "{given} is a {}: name one, as {first}:{given}.",
            kinds.join(" and a ")
        )
        .into()),
    }
}
/// What runs `given`'s tests, as `tooling/testing/owner-tests.ts` lists them, with the debug
/// package for its browser tests: the commands, and what it notes about them. `only` keeps one
/// kind of test, `file` one test file, by its path from the root.
pub fn owned(
    root: &Path,
    given: &str,
    only: Option<&str>,
    file: Option<&str>,
    runner: &dyn Runner,
) -> Result<(Vec<String>, Vec<String>)> {
    let (kind, name) = owner(root, given)?;
    let mut args = vec![
        "node",
        "node_modules/tsx/dist/cli.mjs",
        "tooling/testing/owner-tests.ts",
        &kind,
    ];
    if !name.is_empty() {
        args.push(&name);
    }
    args.extend(["--list", "--debug"]);
    if let Some(only) = only {
        args.extend(["--only", only]);
    }
    if let Some(file) = file {
        args.extend(["--file", file]);
    }
    // It says why it refused on its own; the command it failed in says nothing more.
    let listed = workspace::text(runner, &args, Some(root)).map_err(|error| {
        let text = error.to_string();
        text.split_once(" failed: ")
            .map_or(text.clone(), |(_, why)| why.to_owned())
    })?;
    let lines = |sign: &str| -> Vec<String> {
        listed
            .lines()
            .filter_map(|line| line.strip_prefix(sign))
            .map(str::to_owned)
            .collect()
    };
    Ok((lines("$ "), lines("# ")))
}
/// A file named from the current directory, by its path from `root`, as the owner's tests name
/// it; as given when it is outside the checkout.
pub fn from_root(root: &Path, file: &str) -> String {
    // By its real path, as far as it exists: worktrees/ may link to a disk of its own.
    let real = |path: PathBuf| {
        let mut rest = vec![];
        let mut at = path.as_path();
        loop {
            if let Ok(real) = fs::canonicalize(at) {
                return Some(
                    rest.into_iter()
                        .rev()
                        .fold(real, |path, part| path.join(part)),
                );
            }
            rest.push(at.file_name()?.to_owned());
            at = at.parent()?;
        }
    };
    let path = std::env::current_dir()
        .ok()
        .and_then(|here| real(here.join(file)));
    path.zip(fs::canonicalize(root).ok())
        .and_then(|(path, root)| {
            path.strip_prefix(root)
                .ok()
                .and_then(|path| path.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| file.to_owned())
}
/// A command as a line reads it: a long list of test files as their count.
pub fn label(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    let is_test = |word: &str| {
        let word = word.trim_matches('\'');
        word.ends_with(".test.ts") || word.ends_with(".spec.ts")
    };
    let tests = words.iter().filter(|word| is_test(word)).count();
    if tests <= 3 {
        return command.to_owned();
    }
    let kept: Vec<String> = words
        .iter()
        .filter(|word| !is_test(word))
        .map(|word| {
            let word = word.trim_matches('\'');
            Path::new(word)
                .file_name()
                .filter(|_| word.starts_with('/'))
                .and_then(|name| name.to_str())
                .unwrap_or(word)
                .to_owned()
        })
        .collect();
    format!("{} ({tests} test files)", kept.join(" "))
}
/// Removes the folders of runs that ended without cleaning up, killed or interrupted, each named
/// for its run's process.
pub(crate) fn sweep(builds: &Path) {
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
/// A run's build folder in a checkout, and what the browser tests' packaging makes there, all
/// deleted when the run ends.
pub struct Run {
    root: PathBuf,
    builds: PathBuf,
    pub build: PathBuf,
    made: Vec<PathBuf>,
    binary: PathBuf,
    tmp: PathBuf,
    /// The checkout's package build can take the debug backend.
    debug_package: bool,
    /// The assessment fixture, once this run has built or reused it for the checkout's inputs.
    fixture: std::cell::Cell<bool>,
}
impl Run {
    /// A run in `root`, its temporary files in `tmp`.
    pub fn start(root: &Path, tmp: &Path) -> Result<Self> {
        let builds = root.join(BUILDS);
        sweep(&builds);
        let build = builds.join(std::process::id().to_string());
        fs::create_dir_all(&build)?;
        Ok(Self {
            root: root.to_owned(),
            // What the browser tests' packaging makes in the checkout, if it isn't there already.
            made: ["target/release", ".build"]
                .iter()
                .map(|path| root.join(path))
                .filter(|path| !path.exists())
                .collect(),
            binary: build.join("debug/dispatch-backend"),
            builds,
            build,
            tmp: tmp.to_owned(),
            debug_package: fs::read_to_string(root.join("tooling/build/build.ts"))
                .is_ok_and(|build| build.contains("'--debug'")),
            fixture: std::cell::Cell::new(false),
        })
    }
    /// Runs `step` with `env` added, its output going to `log`: whether it passed, and how long
    /// it took.
    pub fn step(
        &self,
        step: &Step,
        log: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(bool, std::time::Duration)> {
        self.step_in(&self.root, step, log, env)
    }
    /// Runs `step` as `step` does, in the folder `cwd`, such as a copy of the checkout.
    pub fn step_in(
        &self,
        cwd: &Path,
        step: &Step,
        log: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(bool, std::time::Duration)> {
        let fixture = &crate::build::built(&self.build, &crate::build::FIXTURE)[0];
        // A checkout from before the debug package packages the release build in the checkout
        // itself, where the build cache serves it.
        let (command, isolated) = match step.command.as_str() {
            PACKAGE if !self.debug_package => (RELEASE_PACKAGE, false),
            command => (command, true),
        };
        let mut own: Vec<(&str, &OsStr)> = vec![("TMPDIR", self.tmp.as_os_str())];
        if isolated {
            own.extend([
                ("CARGO_TARGET_DIR", self.build.as_os_str()),
                ("CARGO_INCREMENTAL", OsStr::new("0")),
                ("DISPATCH_TEST_BINARY", self.binary.as_os_str()),
            ]);
            if self.fixture.get() {
                own.push(("DISPATCH_ASSESSMENT_FIXTURE", fixture.as_os_str()));
            }
        }
        let command = match command.strip_prefix("dispatchdev ") {
            Some(rest) => {
                let exe = std::env::current_exe()?;
                let exe = exe.to_str().ok_or("Non-UTF8 path")?;
                format!("'{}' {rest}", exe.replace('\'', r"'\''"))
            }
            None => command.to_owned(),
        };
        let (ok, took) = workspace::logged(&command, cwd, log, &[&own[..], env].concat())?;
        if ok && step.command == FIXTURE {
            self.fixture.set(true);
        }
        Ok((ok, took))
    }
    /// Deletes everything the run made, and answers how much that was.
    pub fn end(self) -> Result<u64> {
        let mut freed: u64 = 0;
        for path in
            std::iter::once(self.build).chain(self.made.into_iter().filter(|path| path.exists()))
        {
            freed += workspace::size(&crate::Native, &path).unwrap_or(0);
            fs::remove_dir_all(&path)?;
        }
        let _ = fs::remove_dir(&self.builds);
        Ok(freed)
    }
}
/// Prints a failed command's lines that say why, and where its whole output is.
pub fn explain(log: &Path) {
    for line in check::failure(&fs::read_to_string(log).unwrap_or_default()) {
        println!("      {line}");
    }
    println!("      Whole output: {}", log.display());
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
    // Each run's logs apart, so runs side by side never write over each other's.
    let logs = scratch.join("test").join(std::process::id().to_string());
    fs::create_dir_all(&logs)?;
    let run = Run::start(root, &scratch)?;
    let mut passed = true;
    for (index, step) in steps(commands).iter().enumerate() {
        let log = logs.join(format!("{}.log", index + 1));
        let (ok, took) = run.step(step, &log, &[])?;
        let took = workspace::duration(took);
        if ok {
            println!("ok    {took:>7}  {}", label(&step.command));
            continue;
        }
        println!("FAIL  {took:>7}  {}", label(&step.command));
        explain(&log);
        passed = false;
        break;
    }
    if keep {
        println!("Kept the build in {}.", run.build.display());
        return Ok(passed);
    }
    let freed = run.end()?;
    if passed {
        fs::remove_dir_all(&logs)?;
    }
    println!("Deleted the run's builds: {}.", workspace::human(freed));
    Ok(passed)
}
/// Which tests a run runs.
pub enum Tests<'a> {
    /// Those the diff touches.
    Affected,
    All,
    /// One owner's, of every kind or `only` one, or one `file` of them.
    Owner {
        owner: &'a str,
        only: Option<&'a str>,
        file: Option<&'a str>,
    },
}
/// `dispatchdev test`.
pub fn run(root: &Path, tests: Tests<'_>, keep: bool, runner: &dyn Runner) -> Result<bool> {
    let commands = match tests {
        Tests::All => everything(root, runner)?,
        Tests::Affected => affected(root, runner)?,
        Tests::Owner { owner, only, file } => {
            let file = file.map(|file| from_root(root, file));
            let (commands, notes) = owned(root, owner, only, file.as_deref(), runner)?;
            for note in &notes {
                println!("{note}");
            }
            if commands.is_empty() {
                if notes.is_empty() {
                    println!("{owner} has no such tests.");
                }
                return Ok(true);
            }
            commands
        }
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
            "npm run build -- --debug && npm run test:ui -- app/tests/browser/a.spec.ts",
        ]
        .map(str::to_owned)
        .to_vec();
        let packaged = [
            Step {
                command: "cargo test --locked -p dispatch-core".into(),
            },
            Step {
                command: "npm run build -- --debug".into(),
            },
            Step {
                command: "dispatchdev build --fixture".into(),
            },
            Step {
                command: "npm run test:ui -- app/tests/browser/a.spec.ts".into(),
            },
        ];
        assert_eq!(steps(&commands), packaged);
        // An owner's tests list the packaging on a line of its own.
        let owned: Vec<String> = [
            "cargo test --locked -p dispatch-core",
            "npm run build -- --debug",
            "npm run test:ui -- app/tests/browser/a.spec.ts",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(steps(&owned), packaged);
    }
    #[test]
    fn an_owner_is_named_by_its_kind_and_name_or_found_by_its_name() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("features/parking")).unwrap();
        let owner = |given: &str| owner(temp.path(), given).map_err(|error| error.to_string());
        assert_eq!(owner("parking"), Ok(("feature".into(), "parking".into())));
        assert_eq!(
            owner("collector:paycom"),
            Ok(("collector".into(), "paycom".into()))
        );
        assert_eq!(owner("mcp"), Ok(("mcp".into(), String::new())));
        // A kind there is no owner of is refused in one line, as an empty name is.
        for given in ["page:parking", "feature:", "x:y:z"] {
            assert!(owner(given).unwrap_err().contains("kind:name"), "{given}");
        }
    }
    #[test]
    fn a_long_list_of_test_files_reads_as_its_count() {
        assert_eq!(
            label("cargo test --locked -p dispatch-core"),
            "cargo test --locked -p dispatch-core"
        );
        assert_eq!(
            label("npx tsx --test a.test.ts b.test.ts"),
            "npx tsx --test a.test.ts b.test.ts"
        );
        assert_eq!(
            label(
                "tooling/cli/dispatchdev build && '/opt/node/bin/node' 'tsx.mjs' '--test' 'a.test.ts' 'b.test.ts' 'c.test.ts' 'd.spec.ts'"
            ),
            "tooling/cli/dispatchdev build && node tsx.mjs --test (4 test files)"
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
