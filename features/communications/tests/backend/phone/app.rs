use super::*;

/// A runtime's dashboard folder with the app packed in its `relay/`, as the build packs it.
struct Runtime {
    dashboard: tempfile::TempDir,
}
impl Runtime {
    fn new() -> Self {
        let dashboard = tempfile::tempdir().unwrap();
        let runtime = Self { dashboard };
        std::fs::create_dir(runtime.folder()).unwrap();
        runtime.release("relay.apk");
        std::fs::write(runtime.folder().join("relay-dev.apk"), b"dev apk").unwrap();
        std::fs::write(runtime.folder().join("relay.apk"), b"production apk").unwrap();
        runtime
    }
    fn folder(&self) -> PathBuf {
        self.dashboard.path().join(FOLDER)
    }
    /// Writes the release, Production's build named `production`.
    fn release(&self, production: &str) {
        let release = serde_json::json!({
            "version": "1.4.0",
            "versionCode": 14,
            "builds": {
                "dev": { "asset": "relay-dev.apk", "sha256": "d", "size": 7 },
                "production": { "asset": production, "sha256": "p", "size": 14 },
            },
        });
        std::fs::write(self.folder().join(RELEASE), release.to_string()).unwrap();
    }
}

#[test]
fn each_server_hands_out_its_own_build() {
    let runtime = Runtime::new();
    let dev = packed(&runtime.folder(), Environment::Preview).unwrap();
    assert_eq!(
        (dev.name, dev.version.as_str(), dev.version_code),
        ("DSP Dispatch: Relay Dev", "1.4.0", 14)
    );
    assert_eq!((dev.sha256.as_str(), dev.size), ("d", 7));
    assert_eq!(dev.file, "relay-dev.apk");
    assert_eq!(std::fs::read(dev.path).unwrap(), b"dev apk");
    let production = packed(&runtime.folder(), Environment::Production).unwrap();
    assert_eq!(production.name, "DSP Dispatch: Relay");
    assert_eq!(std::fs::read(production.path).unwrap(), b"production apk");
}

#[test]
fn no_app_is_offered_unless_its_build_packed_it_whole() {
    let empty = tempfile::tempdir().unwrap();
    assert!(packed(&empty.path().join(FOLDER), Environment::Preview).is_none());

    let cut = Runtime::new();
    std::fs::write(cut.folder().join("relay-dev.apk"), b"dev").unwrap();
    assert!(packed(&cut.folder(), Environment::Preview).is_none());
    assert!(packed(&cut.folder(), Environment::Production).is_some());

    let missing = Runtime::new();
    std::fs::remove_file(missing.folder().join("relay.apk")).unwrap();
    assert!(packed(&missing.folder(), Environment::Production).is_none());

    // An APK the release names outside the folder, there whole, is never handed out.
    let outside = Runtime::new();
    std::fs::write(
        outside.dashboard.path().join("relay.apk"),
        b"production apk",
    )
    .unwrap();
    outside.release("../relay.apk");
    assert!(packed(&outside.folder(), Environment::Production).is_none());
}
