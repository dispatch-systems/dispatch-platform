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
}
