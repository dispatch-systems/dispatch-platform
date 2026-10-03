use crate::{REPOSITORY, Result, Runner, cache::frontend};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
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
/// The workspace's crates: each member's directory and the package name its `Cargo.toml`
/// gives, as the root `Cargo.toml` lists the members. A member such as `features/*` is every
/// directory under it with a `Cargo.toml`.
#[derive(Debug, Default)]
pub struct Workspace {
    crates: BTreeMap<String, String>,
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
                let name = manifest(&root.join(&dir))?
                    .get("package")
                    .and_then(|v| v.get("name"))
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| format!("{dir}/Cargo.toml names no package"))?
                    .to_owned();
                crates.insert(dir, name);
            }
        }
        Ok(Self { crates })
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
/// The owner a file belongs to, by its directory: core, a collector, a feature or the app.
fn owner(file: &str) -> Option<String> {
    let mut parts = file.split('/');
    match (parts.next()?, parts.next()?, parts.next()) {
        (top @ ("core" | "app"), _, _) => Some(top.to_owned()),
        (top @ ("collectors" | "features"), name, Some(_)) => Some(format!("{top}/{name}")),
        _ => None,
    }
}
/// The local commands that check what the diff touches: the Rust crates it changes, and the
/// test files it changes or that watch a source it changes, from `tooling/ci/test-plan.json`.
/// `npm run check:rules` already runs the rule and dashboard tests, so they are left out.
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
                // The owner's crate, or the app's for one not yet a crate of its own, which the
                // app mounts; and the app's, which builds on every owner.
                crates.extend(workspace.owned(&owner).or(app));
                crates.extend(app);
            }
        } else if let Some(name) = workspace.holding(file) {
            // A crate of the tooling's or the hosts'.
            crates.insert(name);
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
            "python3 tooling/build/cargo-build.py && npx tsx --test {}",
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
    // Without a readable workspace, as in a checkout with no Rust, no crate is named.
    let workspace = Workspace::read(root).unwrap_or_default();
    let commands = affected(&changed, &plan, &workspace);
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
    /// A workspace on disk of `members`, each crate a directory and its package's name.
    fn workspace(members: &str, crates: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let root = tempfile::tempdir().unwrap();
        let write = |file: &str, text: String| {
            let path = root.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write(
            "Cargo.toml",
            format!("[workspace]\nmembers = [{members}]\n"),
        );
        for (dir, name) in crates {
            write(
                &format!("{dir}/Cargo.toml"),
                format!("[package]\nname = \"{name}\"\n"),
            );
        }
        let workspace = Workspace::read(root.path()).unwrap();
        (root, workspace)
    }
    #[test]
    fn affected_names_the_changed_crates_and_the_tests_the_diff_changes_or_watches() {
        let (_root, workspace) = workspace(
            r#""core", "collectors/*", "app/backend", "ops/host-manager", "tooling/ci/dispatch-ci""#,
            &[
                ("core", "dispatch-core"),
                ("collectors/cortex", "dispatch-cortex"),
                ("collectors/paycom", "dispatch-paycom"),
                ("app/backend", "dispatch-backend"),
                ("ops/host-manager", "dispatch-host"),
                ("tooling/ci/dispatch-ci", "dispatch-ci"),
            ],
        );
        let plan = json!({
            "dashboard": ["core/tenancy/tests/frontend/features.test.ts"],
            "rules": ["tooling/tests/test-plan.test.ts"],
            "native": {"cortex": ["collectors/cortex/tests/native/cortex-worker.test.ts"]},
            "watch": [{"sources": ["core/tenancy/backend/roles.rs"], "tests": [
                "core/tenancy/tests/api/roles.test.ts",
                "core/platform_owner/tests/browser/dsp-features.spec.ts",
                "core/tenancy/tests/frontend/features.test.ts"]}]
        });
        let changed = |files: &[&str]| -> Vec<String> {
            let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            affected(&files, &plan, &workspace)
        };
        assert_eq!(
            changed(&[
                "core/tenancy/backend/roles.rs",
                "collectors/cortex/tests/native/cortex-worker.test.ts"
            ]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend -p dispatch-core",
                "python3 tooling/build/cargo-build.py && npx tsx --test core/tenancy/tests/api/roles.test.ts",
                "npm run test:browseros -- --shard cortex",
                "npm run build && npm run test:ui -- core/platform_owner/tests/browser/dsp-features.spec.ts",
            ]
        );
        // A workspace input touches every crate; rule tests are check:rules' own.
        assert_eq!(
            changed(&["Cargo.lock", "tooling/tests/test-plan.test.ts"]),
            [
                "cargo clippy --locked --all-targets -- -D warnings",
                "cargo test --locked -p dispatch-backend -p dispatch-ci -p dispatch-core -p dispatch-cortex -p dispatch-host -p dispatch-paycom",
            ]
        );
        assert_eq!(
            changed(&[
                "ops/host-manager/src/updater.rs",
                "features/team/tests/browser/roles.spec.ts"
            ])[1],
            "cargo test --locked -p dispatch-host"
        );
        // A collector's change runs its crate's tests, and the app's.
        assert_eq!(
            changed(&["collectors/paycom/connection/mod.rs"])[1],
            "cargo test --locked -p dispatch-backend -p dispatch-paycom"
        );
        assert!(changed(&["collectors/cortex/frontend/CortexCard.tsx"]).is_empty());
        assert!(changed(&["docs/readme.md", "dashboard/src/app/App.tsx"]).is_empty());
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
                "python3 tooling/build/cargo-build.py && npx tsx --test core/tenancy/tests/api/roles.test.ts"
            ]
        );
    }
    #[test]
    fn each_owners_crate_is_the_one_its_directory_holds() {
        let (root, _) = workspace(
            r#""core", "features/*", "app/backend""#,
            &[
                ("core", "dispatch-core"),
                ("features/driver_match", "dispatch-driver-match"),
                ("app/backend", "dispatch-backend"),
            ],
        );
        // A feature the app still mounts has a directory and no Cargo.toml.
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
