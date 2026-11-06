//! Workspace discovery and affected-test planning shared by `dispatchdev check` and `test`.
use crate::{Result, build::frontend};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
/// The workspace's crates: each member's directory and the package name its `Cargo.toml`
/// gives, as the root `Cargo.toml` lists the members, and which of them depend on which. A
/// member such as `features/*/contract` matches each wildcard directory segment.
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
            for dir in member_dirs(root, member)? {
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
            .filter(|(dir, _)| file.starts_with(&format!("{dir}/")))
            .max_by_key(|(dir, _)| dir.len())
            .map(|(_, name)| name)
    }
}
/// Expand wildcard directory segments, including those before a literal path suffix.
fn member_dirs(root: &Path, member: &str) -> Result<Vec<String>> {
    let mut dirs = vec![String::new()];
    for part in member.split('/') {
        let mut next = vec![];
        for dir in dirs {
            if part == "*" {
                if root.join(&dir).is_dir() {
                    for entry in fs::read_dir(root.join(&dir))? {
                        let entry = entry?;
                        if entry.path().is_dir() {
                            next.push(joined(&dir, &entry.file_name().to_string_lossy()));
                        }
                    }
                }
            } else {
                next.push(joined(&dir, part));
            }
        }
        dirs = next;
    }
    Ok(dirs)
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
        } else {
            let owner = owner(file);
            if owner.is_some() && frontend(Path::new(file)) {
                continue;
            }
            // Use the most-specific member before the owner fallback: a nested contract can
            // have consumers that do not depend on its owner's implementation crate.
            let name = workspace.holding(file).or_else(|| {
                owner
                    .as_deref()
                    .and_then(|owner| workspace.owned(owner).or(app))
            });
            if let Some(name) = name {
                crates.insert(name);
                crates.extend(workspace.dependents(name));
            }
            // Owners without their own crate still reach the app, as do owner files outside
            // a member (core's shared tests and the app's tests beside its backend).
            if owner.is_some() {
                crates.extend(app);
            }
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
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
    fn a_nested_contract_selects_its_consumers_and_their_dependents() {
        let (_root, workspace) = workspace(
            r#""core", "features/*", "features/*/contract", "mcp", "app/backend""#,
            &[
                ("core", "dispatch-core", &[]),
                (
                    "features/routes/contract",
                    "dispatch-routes-contract",
                    &["core"],
                ),
                (
                    "features/routes",
                    "dispatch-routes",
                    &["features/routes/contract"],
                ),
                (
                    "features/communications",
                    "dispatch-communications",
                    &["features/routes/contract"],
                ),
                ("mcp", "dispatch-mcp", &["features/routes/contract"]),
                (
                    "features/alerts",
                    "dispatch-alerts",
                    &["features/communications"],
                ),
                ("app/backend", "dispatch-backend", &["mcp"]),
            ],
        );
        let tested = |file: &str| affected(&[file.to_owned()], &Value::Null, &workspace)[1].clone();
        assert_eq!(
            tested("features/routes/contract/contract.rs"),
            "cargo test --locked -p dispatch-alerts -p dispatch-backend -p dispatch-communications -p dispatch-mcp -p dispatch-routes -p dispatch-routes-contract"
        );
        // An implementation edit does not affect the contract's other consumers.
        assert_eq!(
            tested("features/routes/backend/routes.rs"),
            "cargo test --locked -p dispatch-backend -p dispatch-routes"
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
}
