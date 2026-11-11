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
    let (ok, _, error) = run(&["review", "prepare"]);
    assert!(!ok);
    assert!(error.contains("review prepare needs --pass <n>"), "{error}");
    let (ok, _, error) = run(&["review", "take", "--pass", "1", "--model", "m"]);
    assert!(!ok);
    assert!(error.contains("review take takes no --model"), "{error}");
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
fn a_signalled_cli_names_its_sender_once_and_keeps_its_signal_exit_status() {
    use fs2::FileExt;
    use std::{
        fs,
        io::{BufRead, BufReader},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::process::{CommandExt, ExitStatusExt},
        },
        process::Stdio,
        sync::mpsc,
        time::Duration,
    };
    let fixture = checkout("signal-sender");
    let root = fixture.path();
    fs::create_dir_all(root.join("tooling/ci")).unwrap();
    fs::create_dir_all(root.join("tooling/cli/src")).unwrap();
    fs::write(root.join("tooling/ci/python.py"), "").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = ['tooling/cli']\n",
    )
    .unwrap();
    fs::write(
        root.join("tooling/cli/Cargo.toml"),
        "[package]\nname = 'dispatchdev-cli'\n",
    )
    .unwrap();
    fs::write(root.join("tooling/cli/src/lib.rs"), "").unwrap();
    let git = |args: &[&str]| {
        let result = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.com",
        "commit",
        "-qm",
        "fixture",
    ]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    fs::write(root.join("tooling/cli/src/lib.rs"), "// Changed input\n").unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.com",
        "commit",
        "-qm",
        "change",
    ]);
    // Only the fixture's slots are held. The real CLI announces when it reaches the wait;
    // no build or long-lived step is started, and no timer chooses when to send the signal.
    let slots = root.join(".git/dispatch-build-slots");
    fs::create_dir(&slots).unwrap();
    let held: Vec<_> = (0..2)
        .map(|slot| {
            let file = fs::File::create(slots.join(slot.to_string())).unwrap();
            file.lock_exclusive().unwrap();
            file
        })
        .collect();
    let scratch = dispatchdev_cli::workspace::checkout_scratch(root).unwrap();
    let waiting = |ignored: Option<i32>, stderr: Stdio| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dispatchdev"));
        command
            .args(["test", "--root", root.to_str().unwrap()])
            .env_remove("CI")
            .env_remove("DISPATCH_BUILD_SLOT")
            .stdout(Stdio::piped())
            .stderr(stderr);
        if let Some(signal) = ignored {
            // As nohup starts a command: the signal is ignored before the command runs.
            unsafe {
                command.pre_exec(move || {
                    libc::signal(signal, libc::SIG_IGN);
                    Ok(())
                })
            };
        }
        let mut child = command.spawn().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        for line in stdout.lines() {
            if line.unwrap().starts_with(dispatchdev_cli::slot::WAITING) {
                break;
            }
        }
        child
    };
    // A pipe nobody reads, filled to its last byte, then left to block as a terminal's does.
    let full = || {
        let mut ends = [0; 2];
        assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
        let [read, write] = ends.map(|end| unsafe { OwnedFd::from_raw_fd(end) });
        for end in [&read, &write] {
            unsafe { libc::fcntl(end.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
        }
        let flags = unsafe { libc::fcntl(write.as_raw_fd(), libc::F_GETFL) };
        unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) };
        for size in [4096, 1] {
            let bytes = vec![0u8; size];
            while unsafe { libc::write(write.as_raw_fd(), bytes.as_ptr().cast(), size) } > 0 {}
        }
        unsafe { libc::fcntl(write.as_raw_fd(), libc::F_SETFL, flags) };
        (read, write)
    };
    // A command a signal failed to end would hang the suite. It is killed after half a
    // minute, and the signal asserted of it is then not the one it was sent.
    let ended = |mut child: std::process::Child| {
        let pid = child.id() as i32;
        let (end, wait) = mpsc::channel();
        let watch = std::thread::spawn(move || {
            if wait.recv_timeout(Duration::from_secs(30)).is_err() {
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        });
        let status = child.wait().unwrap();
        // The watchdog has gone if it had to kill the command.
        let _ = end.send(());
        watch.join().unwrap();
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            std::io::Read::read_to_string(&mut pipe, &mut stderr).unwrap();
        }
        (status, stderr)
    };
    for (signal, name, stderr) in [
        (libc::SIGTERM, "SIGTERM", "read"),
        (libc::SIGINT, "SIGINT", "read"),
        (libc::SIGHUP, "SIGHUP", "read"),
        (libc::SIGTERM, "SIGTERM", "closed"),
        (libc::SIGTERM, "SIGTERM", "stalled"),
    ] {
        let stderr_open = stderr == "read";
        let stalled = (stderr == "stalled").then(full);
        let mut child = waiting(
            None,
            match &stalled {
                Some((_, write)) => Stdio::from(write.try_clone().unwrap()),
                None => Stdio::piped(),
            },
        );
        let pid = child.id();
        if stderr == "closed" {
            drop(child.stderr.take());
        }
        // Inspect the exact process we started before signalling it.
        let process = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&process.stdout).contains("dispatchdev"));
        assert_eq!(unsafe { libc::kill(pid as i32, signal) }, 0);
        let (status, stderr) = ended(child);
        assert_eq!(status.signal(), Some(signal), "{name}, stderr {stderr}");
        assert_eq!(128 + status.signal().unwrap(), 128 + signal);
        let lines: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("dispatchdev: received "))
            .collect();
        if stderr_open {
            assert_eq!(lines.len(), 1, "one sender diagnostic for {name}: {stderr}");
        }
        let log =
            fs::read_to_string(scratch.join("test").join(pid.to_string()).join("1.log")).unwrap();
        let logged: Vec<_> = log.lines().collect();
        assert_eq!(
            logged.len(),
            1,
            "one sender diagnostic in the run log: {log}"
        );
        let line = logged[0];
        if stderr_open {
            assert_eq!(lines, [line]);
        }
        assert!(line.contains(&format!("received {name} ({signal})")));
        assert!(
            line.contains(&format!("sender pid={} ", std::process::id())),
            "{line}"
        );
        assert!(
            line.contains(&format!("uid={} ", unsafe { libc::getuid() })),
            "{line}"
        );
        assert!(
            line.contains(&format!("ppid={} ", unsafe { libc::getppid() })),
            "{line}"
        );
        let me = std::env::current_exe().unwrap().canonicalize().unwrap();
        #[cfg(target_os = "linux")]
        assert!(line.contains(me.to_str().unwrap()), "{line}");
        // macOS names the executable alone: its arguments come with its environment.
        #[cfg(target_os = "macos")]
        assert!(
            line.ends_with(&format!(
                "command={}",
                serde_json::to_string(me.to_str().unwrap()).unwrap()
            )),
            "{line}"
        );
    }
    // A signal the command was started ignoring is still ignored, and another still ends it.
    let child = waiting(Some(libc::SIGHUP), Stdio::piped());
    let pid = child.id();
    // Linux says which signals a process ignores and which it catches; macOS has no such
    // report, and the code deciding it is the same there.
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        let mask = |name: &str| {
            let line = status.lines().find(|line| line.starts_with(name)).unwrap();
            u64::from_str_radix(line.split_whitespace().nth(1).unwrap(), 16).unwrap()
        };
        let hangup = 1 << (libc::SIGHUP - 1);
        assert_ne!(mask("SigIgn:") & hangup, 0, "{status}");
        assert_eq!(mask("SigCgt:") & hangup, 0, "{status}");
    }
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    let (status, stderr) = ended(child);
    assert_eq!(status.signal(), Some(libc::SIGTERM), "{stderr}");
    let log = fs::read_to_string(scratch.join("test").join(pid.to_string()).join("1.log")).unwrap();
    assert!(log.contains("received SIGTERM (15)"), "{log}");
    drop(held);
    let contents: Vec<_> = fs::read_dir(&scratch)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(contents, ["test"]);
    fs::remove_dir_all(scratch).unwrap();
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
