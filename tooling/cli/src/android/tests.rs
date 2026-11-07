use super::*;
use crate::api;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    scratch: PathBuf,
    env: Environment,
    calls: RefCell<Vec<Vec<String>>>,
    failure: Cell<&'static str>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let scratch = crate::workspace::scratch(root.file_name().unwrap().to_str().unwrap());
        fs::create_dir(root.join("scratch")).unwrap();
        std::os::unix::fs::symlink(root.join("scratch"), &scratch).unwrap();
        for folder in [
            "sdk/platform-tools",
            "android/inventory/src",
            "android/ops/scanner/src",
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("sdk/platform-tools/adb"), "").unwrap();
        fs::write(
            root.join("android/inventory/src/Main.kt"),
            "inventory source",
        )
        .unwrap();
        fs::write(
            root.join("android/ops/scanner/src/Main.kt"),
            "scanner source",
        )
        .unwrap();
        let env = Environment::from([
            ("HOME".into(), root.to_str().unwrap().into()),
            (
                "ANDROID_HOME".into(),
                root.join("sdk").to_str().unwrap().into(),
            ),
            (
                "JAVA_HOME".into(),
                root.join("java").to_str().unwrap().into(),
            ),
            ("CI".into(), "true".into()),
        ]);
        Self {
            _temp: temp,
            root,
            scratch,
            env,
            calls: RefCell::new(vec![]),
            failure: Cell::new(""),
        }
    }

    fn run(&self, action: Action<'_>) -> Result<Option<Value>> {
        execute(&self.root, action, &self.env, self)
    }

    fn prepare(&self, app: &str, variant: Option<&str>) -> Result<Value> {
        self.run(Action::Prepare {
            app,
            device: "emulator-5560",
            variant,
        })?
        .ok_or_else(|| "Preparation returned no result".into())
    }

    fn installations(&self) -> Vec<Vec<String>> {
        self.calls
            .borrow()
            .iter()
            .filter(|call| call.get(3).is_some_and(|arg| arg == "install"))
            .cloned()
            .collect()
    }

    fn write(&self, path: &str, value: &Value) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value.to_string()).unwrap();
    }

    fn server(&self, origin: &str, link: &str) {
        self.write(api::RECORD, &json!({"pid": std::process::id(), "origin": origin, "link": link, "password": "fixture-only"}));
    }

    fn gradlew(&self, script: &str) -> Tools {
        use std::os::unix::fs::PermissionsExt;
        let path = self.root.join("android/gradlew");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        Tools::find(&self.root, &self.env).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_file(&self.scratch).unwrap();
    }
}

