use crate::{
    Result, Runner,
    build::{self, Environment, frontend},
    machine::Machine,
    require,
    slot::{self, Slot},
    workspace,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
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
/// The workspace's crates: each member's directory and the package name its `Cargo.toml`
/// gives, as the root `Cargo.toml` lists the members, and which of them depend on which. A
/// member such as `features/*` is every directory under it with a `Cargo.toml`.
#[derive(Debug, Default)]
pub struct Workspace {
    crates: BTreeMap<String, String>,
    /// Each crate's name, with the names of the crates that name it as a dependency.
    dependents: BTreeMap<String, BTreeSet<String>>,
}
impl Workspace {
    pub fn read(root: &Path) -> Result<Self> {
        let manifest = |dir: &Path| -> Result<toml::Value> {
            Ok(toml::from_str(&fs::read_to_string(
                dir.join("Cargo.toml"),
            )?)?)
        };
        let workspace = manifest(root)?;
        let members = workspace
            .get("workspace")
            .and_then(|v| v.get("members"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str);
        let mut crates = BTreeMap::new();
        let mut manifests = BTreeMap::new();
        for member in members {
            let dirs = match member.strip_suffix("/*") {
                Some(parent) => fs::read_dir(root.join(parent))?
                    .map(|entry| Ok(format!("{parent}/{}", entry?.file_name().to_string_lossy())))
                    .collect::<Result<Vec<_>>>()?,
                None => vec![member.to_owned()],
            };
            for dir in dirs {
                if !root.join(&dir).join("Cargo.toml").is_file() {
                    continue;
                }
                let read = manifest(&root.join(&dir))?;
                let name = read
                    .get("package")
                    .and_then(|v| v.get("name"))
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| format!("{dir}/Cargo.toml names no package"))?
                    .to_owned();
                crates.insert(dir.clone(), name);
                manifests.insert(dir, read);
            }
        }
        // A dependency on another member names its directory, as a path from the member's own
        // or, inherited with `workspace = true`, from the root's.
        let shared = workspace
            .get("workspace")
            .and_then(|v| v.get("dependencies"))
            .and_then(toml::Value::as_table);
        let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (dir, read) in &manifests {
            for (key, spec) in dependencies(read) {
                let inherited = spec.get("workspace").and_then(toml::Value::as_bool) == Some(true);
                let path = match spec.get("path").and_then(toml::Value::as_str) {
                    Some(path) => joined(dir, path),
                    None if inherited => match shared
                        .and_then(|shared| shared.get(key))
                        .and_then(|v| v.get("path"))
                        .and_then(toml::Value::as_str)
                    {
                        Some(path) => joined("", path),
                        None => continue,
                    },
                    None => continue,
                };
                if let Some(dependency) = crates.get(&path)
                    && *dependency != crates[dir]
                {
                    dependents
                        .entry(dependency.clone())
                        .or_default()
                        .insert(crates[dir].clone());
                }
            }
        }
        Ok(Self { crates, dependents })
    }
    /// The crates that depend on `name`, directly or through others: those whose code or
    /// tests a change in it can break.
    fn dependents(&self, name: &str) -> BTreeSet<&String> {
        let mut found = BTreeSet::new();
        let mut next = vec![name];
        while let Some(name) = next.pop() {
            for dependent in self.dependents.get(name).into_iter().flatten() {
                if found.insert(dependent) {
                    next.push(dependent);
                }
            }
        }
        found
    }
    /// The crate in an owner's directory: core's, a collector's, a feature's or the app's.
    fn owned(&self, owner: &str) -> Option<&String> {
        self.crates
            .iter()
            .find(|(dir, _)| *dir == owner || dir.starts_with(&format!("{owner}/")))
            .map(|(_, name)| name)
    }
    /// The crate of the member whose directory holds `file`.
    fn holding(&self, file: &str) -> Option<&String> {
        self.crates
            .iter()
            .find(|(dir, _)| file.starts_with(&format!("{dir}/")))
            .map(|(_, name)| name)
    }
}
/// Every dependency a manifest names, normal, dev and build, for any target, with its spec.
fn dependencies(manifest: &toml::Value) -> Vec<(&String, &toml::Value)> {
    const KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values());
    [manifest]
        .into_iter()
        .chain(targets)
        .flat_map(|table| KINDS.iter().filter_map(|kind| table.get(kind)))
        .filter_map(toml::Value::as_table)
        .flatten()
        .collect()
}
/// `relative`, a path from the workspace directory `dir`, as a path from the root.
fn joined(dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}
/// The owner a file belongs to, by its directory: core, a collector, a feature, the MCP or
/// the app.
fn owner(file: &str) -> Option<String> {
    let mut parts = file.split('/');
    match (parts.next()?, parts.next()?, parts.next()) {
        (top @ ("core" | "mcp" | "app"), _, _) => Some(top.to_owned()),
        (top @ ("collectors" | "features"), name, Some(_)) => Some(format!("{top}/{name}")),
        _ => None,
    }
}
/// The local commands that check what the diff touches: the Rust crates it changes, and the
/// test files it changes or that watch a source it changes, from `tooling/ci/test-plan.json`.
/// `npm run check:rules` already runs the rule and dashboard tests, found by where they live and
/// in the plan's `rules`, so they are left out, and `npm run lint` the linter.
pub fn affected(changed: &[String], plan: &Value, workspace: &Workspace) -> Vec<String> {
    let list = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let mut tests: BTreeSet<String> = changed
        .iter()
        .filter(|file| {
            (file.starts_with("tests/") || file.contains("/tests/"))
                && (file.ends_with(".test.ts") || file.ends_with(".spec.ts"))
        })
        .cloned()
        .collect();
    for group in plan["watch"].as_array().into_iter().flatten() {
        if list(&group["sources"])
            .iter()
            .any(|source| changed.contains(source))
        {
            tests.extend(list(&group["tests"]));
        }
    }
    // Each owner's crate is the one in its directory, whatever its name, so a new feature or
    // collector needs no change here.
    let app = workspace.owned("app");
    let mut crates = BTreeSet::new();
    for file in changed {
        if matches!(
            file.as_str(),
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml"
        ) || file.starts_with(".cargo/")
        {
            crates.extend(workspace.crates.values());
        } else if let Some(owner) = owner(file) {
            if !frontend(Path::new(file)) {
                // The owner's crate, or the app's for an owner with none of its own yet; every
                // crate that depends on it, as one crate did before the owners were cut out of
                // it; and the app's, which builds on every owner.
                if let Some(name) = workspace.owned(&owner).or(app) {
                    crates.insert(name);
                    crates.extend(workspace.dependents(name));
                }
                crates.extend(app);
            }
        } else if let Some(name) = workspace.holding(file) {
            // A crate of the tooling's or the hosts', and every crate that depends on it.
            crates.insert(name);
            crates.extend(workspace.dependents(name));
        }
    }
    let mut commands = vec![];
    if !crates.is_empty() {
        commands.push("cargo clippy --locked --all-targets -- -D warnings".to_owned());
        let packages: Vec<_> = crates.iter().map(|name| format!("-p {name}")).collect();
        commands.push(format!("cargo test --locked {}", packages.join(" ")));
    }
    let listed = list(&plan["rules"]);
    let rule = |test: &str| {
        test.starts_with("app/tests/rules/")
            || test.contains("/tests/frontend/")
            || listed.iter().any(|file| file == test)
    };
    let shard = |test: &str| {
        plan["native"]
            .as_object()
            .into_iter()
            .flatten()
            .find(|(_, files)| list(files).iter().any(|file| file == test))
            .map(|(shard, _)| shard.clone())
    };
    let (mut node, mut shards, mut specs) = (vec![], BTreeSet::new(), vec![]);
    for test in tests.iter().filter(|test| !rule(test)) {
        if test.contains("tests/browser/") {
            specs.push(test.as_str());
        } else if let Some(shard) = shard(test) {
            shards.insert(shard);
        } else {
            node.push(test.as_str());
        }
    }
    if !node.is_empty() {
        commands.push(format!(
            "tooling/cli/dispatchdev build && npx tsx --test {}",
            node.join(" ")
        ));
    }
    for shard in shards {
        commands.push(format!("npm run test:browseros -- --shard {shard}"));
    }
    if !specs.is_empty() {
        commands.push(format!(
            "{} && npm run test:ui -- {}",
            crate::test::PACKAGE,
            specs.join(" ")
        ));
    }
    commands
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
    let commands = affected(&changed, &plan, &workspace)
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
     pass, gh pr merge --squash merges it.";
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
/// Refuses `dispatchdev <command>`, the local check or tests, at once on a machine that runs
/// none: `check`, `test` and `verify` ask it before anything else.
pub fn here(machine: Machine, command: &str) -> Result<()> {
    require(
        machine.checks(),
        &format!(
            "dispatchdev {command} runs on Linux alone: the checks on the branch's PR run every \
             check on GitHub."
        ),
    )
}
/// The rule checks: everything a push can fail on without a build, side by side, in about a
/// minute. What `dispatchdev check` adds, the TypeScript linter and clippy, the PR's checks run
/// too.
pub const RULES: &str = "npm run check:rules";
/// `dispatchdev check`: the rule checks and the lints the plan names, side by side, a line for
/// each as it ends; clippy builds in the checkout's own `target`, where its last run left it.
/// A committed tree they all passed on passes again without running, and so does clippy on
/// identical Rust inputs.
pub fn execute(root: &Path, plan: &Plan, env: &Environment, runner: &dyn Runner) -> Result<bool> {
    let planned: Vec<String> = [RULES.to_owned(), "npm run lint".to_owned()]
        .into_iter()
        .chain(plan.commands.iter().cloned())
        .collect();
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
    use serde_json::json;
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
    /// A file of `text` in the directory `root`, with the directories it needs.
    fn write(root: &Path, file: &str, text: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    /// A workspace on disk of `members`, each crate a directory with its package's name and
    /// the directories of the crates it depends on, as paths from its own.
    fn workspace(
        members: &str,
        crates: &[(&str, &str, &[&str])],
    ) -> (tempfile::TempDir, Workspace) {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers = [{members}]\n"),
        );
        let name = |dir: &str| crates.iter().find(|(d, ..)| *d == dir).unwrap().1;
        for (dir, package, depends) in crates {
            let up = "../".repeat(dir.split('/').count());
            let dependencies: String = depends
                .iter()
                .map(|on| format!("{} = {{ path = \"{up}{on}\" }}\n", name(on)))
                .collect();
            write(
                root.path(),
                &format!("{dir}/Cargo.toml"),
                &format!("[package]\nname = \"{package}\"\n\n[dependencies]\n{dependencies}"),
            );
        }
        let workspace = Workspace::read(root.path()).unwrap();
        (root, workspace)
    }
    #[test]
    fn affected_names_the_changed_crates_and_the_tests_the_diff_changes_or_watches() {
        let (_root, workspace) = workspace(
            r#""core", "collectors/*", "app/backend", "ops/host-manager", "tooling/shared""#,
            &[
                ("core", "dispatch-core", &[]),
                ("collectors/cortex", "dispatch-cortex", &["core"]),
                ("collectors/paycom", "dispatch-paycom", &["core"]),
                (
                    "app/backend",
                    "dispatch-backend",
                    &[
                        "core",
                        "collectors/cortex",
                        "collectors/paycom",
                        "ops/host-manager",
                    ],
                ),
                ("ops/host-manager", "dispatch-host", &["tooling/shared"]),
                ("tooling/shared", "dispatch-shared", &[]),
            ],
        );
        let plan = json!({
            "rules": ["tooling/tests/test-plan.test.ts"],
            "native": {"cortex": ["collectors/cortex/tests/native/cortex-worker.test.ts"]},
            "watch": [{"sources": ["core/tenancy/backend/roles.rs"], "tests": [
                "core/tenancy/tests/api/roles.test.ts",
                "app/tests/browser/dsp-features.spec.ts",
                "core/tenancy/tests/frontend/features.test.ts"]}]
        });
        let changed = |files: &[&str]| -> Vec<String> {
            let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            affected(&files, &plan, &workspace)
        };
        // Core's change runs every crate that depends on it.
        assert_eq!(
            changed(&[
                "core/tenancy/backend/roles.rs",
                "collectors/cortex/tests/native/cortex-worker.test.ts"
            ]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend -p dispatch-core -p dispatch-cortex -p dispatch-paycom",
                "tooling/cli/dispatchdev build && npx tsx --test core/tenancy/tests/api/roles.test.ts",
                "npm run test:browseros -- --shard cortex",
                "npm run build -- --debug && npm run test:ui -- app/tests/browser/dsp-features.spec.ts",
            ]
        );
        // A workspace input touches every crate; rule tests are check:rules' own, listed or
        // found where they live.
        assert_eq!(
            changed(&[
                "Cargo.lock",
                "tooling/tests/test-plan.test.ts",
                "app/tests/rules/names.test.ts"
            ]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend -p dispatch-core -p dispatch-cortex -p dispatch-host -p dispatch-paycom -p dispatch-shared",
            ]
        );
        // The hosts' crate runs its own tests, and the app's, which builds on it.
        assert_eq!(
            changed(&[
                "ops/host-manager/src/updater.rs",
                "features/team/tests/browser/roles.spec.ts"
            ])[1],
            "cargo test --locked -p dispatch-backend -p dispatch-host"
        );
        // A collector's change runs its crate's tests, and the app's.
        assert_eq!(
            changed(&["collectors/paycom/connection/mod.rs"])[1],
            "cargo test --locked -p dispatch-backend -p dispatch-paycom"
        );
        assert!(changed(&["collectors/cortex/frontend/CortexCard.tsx"]).is_empty());
        assert!(changed(&["docs/readme.md", "app/frontend/main.tsx"]).is_empty());
        // Frontend code in an owner's directory never asks for the Rust checks.
        assert!(
            changed(&[
                "features/dvic/frontend/DvicPage.tsx",
                "core/shell/frontend/ui/Modal.tsx",
                "features/dvic/frontend/dvic.css"
            ])
            .is_empty()
        );
        // Without a plan, the changed tests themselves are still named.
        let files = vec!["core/tenancy/tests/api/roles.test.ts".to_owned()];
        assert_eq!(
            affected(&files, &Value::Null, &workspace),
            [
                "tooling/cli/dispatchdev build && npx tsx --test core/tenancy/tests/api/roles.test.ts"
            ]
        );
    }
    #[test]
    fn the_mcp_is_an_owner_whose_frontend_asks_for_no_rust() {
        let (_root, workspace) = workspace(
            r#""core", "mcp", "app/backend""#,
            &[
                ("core", "dispatch-core", &[]),
                ("mcp", "dispatch-mcp", &["core"]),
                ("app/backend", "dispatch-backend", &["core", "mcp"]),
            ],
        );
        let changed = |files: &[&str]| -> Vec<String> {
            let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            affected(&files, &Value::Null, &workspace)
        };
        assert_eq!(
            changed(&["mcp/backend/keys.rs"])[1],
            "cargo test --locked -p dispatch-backend -p dispatch-mcp"
        );
        assert!(changed(&["mcp/frontend/KeysTab.tsx", "mcp/api/client.ts"]).is_empty());
    }
    #[test]
    fn each_owners_crate_is_the_one_its_directory_holds() {
        let (root, _) = workspace(
            r#""core", "features/*", "app/backend""#,
            &[
                ("core", "dispatch-core", &[]),
                ("features/driver_match", "dispatch-driver-match", &[]),
                ("app/backend", "dispatch-backend", &[]),
            ],
        );
        // A feature with a directory and no Cargo.toml yet.
        fs::create_dir_all(root.path().join("features/timecard/backend")).unwrap();
        let before = Workspace::read(root.path()).unwrap();
        let changed = |workspace: &Workspace, file: &str| {
            affected(&[file.to_owned()], &Value::Null, workspace)
        };
        let test = |packages: &str| {
            vec![
                "cargo clippy --locked --all-targets -- -D warnings".to_owned(),
                format!("cargo test --locked {packages}"),
            ]
        };
        // Its package name, not one made from its directory's, with the app, which builds on it.
        assert_eq!(
            changed(&before, "features/driver_match/backend/matching.rs"),
            test("-p dispatch-backend -p dispatch-driver-match")
        );
        assert_eq!(
            changed(&before, "features/timecard/backend/meals/sync.rs"),
            test("-p dispatch-backend")
        );
        assert!(changed(&before, "features/driver_match/frontend/DriverMatchTab.tsx").is_empty());
        // A new feature's crate is found as soon as it has a Cargo.toml.
        fs::create_dir_all(root.path().join("features/parking")).unwrap();
        fs::write(
            root.path().join("features/parking/Cargo.toml"),
            "[package]\nname = \"dispatch-parking\"\n",
        )
        .unwrap();
        let after = Workspace::read(root.path()).unwrap();
        assert_eq!(
            changed(&after, "features/parking/feature.rs"),
            test("-p dispatch-backend -p dispatch-parking")
        );
        assert_eq!(
            changed(&after, "Cargo.lock"),
            test(
                "-p dispatch-backend -p dispatch-core -p dispatch-driver-match -p dispatch-parking"
            )
        );
    }
    #[test]
    fn a_change_runs_every_crate_that_depends_on_it() {
        // The workspace's crates as they are: each feature on core and on the collectors
        // whose collections it keeps, Timecard on Driver Match too, and the app on every
        // owner and on the hosts' crate, which uses the tooling's.
        let (_root, workspace) = workspace(
            r#""core", "collectors/*", "features/*", "app/backend", "ops/host-manager", "tooling/shared""#,
            &[
                ("core", "dispatch-core", &[]),
                ("collectors/cortex", "dispatch-cortex", &["core"]),
                ("collectors/paycom", "dispatch-paycom", &["core"]),
                ("features/driver_match", "dispatch-driver-match", &["core"]),
                (
                    "features/dvic",
                    "dispatch-dvic",
                    &["core", "collectors/cortex"],
                ),
                (
                    "features/routes",
                    "dispatch-routes",
                    &["core", "collectors/cortex"],
                ),
                (
                    "features/weekly_scorecard",
                    "dispatch-weekly-scorecard",
                    &["core", "collectors/cortex"],
                ),
                (
                    "features/timecard",
                    "dispatch-timecard",
                    &[
                        "core",
                        "collectors/cortex",
                        "collectors/paycom",
                        "features/driver_match",
                    ],
                ),
                ("features/uniforms", "dispatch-uniforms", &["core"]),
                (
                    "app/backend",
                    "dispatch-backend",
                    &[
                        "core",
                        "collectors/cortex",
                        "collectors/paycom",
                        "features/driver_match",
                        "features/dvic",
                        "features/routes",
                        "features/weekly_scorecard",
                        "features/timecard",
                        "features/uniforms",
                        "ops/host-manager",
                    ],
                ),
                ("ops/host-manager", "dispatch-host", &["tooling/shared"]),
                ("tooling/shared", "dispatch-shared", &[]),
            ],
        );
        let tested = |file: &str| affected(&[file.to_owned()], &Value::Null, &workspace)[1].clone();
        assert_eq!(
            tested("collectors/cortex/collections/routes/mod.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-cortex -p dispatch-dvic -p dispatch-routes -p dispatch-timecard -p dispatch-weekly-scorecard"
        );
        assert_eq!(
            tested("features/driver_match/backend/matching.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-driver-match -p dispatch-timecard"
        );
        // Every owner's crate depends on core.
        assert_eq!(
            tested("core/db/backend/mod.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-core -p dispatch-cortex -p dispatch-driver-match -p dispatch-dvic -p dispatch-paycom -p dispatch-routes -p dispatch-timecard -p dispatch-uniforms -p dispatch-weekly-scorecard"
        );
        // A crate nothing else depends on runs its own tests, and the app's.
        assert_eq!(
            tested("features/uniforms/backend/stock.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-uniforms"
        );
        // Through the hosts' crate, the tooling's reaches the app.
        assert_eq!(
            tested("tooling/shared/src/process.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-host -p dispatch-shared"
        );
    }
    #[test]
    fn a_crate_depends_on_another_however_its_manifest_names_it() {
        let root = tempfile::tempdir().unwrap();
        let write = |file: &str, text: &str| write(root.path(), file, text);
        write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"core\", \"features/*\", \"app/backend\"]\n\n\
             [workspace.dependencies]\ndispatch-core = { path = \"core\" }\nserde = \"1\"\n",
        );
        // Its own tests' dependency on itself makes it no dependent of its own.
        write(
            "core/Cargo.toml",
            "[package]\nname = \"dispatch-core\"\n\n[dependencies]\nserde = { workspace = true }\n\n\
             [dev-dependencies]\ndispatch-core = { path = \".\", features = [\"testing\"] }\n",
        );
        // Through the workspace's list.
        write(
            "features/fuel/Cargo.toml",
            "[package]\nname = \"dispatch-fuel\"\n\n[dependencies]\ndispatch-core = { workspace = true }\n",
        );
        // For its tests alone.
        write(
            "features/parking/Cargo.toml",
            "[package]\nname = \"dispatch-parking\"\n\n\
             [dev-dependencies]\ndispatch-fuel = { path = \"../fuel\", features = [\"ts\"] }\n",
        );
        // On one platform.
        write(
            "app/backend/Cargo.toml",
            "[package]\nname = \"dispatch-backend\"\n\n\
             [target.'cfg(unix)'.dependencies]\ndispatch-parking = { path = \"../../features/parking\" }\n",
        );
        let workspace = Workspace::read(root.path()).unwrap();
        let dependents = |name: &str| -> Vec<String> {
            workspace.dependents(name).into_iter().cloned().collect()
        };
        assert_eq!(
            dependents("dispatch-core"),
            ["dispatch-backend", "dispatch-fuel", "dispatch-parking"]
        );
        assert_eq!(dependents("dispatch-parking"), ["dispatch-backend"]);
        assert!(dependents("dispatch-backend").is_empty());
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
    fn on_a_mac_the_local_check_refuses_and_names_the_prs_checks() {
        assert_eq!(
            here(Machine::Mac, "check").unwrap_err().to_string(),
            "dispatchdev check runs on Linux alone: the checks on the branch's PR run every check \
             on GitHub."
        );
        assert!(here(Machine::Linux, "check").is_ok());
    }
}
