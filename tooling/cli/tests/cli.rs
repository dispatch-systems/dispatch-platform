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
    // A change's name is read, and refused, before anything asks git or GitHub.
    for command in ["start", "finish"] {
        let (ok, _, error) = run(&[command]);
        assert!(!ok);
        assert!(
            error.contains(&format!("Usage: dispatchdev {command} <name>")),
            "{error}"
        );
        let (ok, _, error) = run(&[command, "Fix_Week"]);
        assert!(!ok);
        assert!(error.contains("A change's name is lowercase"), "{error}");
    }
    let (ok, _, error) = run(&["pr", "--skeleton"]);
    assert!(!ok);
    assert!(error.contains("--skeleton needs the --title"), "{error}");
    let (ok, _, error) = run(&["browser"]);
    assert!(!ok);
    assert!(
        error.contains("Usage: dispatchdev browser <dev | production>"),
        "{error}"
    );
    let (ok, _, error) = run(&["build", "--scope", "full"]);
    assert!(!ok);
    assert!(error.contains("build takes no --scope"), "{error}");
    let (ok, _, error) = run(&["check", "--release"]);
    assert!(!ok);
    assert!(error.contains("check takes no --release"), "{error}");
    let (ok, _, error) = run(&["build", "--scope=full"]);
    assert!(!ok);
    assert!(error.contains("build takes no --scope=full"), "{error}");
    let (ok, help, _) = run(&["browser", "--help"]);
    assert!(ok);
    assert!(
        help.starts_with("Usage: dispatchdev browser <dev | production>"),
        "{help}"
    );
    let (ok, _, error) = run(&["api", "GET"]);
    assert!(!ok);
    assert!(
        error.contains("Usage: dispatchdev api <method> <path>"),
        "{error}"
    );
    let (ok, help, _) = run(&["help", "generate"]);
    assert!(ok);
    assert!(help.starts_with("Usage: dispatchdev generate"), "{help}");
    assert!(run(&["help", "--help"]).0);
    assert!(run(&["help", "-h"]).0);
    let (ok, _, error) = run(&["help", "build", "unexpected"]);
    assert!(!ok);
    assert!(error.contains("help takes no unexpected"), "{error}");
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
    let (ok, _, error) = run(&["android", "prepare", "--app", "relay"]);
    assert!(!ok);
    assert!(error.contains("needs --device"), "{error}");
    let (ok, _, error) = run(&["android", "doctor", "--app", "relay"]);
    assert!(!ok);
    assert!(error.contains("takes no --app"), "{error}");
    let (ok, help, _) = run(&["android", "--help"]);
    assert!(ok);
    assert!(help.contains("android prepare --app <module>"), "{help}");
}

#[test]
fn test_planning_on_dirty_main_selects_prerequisites_without_executing_or_creating_state() {
    let fixture = checkout("main");
    let file = "features/parking/tests/api/routes.test.ts";
    let path = fixture.path().join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "throw new Error('the plan must not run tests');").unwrap();
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(fixture.path())
        .output()
        .unwrap();
    let head = String::from_utf8(head.stdout).unwrap();
    let update = Command::new("git")
        .args(["update-ref", "refs/remotes/origin/main", head.trim()])
        .current_dir(fixture.path())
        .status()
        .unwrap();
    assert!(update.success());
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .args([
            "test",
            "--plan",
            "--json",
            "--root",
            fixture.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["head"], head.trim());
    assert_eq!(plan["dirty"], true);
    assert!(
        plan["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == file)
    );
    assert!(
        plan["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["command"].as_str().unwrap().contains(file))
    );
    assert!(
        plan["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["command"] == "tooling/cli/dispatchdev build")
    );
    assert!(!fixture.path().join("target").exists());
    assert!(!fixture.path().join(".test-build").exists());
    assert!(!fixture.path().join(".runtime").exists());
}

/// A checkout with one commit, on `branch`.
fn checkout(branch: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let done = Command::new("git")
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
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
    if branch != "main" {
        git(&["switch", "-q", "-c", branch]);
    }
    temp
}

#[test]
fn new_and_generate_write_nothing_on_main() {
    let main = checkout("main");
    for args in [&["new", "feature", "fleet_log"][..], &["generate"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
            .args(args)
            .current_dir(main.path())
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success());
        assert!(error.contains("Make it on a branch of its own"), "{error}");
    }
    assert!(!main.path().join("features").exists());
}

#[test]
fn generate_runs_on_a_branch() {
    let branch = checkout("fleet-log");
    let result = Command::new(env!("CARGO_BIN_EXE_dispatchdev"))
        .arg("generate")
        .current_dir(branch.path())
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
    // A checkout of a name of its own, whose scaffolder, run by node as the checkout's tsx is,
    // writes down the TMPDIR it was given.
    let name = format!("scaffolder-scratch-{}", std::process::id());
    let workspace = tempfile::tempdir().unwrap();
    let checkout = workspace.path().join(&name);
    std::fs::create_dir_all(checkout.join("tooling/scaffold")).unwrap();
    std::fs::write(checkout.join("tooling/scaffold/cli.ts"), "").unwrap();
    std::fs::create_dir_all(checkout.join("node_modules/tsx/dist")).unwrap();
    std::fs::write(
        checkout.join("node_modules/tsx/dist/cli.mjs"),
        "import { writeFileSync } from 'node:fs';\n\
         writeFileSync('tmpdir', process.env.TMPDIR ?? '');\n",
    )
    .unwrap();
    // Its scratch folder is a link to a folder of the test's own, as macOS keeps /tmp behind
    // one; made here, so it is no one else's, and the only thing removed.
    let target = workspace.path().join("scratch");
    std::fs::create_dir(&target).unwrap();
    let scratch = std::path::PathBuf::from(format!("/tmp/dispatch-{name}"));
    std::os::unix::fs::symlink(&target, &scratch).unwrap();
    let (ok, _, error) = run(&["generate", "--check", "--root", checkout.to_str().unwrap()]);
    std::fs::remove_file(&scratch).unwrap();
    assert!(ok, "{error}");
    assert_eq!(
        std::fs::read_to_string(checkout.join("tmpdir")).unwrap(),
        std::fs::canonicalize(&target).unwrap().to_str().unwrap()
    );
}