impl Runner for Fixture {
    fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
        self.calls
            .borrow_mut()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        let failure = self.failure.get();
        if args[0] == "git" {
            assert_eq!(cwd, Some(self.root.as_path()));
            return Ok(match args[1] {
                "rev-parse" => b"0123456789abcdef0123456789abcdef01234567\n".to_vec(),
                "status" => b" M android/inventory/src/Main.kt\n".to_vec(),
                "ls-files" => {
                    b"android/inventory/src/Main.kt\0android/ops/scanner/src/Main.kt\0".to_vec()
                }
                other => panic!("unexpected git {other}"),
            });
        }
        if args[0].ends_with("/bin/java") {
            assert_eq!(timeout, 15);
            return Ok(if failure == "java" {
                b"openjdk 17.0.1\n".to_vec()
            } else {
                b"openjdk 25.0.1\n".to_vec()
            });
        }
        if args[0].ends_with("/adb") {
            assert_eq!(timeout, 60);
            assert_eq!(cwd, None);
            if args[1] == "devices" {
                return Ok(match failure {
                    "offline" => "emulator-5560\toffline\n",
                    "duplicate" => "emulator-5560\tdevice\nemulator-5560\tdevice\n",
                    "missing" => "emulator-5554\tdevice\n",
                    _ => "emulator-5554\tdevice\nemulator-5560\tdevice\n",
                }
                .as_bytes()
                .to_vec());
            }
            if args[1] == "version" {
                return Ok(b"Android Debug Bridge version 1.0.41".to_vec());
            }
            assert_eq!(&args[1..3], &["-s", "emulator-5560"]);
            return match args[3] {
                "shell" if args[5] == "ro.kernel.qemu" => Ok(if failure == "physical" {
                    b"0".to_vec()
                } else {
                    b"1".to_vec()
                }),
                "shell" if args[5] == "sys.boot_completed" => Ok(if failure == "booting" {
                    b"0".to_vec()
                } else {
                    b"1".to_vec()
                }),
                "install" if failure == "install" => Err("installation refused".into()),
                "install" => {
                    assert_eq!(&args[4..6], &["-r", "-d"]);
                    assert_eq!(args.len(), 7);
                    Ok(b"Success\n".to_vec())
                }
                "reverse" => {
                    assert_eq!(&args[4..], &["tcp:41733", "tcp:41733"]);
                    Ok(vec![])
                }
                other => panic!("unexpected device command {other}"),
            };
        }
        if args[0] == "bash" {
            assert_eq!(timeout, 900);
            assert_eq!(cwd, Some(self.root.join("android").as_path()));
            let script = args.last().unwrap();
            if script.contains("dispatchdevApps") {
                let report =
                    |name: &str| self.root.join(format!("android/{name}/build/report.json"));
                let mut variants = vec![
                    json!({"module":":inventory", "variant":"devDebug", "flavors":["dev"], "task":":inventory:dispatchdevDevDebugApk", "report":report("inventory")}),
                    json!({"module":":inventory", "variant":"productionDebug", "flavors":["production"], "task":":inventory:dispatchdevProductionDebugApk", "report":report("inventory")}),
                    json!({"module":":ops:scanner", "variant":"qaPreview", "flavors":["qa"], "task":":ops:scanner:dispatchdevQaPreviewApk", "report":report("ops/scanner")}),
                ];
                if failure == "ambiguous" {
                    variants.push(json!({"module":":ops:scanner", "variant":"betaPreview", "flavors":["beta"], "task":":ops:scanner:dispatchdevBetaPreviewApk", "report":report("ops/scanner")}));
                }
                self.write("android/build/dispatchdev/apps.json", &json!(variants));
                return Ok(vec![]);
            }
            if failure == "build" {
                return Err("Gradle build failed".into());
            }
            let (module, variant, package, filename) =
                if script.contains(":inventory:dispatchdevDevDebugApk") {
                    (
                        "inventory",
                        "devDebug",
                        "com.example.stock.dev.debug",
                        "stock-local.apk",
                    )
                } else if script.contains(":ops:scanner:dispatchdevQaPreviewApk") {
                    (
                        "ops/scanner",
                        "qaPreview",
                        "org.example.scanner.qa",
                        "scan-preview-universal.apk",
                    )
                } else {
                    panic!("unexpected Gradle build: {script}");
                };
            let apk = self
                .root
                .join(format!("android/{module}/build/custom-output/{filename}"));
            fs::create_dir_all(apk.parent().unwrap()).unwrap();
            fs::write(&apk, format!("{package} binary")).unwrap();
            let apks = if failure == "splits" {
                vec![apk.clone(), apk]
            } else {
                vec![apk]
            };
            self.write(
                &format!("android/{module}/build/report.json"),
                &json!({"applicationId":package,"variant":variant,"apks":apks}),
            );
            if failure == "source" {
                fs::write(
                    self.root.join("android/inventory/src/Main.kt"),
                    "changed while building",
                )
                .unwrap();
            }
            return Ok(if failure == "warning" {
                b"w: a Kotlin warning\n".to_vec()
            } else {
                vec![]
            });
        }
        if args[0] == "curl" {
            let url = args.last().unwrap();
            let answer = if url.ends_with("/api/session/dsp") {
                assert!(args.contains(&"x-csrf-token: fixture-csrf"));
                json!({"token":"fixture-view"})
            } else if url.ends_with("/phones/pairing") {
                assert!(args.contains(&"x-csrf-token: fixture-csrf"));
                assert!(args.contains(&"x-dispatch-view: fixture-view"));
                if failure == "pairing" {
                    return Ok(b"{\"error\":\"phones_off\"}\n403".to_vec());
                }
                json!({"code":"ABCD-1234", "expiresAt":123456})
            } else if url.ends_with("/api/session") {
                json!({"csrf":"fixture-csrf", "dsps":[{"id":"dsp-summit", "name":"Summit Delivery", "code":"sum"}]})
            } else {
                panic!("unexpected API request {url}");
            };
            return Ok(format!("{answer}\n200").into_bytes());
        }
        if args[0] == "ffmpeg" {
            assert_eq!(timeout, 15);
            return if failure == "encoder" {
                Err("missing encoder".into())
            } else {
                Ok(vec![])
            };
        }
        panic!("unexpected command: {args:?}")
    }
}

