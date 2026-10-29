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

#[test]
fn the_scaffolder_keeps_its_temporary_files_in_the_checkouts_scratch_folder_by_its_real_path() {
    // A checkout whose scaffolder, run by node as the checkout's tsx is, writes down the
    // TMPDIR it was given.
    let name = "scaffolder-scratch";
    let workspace = tempfile::tempdir().unwrap();
    let checkout = workspace.path().join("worktrees").join(name);
    std::fs::create_dir_all(checkout.join("tooling/scaffold")).unwrap();
    std::fs::write(checkout.join("tooling/scaffold/cli.ts"), "").unwrap();
    std::fs::create_dir_all(checkout.join("node_modules/tsx/dist")).unwrap();
    std::fs::write(
        checkout.join("node_modules/tsx/dist/cli.mjs"),
        "import { writeFileSync } from 'node:fs';\n\
         writeFileSync('tmpdir', process.env.TMPDIR ?? '');\n",
    )
    .unwrap();
    let (ok, _, error) = run(&["generate", "--check", "--root", checkout.to_str().unwrap()]);
    assert!(ok, "{error}");
    // The scratch folder by its real path, made before the scaffolder ran.
    let scratch = std::fs::canonicalize("/tmp")
        .unwrap()
        .join(format!("dispatch-{name}"));
    assert_eq!(
        std::fs::read_to_string(checkout.join("tmpdir")).unwrap(),
        scratch.to_str().unwrap()
    );
    assert!(scratch.is_dir());
    std::fs::remove_dir_all(scratch).unwrap();
}
