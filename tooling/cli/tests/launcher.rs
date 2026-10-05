//! `tooling/cli/dispatchdev` finds or builds the binary: a CI tool only from the workspace's
//! own `.ci-tools`, and anywhere else one build for each version of the source.
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

const LAUNCHER: &str = include_str!("../dispatchdev");

fn executable(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
/// A checkout holding the launcher and the sources it versions, with a `cargo` that counts
/// its builds and writes a binary naming the build it came from.
fn checkout() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap().join("checkout");
    executable(&root.join("tooling/cli/dispatchdev"), LAUNCHER);
    for (file, text) in [
        ("tooling/cli/src/main.rs", "first"),
        ("tooling/shared/src/lib.rs", "shared"),
        ("Cargo.toml", "[workspace]"),
        ("Cargo.lock", "lock"),
        ("rust-toolchain.toml", "toolchain"),
    ] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::write(root.join(file), text).unwrap();
    }
    git(&root, &["init", "-q"]);
    executable(
        &temp.path().join("bin/cargo"),
        "#!/bin/sh\nset -eu\nprintf x >> \"$BUILDS\"\ntarget=$(printf '%s\\n' \"$@\" | sed -n '/--target-dir/{n;p}')\n\
         mkdir -p \"$target/debug\"\nprintf '#!/bin/sh\\necho built %s \"$@\"\\n' \"$(wc -c < \"$BUILDS\")\" > \"$target/debug/dispatchdev\"\n\
         chmod +x \"$target/debug/dispatchdev\"\n",
    );
    (temp, root)
}
fn launch(temp: &Path, root: &Path, env: &[(&str, &str)]) -> Output {
    Command::new(root.join("tooling/cli/dispatchdev"))
        .arg("check")
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", temp.join("bin").display()),
        )
        .env("BUILDS", temp.join("builds"))
        .envs(env.iter().copied())
        .output()
        .unwrap()
}
fn said(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn a_ci_tool_runs_only_from_the_workspaces_own_ci_tools() {
    let (temp, root) = checkout();
    let tools = root.join(".ci-tools");
    let tool = tools.join("tools/dispatchdev");
    let trusted = [
        ("CI", "true"),
        ("DISPATCH_CI_TOOLS", tools.to_str().unwrap()),
    ];
    // Nothing restored: it builds, as anywhere else.
    assert_eq!(said(&launch(temp.path(), &root, &trusted)), "built 1 check");
    executable(&tool, "#!/bin/sh\necho prebuilt \"$@\"\n");
    assert_eq!(
        said(&launch(temp.path(), &root, &trusted)),
        "prebuilt check"
    );
    let elsewhere = temp.path().join("elsewhere");
    for untrusted in [
        vec![("DISPATCH_CI_TOOLS", tools.to_str().unwrap())],
        vec![("CI", "true")],
        vec![
            ("CI", "true"),
            ("DISPATCH_CI_TOOLS", elsewhere.to_str().unwrap()),
        ],
    ] {
        assert_eq!(
            said(&launch(temp.path(), &root, &untrusted)),
            "built 1 check"
        );
    }
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        said(&launch(temp.path(), &root, &trusted)),
        "built 1 check",
        "not executable"
    );
    fs::remove_file(&tool).unwrap();
    symlink("/bin/echo", &tool).unwrap();
    assert_eq!(
        said(&launch(temp.path(), &root, &trusted)),
        "built 1 check",
        "a symlink"
    );
}

#[test]
fn each_version_of_the_source_is_built_once() {
    let (temp, root) = checkout();
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 1 check");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 1 check");
    let change = |file: &str, text: &str| fs::write(root.join(file), text).unwrap();
    change("tooling/cli/src/main.rs", "second");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 2 check");
    change("tooling/shared/src/lib.rs", "changed");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 3 check");
    // The three newest versions are kept, so going back to one costs no build.
    change("tooling/shared/src/lib.rs", "shared");
    change("tooling/cli/src/main.rs", "first");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 1 check");
    // A fourth drops the oldest, which is built again if its source comes back.
    std::thread::sleep(std::time::Duration::from_millis(20));
    change("Cargo.lock", "bumped");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 4 check");
    change("Cargo.lock", "lock");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 5 check");
    // Run through a link, it is still this checkout's.
    let link = temp.path().join("bin/dispatchdev");
    symlink(root.join("tooling/cli/dispatchdev"), &link).unwrap();
    let output = Command::new(&link)
        .arg("status")
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", temp.path().join("bin").display()),
        )
        .env("BUILDS", temp.path().join("builds"))
        .output()
        .unwrap();
    assert_eq!(said(&output), "built 5 status");
}
