use super::*;
use std::{os::unix::fs::PermissionsExt, process::Command};

#[test]
fn verifier_projection_survives_remapped_ownership_and_refuses_changed_boundaries() -> Result<()> {
    // The child reaches the real constructors in an actual remapped namespace. The
    // stand-in tools need no browser package or provider, and grant no network access.
    if let Ok(expected) = std::env::var("DISPATCH_TEST_PROJECTION_EXPECTED") {
        let config = crate::foundation::config::Config::load()?;
        assert_ne!(fs::metadata(&config.browseros)?.uid(), 0);
        let normal = Runtime::new(
            &config.browseros,
            &config.sandbox,
            &std::env::current_exe()?,
            Path::new("/state/runs"),
            1,
        )
        .err()
        .expect("normal platform authority must still reject remapped tools");
        assert_eq!(normal.code, "trusted_browser_executable_required");
        let result = VerificationAuthority::load(&config);
        if expected == "valid" {
            let runtime = result?.runtime(Path::new("/state/runs"), 1)?;
            runtime.verification.as_ref().unwrap().display()?;
            assert_eq!(runtime.browser, config.browseros);
            assert_eq!(runtime.sandbox, config.sandbox);
        } else {
            let refusal = result.err().expect("changed projection must be refused");
            assert_eq!(
                refusal.code,
                if expected == "unknown_field" {
                    "invalid_input"
                } else {
                    "support_browser_projection_invalid"
                },
                "{expected}"
            );
        }
        return Ok(());
    }

    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    for name in ["state", "authority", "artifact/tooling"] {
        fs::create_dir_all(root.join(name))?;
    }
    let source = "a".repeat(40);
    fs::write(
        root.join("artifact/tooling/build-info.json"),
        json!({"commit":source}).to_string(),
    )?;
    let test_binary = root.join("dispatch-tests");
    fs::copy(std::env::current_exe()?, &test_binary)?;
    // The installed artifact has private executable modes; raw Cargo output can be 0775.
    fs::set_permissions(&test_binary, fs::Permissions::from_mode(0o700))?;
    let tool = Path::new("/usr/bin/true");
    let sandbox = Path::new("/usr/bin/bwrap");
    assert_eq!(fs::metadata(tool)?.uid(), 0);
    let digest = |path: &Path| -> Result<String> { Ok(crypto::sha(fs::read(path)?)) };
    let namespaces: serde_json::Map<String, Value> = ["pid", "net", "mnt", "ipc", "uts", "user"]
        .into_iter()
        .map(|name| {
            Ok((
                name.into(),
                json!(fs::read_link(format!("/proc/self/ns/{name}"))?),
            ))
        })
        .collect::<Result<_>>()?;
    let valid = json!({
        "schemaVersion":1,"sourceSha":source,"backendSha256":digest(&test_binary)?,
        "hostNamespaces":namespaces,
        "executables":{
            "browser":{"path":"/opt/dispatch-browseros/0.50.5/browseros","sha256":digest(tool)?},
            "sandbox":{"path":"/usr/bin/bwrap","sha256":digest(sandbox)?},
            "display":{"path":"/usr/bin/Xvfb","sha256":digest(tool)?}
        }
    });
    let test = format!(
        "{}::verifier_projection_survives_remapped_ownership_and_refuses_changed_boundaries",
        module_path!().split_once("::").unwrap().1
    );
    let receipt = root.join("authority/browser-authority.json");
    for expected in ["valid", "digest", "writable", "namespace", "unknown_field"] {
        let mut authority = valid.clone();
        if expected == "digest" {
            authority["executables"]["browser"]["sha256"] = json!("0".repeat(64));
        } else if expected == "unknown_field" {
            authority["unrecognizedAuthority"] = json!(true);
        }
        if receipt.exists() {
            fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600))?;
        }
        fs::write(&receipt, serde_json::to_vec(&authority)?)?;
        fs::set_permissions(&receipt, fs::Permissions::from_mode(0o400))?;
        let mut command = Command::new(sandbox);
        command.args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--clearenv",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--tmpfs",
            "/usr/bin",
        ]);
        if Path::new("/usr/lib64").exists() {
            command.args(["--symlink", "usr/lib64", "/lib64"]);
        }
        if expected == "namespace" {
            command.arg("--share-net");
        }
        command
            .arg(if expected == "writable" {
                "--bind"
            } else {
                "--ro-bind"
            })
            .arg(sandbox)
            .arg("/usr/bin/bwrap")
            .arg("--ro-bind")
            .arg(tool)
            .arg("/usr/bin/Xvfb")
            .arg("--ro-bind")
            .arg(tool)
            .arg("/opt/dispatch-browseros/0.50.5/browseros")
            .arg("--ro-bind")
            .arg(&test_binary)
            .arg("/runtime/dispatch-tests")
            .arg("--ro-bind")
            .arg(root.join("authority"))
            .arg("/run/support-provider")
            .arg("--ro-bind")
            .arg(root.join("artifact"))
            .arg("/artifact")
            .arg("--bind")
            .arg(root.join("state"))
            .arg("/state")
            .args([
                "--setenv",
                "NODE_ENV",
                "development",
                "--setenv",
                "DISPATCH_ENVIRONMENT",
                "preview",
                "--setenv",
                "DISPATCH_PROVIDER_MODE",
                "native",
                "--setenv",
                "DISPATCH_STATE_ROOT",
                "/state",
                "--setenv",
                "DISPATCH_ARTIFACT_ROOT",
                "/artifact",
                "--setenv",
                "DISPATCH_TEST_PROJECTION_EXPECTED",
                expected,
                "--chdir",
                "/state",
                "/runtime/dispatch-tests",
                "--exact",
            ])
            .arg(&test)
            .arg("--nocapture");
        let result = command.output()?;
        assert!(
            result.status.success(),
            "{expected}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("1 passed"),
            "the actual Runtime test must execute in the namespace"
        );
    }
    Ok(())
}
