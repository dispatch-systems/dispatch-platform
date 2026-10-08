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
    assert!(
        error.contains("Usage: dispatchdev ship <PR number>"),
        "{error}"
    );
    let (ok, _, error) = run(&["build", "--scope", "full"]);
    assert!(!ok);
    assert!(error.contains("build takes no --scope"), "{error}");
    let (ok, _, error) = run(&["check", "--release"]);
    assert!(!ok);
    assert!(error.contains("check takes no --release"), "{error}");
    let (ok, _, error) = run(&["new", "widget"]);
    assert!(!ok);
    assert!(error.contains("Usage: dispatchdev new feature"), "{error}");
}

#[test]
fn new_makes_nothing_in_devs_checkout() {
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
}
