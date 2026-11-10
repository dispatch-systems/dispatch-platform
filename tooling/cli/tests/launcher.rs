//! `tooling/cli/dispatchdev` finds or builds the binary: a CI tool only from the workspace's
//! own `.ci-tools`, and anywhere else one build for each version of the source.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, UNIX_EPOCH},
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
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("cache.py");
    if cache.exists() {
        fs::copy(cache, root.join("tooling/cli/cache.py")).unwrap();
    }
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
        "#!/bin/sh\nset -eu\nprintf x >> \"$BUILDS\"\ntarget=$(printf '%s\\n' \"$@\" | sed -n '/--target-dir/{n;p;}')\n\
         mkdir -p \"$target/debug\"\nprintf '#!/bin/sh\\necho built %s \"$@\"\\n' \"$(wc -c < \"$BUILDS\")\" > \"$target/debug/dispatchdev\"\n\
         chmod +x \"$target/debug/dispatchdev\"\n",
    );
    (temp, root)
}
fn command(temp: &Path, root: &Path, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(root.join("tooling/cli/dispatchdev"));
    command
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", temp.join("bin").display()),
        )
        .env("BUILDS", temp.join("builds"))
        .envs(env.iter().copied());
    command
}
fn launch(temp: &Path, root: &Path, env: &[(&str, &str)]) -> Output {
    command(temp, root, env).arg("check").output().unwrap()
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
    // The ten newest versions are kept, so going back to one costs no build.
    change("tooling/shared/src/lib.rs", "shared");
    change("tooling/cli/src/main.rs", "first");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 1 check");
    // An eleventh drops the oldest, which is built again if its source comes back.
    for build in 4..=11 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        change("Cargo.lock", &format!("bumped {build}"));
        assert_eq!(
            said(&launch(temp.path(), &root, &[])),
            format!("built {build} check")
        );
    }
    change("Cargo.lock", "lock");
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 12 check");
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
    assert_eq!(said(&output), "built 12 status");
}

#[test]
fn each_checkout_builds_in_its_own_target() {
    // Cargo copies out only a binary it rebuilt, so a target shared by checkouts of different
    // sources would hand one of them the other's binary.
    let (temp, root) = checkout();
    let tree = worktree(temp.path(), &root);
    fs::write(tree.join("tooling/cli/src/main.rs"), "second").unwrap();
    assert_eq!(said(&launch(temp.path(), &root, &[])), "built 1 check");
    assert_eq!(said(&launch(temp.path(), &tree, &[])), "built 2 check");
    for checkout in [&root, &tree] {
        assert!(checkout.join("target/debug/dispatchdev").is_file());
    }
}

fn worktree(temp: &Path, root: &Path) -> PathBuf {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "first",
        ],
    );
    let tree = temp.join("tree");
    git(root, &["worktree", "add", "-q", tree.to_str().unwrap()]);
    tree
}

/// EOF releases the fixture, including when an assertion fails while it is waiting.
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        self.0.wait().unwrap();
    }
}

#[test]
fn a_running_cached_cli_survives_publication_and_eviction_until_it_dies() {
    use std::os::unix::fs::MetadataExt;
    let (temp, root) = checkout();
    let tree = worktree(temp.path(), &root);
    let binary = temp.path().join("executable");
    executable(
        &binary,
        "#!/bin/sh\nset -eu\nif [ \"${1:-}\" = child ]; then echo child; exit; fi\n\
         echo \"$0\"\nwhile read -r line; do \"$0\" child; done\n",
    );
    executable(
        &temp.path().join("bin/cargo"),
        "#!/bin/sh\nset -eu\nif [ \"${GATE:-}\" = yes ]; then echo building >&2; read -r line; fi\n\
         target=$(printf '%s\\n' \"$@\" | sed -n '/--target-dir/{n;p;}')\n\
         mkdir -p \"$target/debug\"\ncp \"$BINARY\" \"$target/debug/dispatchdev\"\n",
    );
    let env = [("BINARY", binary.to_str().unwrap())];
    let start = |gate| {
        Running(
            command(temp.path(), &root, &env)
                .env("GATE", gate)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    };
    let answer = |stream: &mut dyn std::io::Read| {
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "the fixture ended without answering");
        line.trim().to_owned()
    };
    let mut running = start("no");
    let active = PathBuf::from(answer(running.0.stdout.as_mut().unwrap()));
    // Move recency explicitly; no wall-clock wait decides which entries are old.
    let stamp = |entry: &Path, seconds| {
        fs::File::open(entry)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .unwrap();
    };
    stamp(active.parent().unwrap(), 1);
    let build = |number: u64| {
        fs::write(
            tree.join("tooling/cli/src/main.rs"),
            format!("version {number}"),
        )
        .unwrap();
        let output = command(temp.path(), &tree, &env).output().unwrap();
        let path = PathBuf::from(said(&output));
        assert!(path.is_file());
        assert_ne!(path, active);
        stamp(path.parent().unwrap(), number + 1);
        path
    };
    let unused = build(1);
    for number in 2..=10 {
        build(number);
    }
    assert!(
        active.is_file(),
        "a newer source evicted the running executable"
    );
    running
        .0
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"check\n")
        .unwrap();
    assert_eq!(answer(running.0.stdout.as_mut().unwrap()), "child");
    build(11);
    assert!(
        !unused.parent().unwrap().exists(),
        "an unused old entry was retained"
    );
    // Inspect only our fixture before killing it. SIGKILL cannot run cleanup.
    let process = Command::new("ps")
        .args(["-ww", "-p", &running.0.id().to_string(), "-o", "args="])
        .output()
        .unwrap();
    assert!(said(&process).contains(active.to_str().unwrap()));
    running.0.kill().unwrap();
    running.0.wait().unwrap();
    build(12);
    assert!(
        !active.parent().unwrap().exists(),
        "a dead run retained its cache entry"
    );
    assert_eq!(
        fs::read_dir(root.join(".git/dispatchdev")).unwrap().count(),
        10
    );
    // Two misses of the same inputs: one waits in Cargo while the other publishes
    // and starts using its executable. Finishing the first must not replace it.
    let mut delayed = start("yes");
    assert_eq!(answer(delayed.0.stderr.as_mut().unwrap()), "building");
    let mut competing = start("no");
    let active = PathBuf::from(answer(competing.0.stdout.as_mut().unwrap()));
    let identity = fs::metadata(&active).unwrap().ino();
    delayed
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert!(delayed.0.wait().unwrap().success());
    assert_eq!(
        fs::metadata(&active).unwrap().ino(),
        identity,
        "a concurrent publication replaced the running executable",
    );
}
