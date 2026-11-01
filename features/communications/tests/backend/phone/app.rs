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
        runtime.builds("relay.apk");
        std::fs::write(runtime.folder().join("relay-dev.apk"), b"dev apk").unwrap();
        std::fs::write(runtime.folder().join("relay.apk"), b"production apk").unwrap();
        runtime
    }
    fn folder(&self) -> PathBuf {
        self.dashboard.path().join(FOLDER)
    }
    /// Writes the builds, Production's APK named `production`.
    fn builds(&self, production: &str) {
        let builds = serde_json::json!({
            "dev": {
                "version": "1.4.1-dev.812", "versionCode": 812, "commit": "2".repeat(40),
                "asset": "relay-dev.apk", "sha256": "d", "size": 7,
            },
            "production": {
                "version": "1.4.0", "versionCode": 790, "commit": "1".repeat(40),
                "asset": production, "sha256": "p", "size": 14,
            },
        });
        std::fs::write(self.folder().join(BUILDS), builds.to_string()).unwrap();
    }
}

#[test]
fn each_server_hands_out_its_own_build() {
    let runtime = Runtime::new();
    let dev = packed(&runtime.folder(), Environment::Preview).unwrap();
    assert_eq!(
        (dev.name, dev.version.as_str(), dev.version_code),
        ("DSP Dispatch: Relay Dev", "1.4.1-dev.812", 812)
    );
    assert_eq!((dev.sha256.as_str(), dev.size), ("d", 7));
    assert_eq!(dev.file, "relay-dev.apk");
    assert_eq!(std::fs::read(dev.path).unwrap(), b"dev apk");
    let production = packed(&runtime.folder(), Environment::Production).unwrap();
    assert_eq!(
        (
            production.name,
            production.version.as_str(),
            production.version_code
        ),
        ("DSP Dispatch: Relay", "1.4.0", 790)
    );
    assert_eq!(std::fs::read(production.path).unwrap(), b"production apk");
}

#[test]
fn production_hands_out_none_from_a_package_without_its_build() {
    // As main's packages are: Dev's build alone.
    let runtime = Runtime::new();
    let builds = std::fs::read_to_string(runtime.folder().join(BUILDS)).unwrap();
    let mut builds: serde_json::Value = serde_json::from_str(&builds).unwrap();
    builds.as_object_mut().unwrap().remove("production");
    std::fs::write(runtime.folder().join(BUILDS), builds.to_string()).unwrap();
    assert!(packed(&runtime.folder(), Environment::Production).is_none());
    assert!(packed(&runtime.folder(), Environment::Preview).is_some());
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

    // An APK its entry names outside the folder, there whole, is never handed out.
    let outside = Runtime::new();
    std::fs::write(
        outside.dashboard.path().join("relay.apk"),
        b"production apk",
    )
    .unwrap();
    outside.builds("../relay.apk");
    assert!(packed(&outside.folder(), Environment::Production).is_none());
}
