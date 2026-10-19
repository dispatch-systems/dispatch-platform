//! `dispatchdev verify <feature> [--removability]`: one look at a feature in progress, committed
//! or not: whether `generate` would change anything, what a generator left to write in it, the
//! architecture rules, TypeScript, the format and lint of the changed files, and the feature's
//! own tests of every kind, built in a folder of the run's own (`crate::test`). `--removability`
//! then leaves it out as the weekly check does: on GitHub when the branch is pushed as it is
//! (`crate::ci`), else in a copy of the checkout, so neither the worktree nor its preview is
//! touched. A line a step, then what failed and what to do next.
use crate::{Result, Runner, ci, require, test, workspace};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const TSX: &str = "node node_modules/tsx/dist/cli.mjs";

/// The steps that need no build of their own, run in the checkout as they are: each its line's
/// label and its command.
fn quick(root: &Path, changed: &[String]) -> Result<Vec<(String, String)>> {
    let mut steps = vec![(
        "generate --check".to_owned(),
        format!("{TSX} tooling/scaffold/cli.ts generate --check"),
    )];
    // Every architecture rule, found where it lives. What a generator left to write is listed
    // rather than failed: a feature in progress has it on purpose.
    let mut rules: Vec<String> = fs::read_dir(root.join("app/tests/rules"))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".test.ts"))
        .map(|name| format!("app/tests/rules/{name}"))
        .collect();
    rules.sort();
    steps.push((
        format!("rules ({} files)", rules.len()),
        format!(
            "{TSX} --test --test-skip-pattern='generator left' {}",
            rules.join(" ")
        ),
    ));
    steps.push((
        "tsc --noEmit".into(),
        "node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json".into(),
    ));
    let files = |kept: &dyn Fn(&str) -> bool| -> Vec<String> {
        changed
            .iter()
            .filter(|file| kept(file))
            .map(|file| workspace::quote(file))
            .collect()
    };
    let formatted = files(&|_| true);
    if !formatted.is_empty() {
        steps.push((
            format!("prettier --check ({} changed files)", formatted.len()),
            format!(
                "node node_modules/prettier/bin/prettier.cjs --check --ignore-unknown {}",
                formatted.join(" ")
            ),
        ));
    }
    let linted = files(&|file| {
        [".ts", ".tsx", ".js", ".mjs", ".cjs"]
            .iter()
            .any(|kind| file.ends_with(kind))
    });
    if !linted.is_empty() {
        steps.push((
            format!("eslint ({} changed files)", linted.len()),
            format!(
                "node node_modules/eslint/bin/eslint.js --no-warn-ignored {}",
                linted.join(" ")
            ),
        ));
    }
    Ok(steps)
}
/// A step's line, as `dispatchdev test` prints its own.
fn line(ok: bool, took: Duration, label: &str) {
    let took = workspace::duration(took);
    println!("{}  {took:>7}  {label}", if ok { "ok  " } else { "FAIL" });
}
/// What a generator left to write in `feature`, a line each: `file:line: what it asks for`.
fn markers(root: &Path, feature: &str, runner: &dyn Runner) -> Result<Vec<String>> {
    let owner = format!("feature:{feature}");
    let listed: Value = serde_json::from_str(&workspace::text(
        runner,
        &[
            "node",
            "node_modules/tsx/dist/cli.mjs",
            "tooling/scaffold/cli.ts",
            "todo",
            &owner,
            "--json",
        ],
        Some(root),
    )?)?;
    Ok(listed
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|owner| owner["todos"].as_array().cloned().unwrap_or_default())
        .map(|todo| {
            format!(
                "{}:{}: {}",
                todo["file"].as_str().unwrap_or(""),
                todo["line"],
                todo["text"].as_str().unwrap_or("")
            )
        })
        .collect())
}
/// The branch the checkout is on, when origin has it as it is here: committed and pushed.
fn pushed(root: &Path, runner: &dyn Runner) -> Option<String> {
    let git =
        |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(root)).ok();
    if !git(&["status", "--porcelain"])?.is_empty() {
        return None;
    }
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    if matches!(branch.as_str(), "HEAD" | "main") {
        return None;
    }
    let head = git(&["rev-parse", "HEAD"])?;
    let theirs = git(&["ls-remote", "origin", &format!("refs/heads/{branch}")])?;
    (theirs.split_whitespace().next() == Some(head.as_str())).then_some(branch)
}
/// Copies the checkout's files, committed or not, into `to`, each with its times, and links its
/// packages: what the removability check moves and rewrites is the copy's.
fn copy(root: &Path, to: &Path, runner: &dyn Runner) -> Result<()> {
    let listed = runner.command(
        &["git", "ls-files", "-z", "-co", "--exclude-standard"],
        Some(root),
        120,
    )?;
    for file in listed.split(|b| *b == 0).filter(|file| !file.is_empty()) {
        let file = std::str::from_utf8(file)?;
        let from = root.join(file);
        // A file deleted from the worktree is still listed until the deletion is committed.
        let Ok(times) = fs::metadata(&from) else {
            continue;
        };
        let into = to.join(file);
        fs::create_dir_all(into.parent().ok_or("A file without a folder")?)?;
        fs::copy(&from, &into)?;
        fs::File::options().write(true).open(&into)?.set_times(
            fs::FileTimes::new()
                .set_accessed(times.accessed()?)
                .set_modified(times.modified()?),
        )?;
    }
    std::os::unix::fs::symlink(root.join("node_modules"), to.join("node_modules"))?;
    Ok(())
}
/// Leaves `feature` out in a copy of the checkout in the scratch folder, built in a folder of the
/// run's own in the checkout, on its disk, its output in `log`: whether it passed, and how long
/// it took. The copies a killed run left go first.
fn removed_in_copy(
    root: &Path,
    feature: &str,
    scratch: &Path,
    log: &Path,
    runner: &dyn Runner,
) -> Result<(bool, Duration)> {
    let copies = scratch.join("removability");
    test::sweep(&copies);
    let copy_root = copies.join(std::process::id().to_string());
    let _ = fs::remove_dir_all(&copy_root);
    let checkout = copy_root.join("checkout");
    copy(root, &checkout, runner)?;
    let run = test::Run::start(root, scratch)?;
    let step = test::Step {
        command: format!("npm run check:removability -- --feature {feature}"),
    };
    let done = run.step_in(&checkout, &step, log, &[]);
    run.end()?;
    fs::remove_dir_all(&copy_root)?;
    // The folder of copies goes once no run has one there.
    let _ = fs::remove_dir(&copies);
    done
}

