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
fn dispatchdev_names_its_commands_and_refuses_the_rest() {
    let (ok, _, error) = run(&[]);
    assert!(!ok);
    assert!(error.contains("Usage: dispatchdev <command>"), "{error}");
    let (ok, help, _) = run(&["help"]);
    assert!(ok);
    for command in [
        "start", "preview", "api", "check", "pr", "ship", "finish", "status", "build",
    ] {
        assert!(help.contains(&format!("\n  {command} ")), "{help}");
    }
    // The old names, and the planner, receipts and gate before them, are gone.
    for gone in ["preflight", "plan", "receipt", "gate"] {
        let (ok, _, error) = run(&[gone]);
        assert!(!ok, "{gone}");
        assert!(
            error.contains(&format!("Unknown command {gone}")),
            "{error}"
        );
    }
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
}
