//! `dispatchdev verify <feature>`: one look at a feature in progress, committed or not: whether
//! `generate` would change anything, what a generator left to write in it, the architecture rules,
//! TypeScript, the format and lint of the changed files, and the feature's own tests of every
//! kind, built in a folder of the run's own (`crate::test`). A line a step, then what failed and
//! what to do next. It runs on Linux alone, as the local checks do.
use crate::{
    Result, Runner,
    machine::Machine,
    require,
    slot::{self, Slot},
    test, workspace,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const TSX: &str = "node node_modules/tsx/dist/cli.mjs";
/// Whether `generate` would change anything, from the app's tests built in the checkout's own
/// target folder.
const GENERATE: &str =
    "node node_modules/tsx/dist/cli.mjs tooling/scaffold/cli.ts generate --check";

/// The steps that need no build of their own, run in the checkout as they are: each its line's
/// label and its command.
fn quick(root: &Path, changed: &[String]) -> Result<Vec<(String, String)>> {
    let mut steps = vec![("generate --check".to_owned(), GENERATE.to_owned())];
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
/// `dispatchdev verify`: whether every step passed.
pub fn run(root: &Path, feature: &str, machine: Machine, runner: &dyn Runner) -> Result<bool> {
    crate::check::here(machine, "verify")?;
    require(
        workspace::has_feature(root, feature),
        &format!("This checkout has no feature named {feature}: features/{feature} isn't there."),
    )?;
    let scratch = workspace::checkout_scratch(root)?;
    let logs: PathBuf = scratch.join("verify").join(std::process::id().to_string());
    fs::create_dir_all(&logs)?;
    let (mut passed, mut failed) = (0, Vec::<String>::new());

    let changed = test::changed(root, runner)?;
    for (index, (label, command)) in quick(root, &changed)?.iter().enumerate() {
        let log = logs.join(format!("{}.log", index + 1));
        // generate --check builds the app's tests, so it waits its turn for a build slot.
        let slot = if command == GENERATE {
            Slot::take(root, label, &std::env::vars().collect(), runner)?
        } else {
            None
        };
        let mut env = vec![("TMPDIR", scratch.as_os_str())];
        if let Some(slot) = &slot {
            env.push((slot::HELD, slot.path.as_os_str()));
        }
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
        next.push("Commit it, then `dispatchdev check`.".to_owned());
    }
    println!("{}", workspace::next(&next));
    Ok(failed.is_empty())
}