#[test]
fn prepare_builds_only_the_selected_app_and_installs_its_actual_apk_on_the_explicit_emulator() {
    for (app, variant, apk, package) in [
        (
            "inventory",
            None,
            "stock-local.apk",
            "com.example.stock.dev.debug",
        ),
        (
            "ops:scanner",
            Some("qaPreview"),
            "scan-preview-universal.apk",
            "org.example.scanner.qa",
        ),
    ] {
        let fixture = Fixture::new();
        let prepared = fixture.prepare(app, variant).unwrap();
        assert_eq!(prepared["package"], package);
        assert_eq!(prepared["device"], "emulator-5560");
        let installs = fixture.installations();
        assert_eq!(installs.len(), 1);
        assert!(installs[0][6].ends_with(apk), "{installs:?}");
        let calls = fixture.calls.borrow();
        assert_eq!(calls.iter().filter(|args| args[0] == "bash").count(), 2);
        assert!(!calls.iter().any(|args| {
            args.iter()
                .any(|arg| matches!(arg.as_str(), "clean" | "uninstall" | "clear" | "am"))
        }));
    }
}

#[test]
fn invalid_targets_variants_and_failed_builds_never_install() {
    for (failure, app, variant, expected) in [
        ("offline", "inventory", None, "offline"),
        ("duplicate", "inventory", None, "ambiguous"),
        ("missing", "inventory", None, "missing"),
        ("physical", "inventory", None, "not an Android emulator"),
        ("booting", "inventory", None, "still booting"),
        ("java", "inventory", None, "Java 25"),
        ("", "library", None, "not an application"),
        ("", "inventory", Some("notDeclared"), "No variant"),
        ("ambiguous", "ops:scanner", None, "Choose --variant"),
        ("build", "inventory", None, "Gradle build failed"),
        ("splits", "inventory", None, "multiple or no APKs"),
        ("source", "inventory", None, "source changed"),
        ("warning", "inventory", None, "compiler warnings"),
    ] {
        let fixture = Fixture::new();
        fixture.failure.set(failure);
        let error = fixture.prepare(app, variant).unwrap_err().to_string();
        assert!(error.contains(expected), "{failure}: {error}");
        assert!(fixture.installations().is_empty(), "{failure}");
    }
    let fixture = Fixture::new();
    fixture.failure.set("install");
    assert!(
        fixture
            .prepare("inventory", None)
            .unwrap_err()
            .to_string()
            .contains("installation refused")
    );
}

#[test]
fn a_device_operation_cannot_overlap_another_checkout_and_releases_its_lock_on_failure() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let fixture = Fixture::new();
    let tools = Tools::find(&fixture.root, &fixture.env).unwrap();
    let held = Device::take(
        &fixture.root,
        "emulator-5560",
        &tools,
        &fixture.env,
        &fixture,
    )
    .unwrap();
    let other = fixture.root.join("another-checkout");
    let error = Device::take(&other, "emulator-5560", &tools, &fixture.env, &fixture)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("another dispatchdev operation"), "{error}");
    let mut pipe = [0; 2];
    assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
    let read = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
    let write = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
        // After fork in a threaded test, use only async-signal-safe calls until _exit.
        unsafe {
            libc::close(write.as_raw_fd());
            let mut byte = 0u8;
            libc::read(read.as_raw_fd(), (&mut byte as *mut u8).cast(), 1);
            libc::_exit(0);
        }
    }
    drop(read);
    drop(held);
    let reacquired = Device::take(&other, "emulator-5560", &tools, &fixture.env, &fixture);
    drop(write);
    assert_eq!(
        unsafe { libc::waitpid(child, std::ptr::null_mut(), 0) },
        child
    );
    drop(reacquired.unwrap());
    fixture.failure.set("offline");
    assert!(fixture.prepare("inventory", None).is_err());
    fixture.failure.set("");
    fixture.prepare("inventory", None).unwrap();
}

#[test]
fn tools_are_explicit_and_app_discovery_does_not_build_an_apk() {
    let fixture = Fixture::new();
    fixture.run(Action::Apps).unwrap();
    assert_eq!(
        fixture
            .calls
            .borrow()
            .iter()
            .filter(|args| args[0] == "bash")
            .count(),
        1
    );
    assert!(fixture.installations().is_empty());
    fixture.run(Action::Doctor).unwrap();
    fixture.failure.set("encoder");
    assert!(fixture.run(Action::Doctor).is_err());
    let mut env = fixture.env.clone();
    env.insert("ANDROID_HOME".into(), "/missing-sdk".into());
    assert!(execute(&fixture.root, Action::Doctor, &env, &fixture).is_err());
    let tools = Tools::find(&fixture.root, &fixture.env).unwrap();
    for serial in ["", "phone-123", "emulator-", "emulator-5560 extra"] {
        assert!(Device::take(&fixture.root, serial, &tools, &fixture.env, &fixture).is_err());
    }
}