/// `dispatchdev verify`: whether every step passed.
pub fn run(
    root: &Path,
    feature: &str,
    removability: bool,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
    say: &mut dyn FnMut(&str),
) -> Result<bool> {
    require(
        workspace::has_feature(root, feature),
        &format!("This checkout has no feature named {feature}: features/{feature} isn't there."),
    )?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("A checkout without a folder name")?;
    let scratch = workspace::scratch_dir(name)?;
    let logs: PathBuf = scratch.join("verify").join(std::process::id().to_string());
    fs::create_dir_all(&logs)?;
    let env = [("TMPDIR", scratch.as_os_str())];
    let (mut passed, mut failed) = (0, Vec::<String>::new());

    let changed = test::changed(root, runner)?;
    for (index, (label, command)) in quick(root, &changed)?.iter().enumerate() {
        let log = logs.join(format!("{}.log", index + 1));
        let (ok, took) = workspace::logged(command, root, &log, &env)?;
        line(ok, took, label);
        if ok {
            passed += 1;
        } else {
            test::explain(&log);
            failed.push(label.clone());
        }
    }

    let left = markers(root, feature, runner)?;
    if left.is_empty() {
        println!("Nothing a generator left to write is left in {feature}.");
    } else {
        println!(
            "What a generator left to write in {feature}, {} scaffold markers:",
            left.len()
        );
        for marker in &left {
            println!("      {marker}");
        }
    }

    if failed.is_empty() {
        let (commands, notes) =
            test::owned(root, &format!("feature:{feature}"), None, None, runner)?;
        for note in &notes {
            println!("{note}");
        }
        if !commands.is_empty() {
            if test::run_commands(root, &commands, false)? {
                passed += 1;
            } else {
                failed.push("its own tests".into());
            }
        }
    } else {
        println!("Its own tests wait until the steps above pass.");
    }

    if removability && failed.is_empty() {
        let label = format!("removability without {feature}");
        match pushed(root, runner) {
            Some(branch) => {
                let started = std::time::Instant::now();
                let verdict = ci::removability(root, &branch, feature, runner, pause, say);
                line(
                    verdict.is_ok(),
                    started.elapsed(),
                    &format!("{label}, on GitHub"),
                );
                match verdict {
                    Ok(verdict) => {
                        println!("      {verdict}");
                        passed += 1;
                    }
                    Err(error) => {
                        for why in error.to_string().lines() {
                            println!("      {why}");
                        }
                        failed.push(label);
                    }
                }
            }
            None => {
                let log = logs.join("removability.log");
                println!(
                    "Leaving out {feature} in a copy of the checkout, with a build of its own; it \
                     writes to {} as it goes.",
                    log.display()
                );
                let (ok, took) = removed_in_copy(root, feature, &scratch, &log, runner)?;
                line(ok, took, &format!("{label}, in a copy of the checkout"));
                if ok {
                    passed += 1;
                } else {
                    test::explain(&log);
                    failed.push(label);
                }
            }
        }
    } else if removability {
        println!("Its removability waits until the steps above pass.");
    }

    let mut next = vec![];
    if failed.is_empty() {
        fs::remove_dir_all(&logs)?;
        println!("All {passed} passed for {feature}.");
    } else {
        println!(
            "{} of {} failed for {feature}: {}.",
            failed.len(),
            passed + failed.len(),
            failed.join(", ")
        );
        next.push(format!(
            "Fix what failed, then `dispatchdev verify {feature}` again."
        ));
    }
    if !left.is_empty() {
        next.push(format!(
            "Write what each scaffold marker asks for: `dispatchdev todo feature:{feature}` lists them."
        ));
    }
    if next.is_empty() {
        next.push(format!(
            "Commit it, then `dispatchdev check` and `dispatchdev ci {name}`."
        ));
        if !removability {
            next.push(format!(
                "`dispatchdev verify {feature} --removability` also leaves it out, as the weekly check does."
            ));
        }
    }
    println!("{}", workspace::next(&next));
    Ok(failed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removability_runs_on_github_only_for_a_branch_pushed_as_it_is() {
        struct Git {
            status: &'static str,
            theirs: &'static str,
        }
        impl Runner for Git {
            fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
                Ok(match args[1] {
                    "status" => self.status,
                    "rev-parse" if args[2] == "--abbrev-ref" => "lean\n",
                    "rev-parse" => "2222222222222222\n",
                    "ls-remote" => self.theirs,
                    other => panic!("unexpected git {other}"),
                }
                .as_bytes()
                .to_vec())
            }
        }
        let pushed_as = |status, theirs| pushed(Path::new("/checkout"), &Git { status, theirs });
        let there = "2222222222222222\trefs/heads/lean\n";
        assert_eq!(pushed_as("", there).as_deref(), Some("lean"));
        // Uncommitted work, a head GitHub hasn't got, and a branch it hasn't got are tried here.
        assert_eq!(pushed_as(" M features/uniforms/feature.rs\n", there), None);
        assert_eq!(pushed_as("", "1111111111111111\trefs/heads/lean\n"), None);
        assert_eq!(pushed_as("", ""), None);
    }
    #[test]
    fn a_copy_holds_the_checkouts_files_committed_or_not_and_links_its_packages() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        let write = |file: &str| {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        };
        let git = |args: &[&str]| {
            crate::Native
                .command(&[&["git"][..], args].concat(), Some(&root), 30)
                .unwrap()
        };
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".gitignore"), "target/\nnode_modules/\n").unwrap();
        write("features/uniforms/feature.rs");
        git(&["init", "-q"]);
        git(&["add", "."]);
        write("features/parking/feature.rs");
        write("target/debug/built");
        write("node_modules/tsx/cli.mjs");
        let copied = temp.path().join("copy");
        copy(&root, &copied, &crate::Native).unwrap();
        for file in [
            "features/uniforms/feature.rs",
            "features/parking/feature.rs",
        ] {
            assert_eq!(fs::read_to_string(copied.join(file)).unwrap(), file);
        }
        assert!(!copied.join("target").exists());
        assert_eq!(
            fs::read_link(copied.join("node_modules")).unwrap(),
            root.join("node_modules")
        );
    }
}
