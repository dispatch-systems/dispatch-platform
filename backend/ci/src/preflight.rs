use crate::{REPOSITORY, Result, Runner};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};
/// Problems that stop a push. With a merge queue on `main`, the queue validates the actual
/// merged state, so a moved `main` and other ready PRs no longer block.
pub fn blockers(
    branch: &str,
    dirty: bool,
    current: bool,
    pulls: &[Value],
    concurrent: bool,
    queued: bool,
) -> Vec<String> {
    let mut problems = vec![];
    if matches!(branch, "main" | "HEAD") {
        problems.push("Use an isolated feature branch.".into());
    }
    if dirty {
        problems.push("Commit the completed changes before starting final validation.".into());
    }
    if !current && !queued {
        problems.push("origin/main has advanced. Incorporate it once, review the combined change, then rerun this preflight.".into());
    }
    let others: Vec<_> = pulls
        .iter()
        .filter(|pr| pr["headRefName"] != branch && pr["isDraft"] != true)
        .map(|pr| format!("#{}", pr["number"]))
        .collect();
    if !concurrent && !queued && !others.is_empty() {
        problems.push(format!("Finish the ready PRs first, or leave this PR as a draft: {}. Use --allow-concurrent when overlap is intentional.",others.join(", ")));
    }
    problems
}
/// The local commands that check what the diff touches: the Rust crates it changes, and the
/// test files it changes or that watch a source it changes, from `tooling/ci/test-plan.json`.
/// `npm run check:rules` already runs the rule and dashboard tests, so they are left out.
pub fn affected(changed: &[String], plan: &Value) -> Vec<String> {
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
            file.starts_with("tests/") && (file.ends_with(".test.ts") || file.ends_with(".spec.ts"))
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
    let mut crates = BTreeSet::new();
    for file in changed {
        if file.starts_with("backend/host/") {
            crates.insert("dispatch-host");
        } else if file.starts_with("backend/ci/") {
            crates.insert("dispatch-ci");
        } else if ["backend/", "core/", "collectors/", "features/"]
            .iter()
            .any(|root| file.starts_with(root))
        {
            crates.insert("dispatch-backend");
        } else if matches!(
            file.as_str(),
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml"
        ) || file.starts_with(".cargo/")
        {
            crates.extend(["dispatch-backend", "dispatch-ci", "dispatch-host"]);
        }
    }
    let mut commands = vec![];
    if !crates.is_empty() {
        commands.push("cargo clippy --locked --all-targets -- -D warnings".to_owned());
        let packages: Vec<_> = crates.iter().map(|name| format!("-p {name}")).collect();
        commands.push(format!("cargo test --locked {}", packages.join(" ")));
    }
    let rules = [list(&plan["dashboard"]), list(&plan["rules"])].concat();
    let shard = |test: &str| {
        plan["native"]
            .as_object()
            .into_iter()
            .flatten()
            .find(|(_, files)| list(files).iter().any(|file| file == test))
            .map(|(shard, _)| shard.clone())
    };
    let (mut node, mut shards, mut specs) = (vec![], BTreeSet::new(), vec![]);
    for test in tests.iter().filter(|test| !rules.contains(test)) {
        if let Some(spec) = test.strip_prefix("tests/browser/") {
            specs.push(spec);
        } else if let Some(shard) = shard(test) {
            shards.insert(shard);
        } else {
            node.push(test.as_str());
        }
    }
    if !node.is_empty() {
        commands.push(format!(
            "python3 tooling/cargo-build.py && npx tsx --test {}",
            node.join(" ")
        ));
    }
    for shard in shards {
        commands.push(format!("npm run test:browseros -- --shard {shard}"));
    }
    if !specs.is_empty() {
        commands.push(format!(
            "npm run build && npm run test:ui -- {}",
            specs.join(" ")
        ));
    }
    commands
}
pub fn run(root: &Path, concurrent: bool, runner: &dyn Runner) -> Result<()> {
    let command = |args: &[&str]| -> Result<String> {
        Ok(String::from_utf8(runner.command(args, Some(root), 120)?)?
            .trim()
            .to_owned())
    };
    let branch = command(&["git", "rev-parse", "--abbrev-ref", "HEAD"])?;
    let dirty = !command(&["git", "status", "--porcelain"])?.is_empty();
    command(&["git", "fetch", "origin", "main"])?;
    // Compare object IDs instead of treating arbitrary Git failures as ancestry results.
    let base = command(&["git", "rev-parse", "origin/main"])?;
    let ancestor = command(&["git", "merge-base", "origin/main", "HEAD"])?;
    let pulls: Vec<Value> = serde_json::from_str(&command(&[
        "gh",
        "pr",
        "list",
        "--repo",
        REPOSITORY,
        "--base",
        "main",
        "--state",
        "open",
        "--json",
        "number,headRefName,isDraft,statusCheckRollup",
    ])?)?;
    let queued = merge_queue(&command);
    let problems = blockers(&branch, dirty, base == ancestor, &pulls, concurrent, queued);
    if !problems.is_empty() {
        return Err(format!(
            "PR preparation needs attention:\n- {}",
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
    let commands = affected(&changed, &plan);
    if commands.is_empty() {
        println!(
            "Nothing in the diff has tests beyond npm run check:rules. The merge queue runs the full suite on the squash commit; nothing runs on the PR itself."
        );
    } else {
        println!(
            "Run what the diff touches before pushing. The merge queue runs the full suite on the squash commit; nothing runs on the PR itself.\n- {}",
            commands.join("\n- ")
        );
    }
    println!(
        "Ready for final validation against {}.",
        command(&["git", "rev-parse", "--short", "origin/main"])?
    );
    if queued {
        println!(
            "main has a merge queue: merging enqueues the PR, and the queue validates the actual merged state."
        );
    }
    let running = pulls
        .iter()
        .find(|pr| pr["headRefName"] == branch)
        .is_some_and(|pr| {
            pr["statusCheckRollup"].as_array().is_some_and(|checks| {
                checks.iter().any(|check| {
                    matches!(
                        check["status"].as_str(),
                        Some("QUEUED" | "IN_PROGRESS" | "PENDING")
                    )
                })
            })
        });
    println!(
        "{}",
        if running {
            "This PR is already being checked in the queue. Avoid another push unless there is a necessary correction."
        } else {
            "Push the final head, open the PR and ship it: npm run pr:ship -- <n> queues it at once."
        }
    );
    Ok(())
}
/// Whether `main` requires a merge queue. Any failure or unexpected answer counts as no queue.
fn merge_queue(command: &dyn Fn(&[&str]) -> Result<String>) -> bool {
    let (owner, name) = REPOSITORY.split_once('/').unwrap_or((REPOSITORY, ""));
    let query = format!(
        "query={{ repository(owner: \"{owner}\", name: \"{name}\") {{ mergeQueue(branch: \"main\") {{ id }} }} }}"
    );
    command(&["gh", "api", "graphql", "-f", &query])
        .ok()
        .and_then(|reply| serde_json::from_str::<Value>(&reply).ok())
        .is_some_and(|reply| reply["data"]["repository"]["mergeQueue"]["id"].is_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn preflight_coordinates_ready_branches_without_blocking_drafts() {
        let mut pull = json!({"number":1,"headRefName":"another","isDraft":true});
        assert!(blockers("feature", false, true, &[pull.clone()], false, false).is_empty());
        pull["isDraft"] = false.into();
        assert!(!blockers("feature", false, true, &[pull.clone()], false, false).is_empty());
        assert!(blockers("feature", false, true, &[pull.clone()], true, false).is_empty());
        assert!(blockers("another", false, true, &[pull.clone()], false, false).is_empty());
        for branch in ["main", "HEAD"] {
            assert!(!blockers(branch, false, true, &[], false, false).is_empty());
        }
        assert!(!blockers("feature", true, true, &[], true, false).is_empty());
        assert!(!blockers("feature", false, false, &[], true, false).is_empty());
        // A merge queue validates the merged state: a moved main and other ready PRs are fine,
        // but the branch and a dirty tree still block.
        assert!(blockers("feature", false, false, &[pull], false, true).is_empty());
        assert!(!blockers("feature", true, true, &[], true, true).is_empty());
        assert!(!blockers("main", false, true, &[], true, true).is_empty());
    }
    #[test]
    fn affected_names_the_changed_crates_and_the_tests_the_diff_changes_or_watches() {
        let plan = json!({
            "dashboard": ["tests/dashboard/features.test.ts"],
            "rules": ["tests/tooling/test-plan.test.ts"],
            "native": {"cortex": ["tests/providers/cortex-worker.test.ts"]},
            "watch": [{"sources": ["backend/src/roles.rs"], "tests": [
                "tests/api/roles.test.ts", "tests/browser/dsp-features.spec.ts",
                "tests/dashboard/features.test.ts"]}]
        });
        let changed = |files: &[&str]| -> Vec<String> {
            let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            affected(&files, &plan)
        };
        assert_eq!(
            changed(&[
                "backend/src/roles.rs",
                "tests/providers/cortex-worker.test.ts"
            ]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend",
                "python3 tooling/cargo-build.py && npx tsx --test tests/api/roles.test.ts",
                "npm run test:browseros -- --shard cortex",
                "npm run build && npm run test:ui -- dsp-features.spec.ts",
            ]
        );
        // A workspace input touches every crate; rule tests are check:rules' own.
        assert_eq!(
            changed(&["Cargo.lock", "tests/tooling/test-plan.test.ts"]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend -p dispatch-ci -p dispatch-host",
            ]
        );
        assert_eq!(
            changed(&["backend/host/src/updater.rs", "tests/browser/roles.spec.ts"])[1],
            "cargo test --locked -p dispatch-host"
        );
        assert!(changed(&["docs/readme.md", "dashboard/src/app/App.tsx"]).is_empty());
        // Without a plan, the changed tests themselves are still named.
        let files = vec!["tests/api/roles.test.ts".to_owned()];
        assert_eq!(
            affected(&files, &Value::Null),
            ["python3 tooling/cargo-build.py && npx tsx --test tests/api/roles.test.ts"]
        );
    }
    #[test]
    fn merge_queue_is_detected_only_from_a_well_formed_answer() {
        let answer = |reply: &'static str| {
            merge_queue(&|args: &[&str]| {
                assert_eq!(&args[..3], ["gh", "api", "graphql"]);
                assert!(args[4].contains("mergeQueue(branch: \"main\")"));
                Ok(reply.into())
            })
        };
        assert!(answer(
            r#"{"data":{"repository":{"mergeQueue":{"id":"MQ_1"}}}}"#
        ));
        assert!(!answer(r#"{"data":{"repository":{"mergeQueue":null}}}"#));
        assert!(!answer("[]"));
        assert!(!answer("not json"));
        assert!(!merge_queue(&|_: &[&str]| Err("offline".into())));
    }
}
