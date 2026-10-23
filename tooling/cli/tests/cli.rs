use std::process::Command;

fn run(args: &[&str]) -> (bool, String, String) {
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .args(args)
        .output()
        .unwrap();
    (
        result.status.success(),
        String::from_utf8_lossy(&result.stdout).into_owned(),
        String::from_utf8_lossy(&result.stderr).into_owned(),
    )
}

#[test]
fn dispatchdev_shows_its_usage_and_refuses_what_it_does_not_take() {
    let (ok, _, error) = run(&[]);
    assert!(!ok);
    assert!(error.contains("Usage: dispatchdev <command>"), "{error}");
    let (ok, _, _) = run(&["help"]);
    assert!(ok);
    let (ok, _, error) = run(&["deploy"]);
    assert!(!ok);
    assert!(error.contains("Unknown command deploy"), "{error}");
    let (ok, _, error) = run(&["ship"]);
    assert!(!ok);
    assert!(error.contains("Usage: dispatchdev ship <name>"), "{error}");
    let (ok, _, error) = run(&["build", "--scope", "full"]);
    assert!(!ok);
    assert!(error.contains("build takes no --scope"), "{error}");
    let (ok, _, error) = run(&["check", "--release"]);
    assert!(!ok);
    assert!(error.contains("check takes no --release"), "{error}");
    let (ok, _, error) = run(&["build", "--scope=full"]);
    assert!(!ok);
    assert!(error.contains("build takes no --scope=full"), "{error}");
    let (ok, help, _) = run(&["ship", "--help"]);
    assert!(ok);
    assert!(help.starts_with("Usage: dispatchdev ship <name>"), "{help}");
    let (ok, help, _) = run(&["help", "generate"]);
    assert!(ok);
    assert!(help.starts_with("Usage: dispatchdev generate"), "{help}");
    let (ok, _, error) = run(&["browser", "dev"]);
    assert!(!ok);
    assert!(error.contains("Usage: dispatchdev browser"), "{error}");
    let (ok, _, error) = run(&["verify"]);
    assert!(!ok);
    assert!(
        error.contains("Usage: dispatchdev verify <feature>"),
        "{error}"
    );
    let (ok, _, error) = run(&["test", "--only", "rust"]);
    assert!(!ok);
    assert!(error.contains("an --owner's tests"), "{error}");
}

#[test]
fn new_and_generate_write_nothing_in_devs_checkout() {
    // A workspace whose Dev checkout has a scaffolder that would write if it ran.
    let workspace = tempfile::tempdir().unwrap();
    let dev = workspace.path().join("dev");
    std::fs::create_dir_all(dev.join(".git")).unwrap();
    std::fs::create_dir_all(workspace.path().join("worktrees")).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .args(["new", "feature", "fleet_log", "--root"])
        .arg(&dev)
        .current_dir(&dev)
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(error.contains("Make it in a change's worktree"), "{error}");
    assert!(!dev.join("features").exists());
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .args(["generate", "--root"])
        .arg(&dev)
        .current_dir(&dev)
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(error.contains("Make it in a change's worktree"), "{error}");
}

#[test]
fn generate_runs_in_a_worktree_on_a_disk_of_its_own() {
    // worktrees/ links to another folder, where git makes the worktree and reports it.
    let workspace = tempfile::tempdir().unwrap();
    let (dev, elsewhere) = (
        workspace.path().join("dev"),
        workspace.path().join("elsewhere"),
    );
    let git = |args: &[&str]| {
        let done = Command::new("git")
            .args(args)
            .current_dir(&dev)
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, workspace.path().join("worktrees")).unwrap();
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.com",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "first",
    ]);
    git(&["worktree", "add", "-q", "-b", "one", "../worktrees/one"]);
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .arg("generate")
        .current_dir(workspace.path().join("worktrees/one"))
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    // It gets past where it's made, to the checkout's scaffolder, which this one lacks.
    assert!(
        error.contains("predates dispatchdev's scaffolder"),
        "{error}"
    );
}