#[test]
fn relay_pairing_reuses_fixture_auth_and_only_reverses_the_selected_emulator_port() {
    let fixture = Fixture::new();
    fixture.server(
        "http://127.0.0.1:41733",
        "http://127.0.0.1:41733/__preview/demo",
    );
    let tools = Tools::find(&fixture.root, &fixture.env).unwrap();
    let device = Device::take(
        &fixture.root,
        "emulator-5560",
        &tools,
        &fixture.env,
        &fixture,
    )
    .unwrap();
    let paired = relay::pair(&fixture.root, &device, "Summit", &tools, &fixture).unwrap();
    assert_eq!(
        paired["link"],
        "dispatch-phone:pair?site=http://sum.localhost:41733&code=ABCD-1234"
    );
    assert_eq!(paired["expiresAt"], 123456);
    assert_eq!(
        fixture
            .calls
            .borrow()
            .iter()
            .filter(|args| args[0] == "curl" && args.last().unwrap().ends_with("/api/session"))
            .count(),
        2
    );
    fixture.failure.set("pairing");
    assert!(
        relay::pair(&fixture.root, &device, "Summit", &tools, &fixture)
            .unwrap_err()
            .to_string()
            .contains("phones_off")
    );
    assert!(fixture.installations().is_empty());
}

#[test]
fn gradle_output_reaches_the_bounded_runner_before_the_build_finishes() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::{Command, Stdio},
        time::Duration,
    };
    struct Streaming {
        observed: Cell<bool>,
    }
    impl Runner for Streaming {
        fn command(&self, args: &[&str], cwd: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            let (read, write) = UnixStream::pair()?;
            read.set_read_timeout(Some(Duration::from_secs(15)))?;
            let mut child = Command::new(args[0])
                .args(&args[1..])
                .current_dir(cwd.unwrap())
                .stdin(Stdio::piped())
                .stdout(Stdio::from(OwnedFd::from(write)))
                .stderr(Stdio::null())
                .spawn()?;
            let mut reader = BufReader::new(read);
            let mut first = String::new();
            self.observed
                .set(reader.read_line(&mut first).is_ok() && first == "w: compiler output\n");
            // Always release the finite stand-in, including when the old buffered path times out.
            let released = child.stdin.take().unwrap().write_all(b"continue\n");
            let mut output = first.into_bytes();
            let read = reader.read_to_end(&mut output);
            let status = child.wait()?;
            released?;
            read?;
            require(status.success(), "Stand-in Gradle failed")?;
            Ok(output)
        }
    }
    let fixture = Fixture::new();
    let tools = fixture.gradlew(
        "printf 'w: compiler output\\n' >&2\nIFS= read -r proceed\nprintf 'build complete\\n'",
    );
    let runner = Streaming {
        observed: Cell::new(false),
    };
    let output = tools.gradle(&fixture.root, &["report"], &runner).unwrap();
    assert!(
        runner.observed.get(),
        "Gradle output was hidden until after the build finished"
    );
    assert_eq!(output, b"w: compiler output\nbuild complete\n");
}

#[test]
fn failed_gradle_invocations_keep_separate_private_diagnostics_and_propagate_failure() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let tools = fixture.gradlew("printf '%s\\n' \"$*\" >&2\nexit 17");
    let failed = |task| {
        let error = tools
            .gradle(&fixture.root, &[task], &crate::Native)
            .unwrap_err()
            .to_string();
        PathBuf::from(error.rsplit_once("\nGradle log: ").unwrap().1)
    };
    let first = failed("first-task");
    let second = failed("second-task");
    assert_ne!(
        first, second,
        "The second invocation replaced the first log"
    );
    assert!(fs::read_to_string(&first).unwrap().contains("first-task"));
    assert!(fs::read_to_string(&second).unwrap().contains("second-task"));
    assert_eq!(
        fs::metadata(first).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn pairing_refuses_nonlocal_servers_foreign_login_links_and_other_apps_before_any_api_request() {
    for (origin, link) in [
        (
            "http://example.com:41733",
            "http://example.com:41733/__preview/demo",
        ),
        (
            "http://127.0.0.1:41733",
            "https://admin.dspdispatch.com/__preview/demo",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.server(origin, link);
        let error = fixture
            .run(Action::Pair {
                app: "relay",
                device: "emulator-5560",
                dsp: "Summit",
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("local fixture server"), "{error}");
        assert!(!fixture.calls.borrow().iter().any(|args| args[0] == "curl"));
    }
    let fixture = Fixture::new();
    assert!(
        fixture
            .run(Action::Pair {
                app: "inventory",
                device: "emulator-5560",
                dsp: "Summit"
            })
            .unwrap_err()
            .to_string()
            .contains("Relay only")
    );
    assert!(fixture.calls.borrow().is_empty());
}
