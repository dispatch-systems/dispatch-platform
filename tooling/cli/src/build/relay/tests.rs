use super::*;
use std::path::PathBuf;

/// The file as the build packs it, from the APKs `APKS` holds: a release's package, with both
/// builds.
const FIXTURE: &str = include_str!("relay.json");
/// Each build's APK, as CI made them.
const APKS: [(&str, &[u8]); 2] = [("dev", b"dev apk"), ("production", b"production apk")];

/// A checkout, and the folder of builds CI made for it.
struct Checkout {
    _temp: tempfile::TempDir,
    root: PathBuf,
    apps: PathBuf,
}
impl Checkout {
    /// With `names`' builds made, as the fixture names them.
    fn new(names: &[&str]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let apps = temp.path().join("apps");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&apps).unwrap();
        let checkout = Self {
            _temp: temp,
            root,
            apps,
        };
        for name in names {
            checkout.made(name, &entry(name).to_string());
        }
        checkout
    }
    /// CI's build `name`, its entry `text`, beside its APK.
    fn made(&self, name: &str, text: &str) {
        let (_, bytes) = APKS.iter().find(|(n, _)| *n == name).unwrap();
        let asset = serde_json::from_str::<serde_json::Value>(text).unwrap()["asset"]
            .as_str()
            .unwrap()
            .to_owned();
        fs::write(self.apps.join(asset), bytes).unwrap();
        fs::write(self.apps.join(format!("{name}.json")), text).unwrap();
    }
    fn pack(&self) -> Result<()> {
        pack(&self.root, Some(&self.apps))
    }
    fn packed(&self, name: &str) -> Vec<u8> {
        fs::read(self.root.join(PACKED).join(name)).unwrap()
    }
    fn is_packed(&self) -> bool {
        self.root.join(PACKED).exists()
    }
}
/// Build `name`'s entry, as the fixture has it.
fn entry(name: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap()[name].clone()
}

#[test]
fn each_build_is_checked_and_packed_with_its_entry() {
    let checkout = Checkout::new(&["dev", "production"]);
    checkout.pack().unwrap();
    assert_eq!(String::from_utf8(checkout.packed(FILE)).unwrap(), FIXTURE);
    assert_eq!(
        checkout.packed("dsp-dispatch-relay-1.4.1-dev.812.apk"),
        APKS[0].1
    );
    assert_eq!(checkout.packed("dsp-dispatch-relay-1.4.0.apk"), APKS[1].1);
    assert_eq!(fs::read_dir(checkout.root.join(PACKED)).unwrap().count(), 3);
}

#[test]
fn main_packs_the_dev_build_alone() {
    let checkout = Checkout::new(&["dev"]);
    checkout.pack().unwrap();
    let packed: serde_json::Value = serde_json::from_slice(&checkout.packed(FILE)).unwrap();
    assert_eq!(packed, serde_json::json!({ "dev": entry("dev") }));
}

#[test]
fn without_the_builds_no_app_is_packed() {
    let checkout = Checkout::new(&["dev"]);
    checkout.pack().unwrap();
    pack(&checkout.root, None).unwrap();
    assert!(!checkout.is_packed());

    // Nor from a folder that holds none.
    fs::remove_file(checkout.apps.join("dev.json")).unwrap();
    assert!(checkout.pack().is_err());
    assert!(!checkout.is_packed());
}

#[test]
fn an_apk_unlike_its_entry_is_refused_and_nothing_packed() {
    let checkout = Checkout::new(&["dev", "production"]);
    checkout.pack().unwrap();
    fs::write(
        checkout.apps.join("dsp-dispatch-relay-1.4.0.apk"),
        b"other apk",
    )
    .unwrap();
    assert!(checkout.pack().is_err());
    assert!(!checkout.is_packed());

    // An APK by a symbolic link is none.
    let checkout = Checkout::new(&["dev"]);
    let apk = checkout.apps.join("dsp-dispatch-relay-1.4.1-dev.812.apk");
    fs::rename(&apk, checkout.apps.join("elsewhere.apk")).unwrap();
    std::os::unix::fs::symlink("elsewhere.apk", &apk).unwrap();
    assert!(checkout.pack().is_err());
    assert!(!checkout.is_packed());
}

#[test]
fn an_entry_is_read_as_its_format_has_it_and_refused_otherwise() {
    for name in ["dev", "production"] {
        let text = entry(name).to_string();
        assert!(Build::parse(name, &text).is_ok(), "{name}");
    }
    let dev = entry("dev").to_string();
    let production = entry("production").to_string();
    for (name, text, from, to) in [
        ("production", &production, "\"1.4.0\"", "\"1.4\""),
        ("production", &production, "\"1.4.0\"", "\"1.04.0\""),
        ("production", &production, "\"1.4.0\"", "\"1.4.0-dev.790\""),
        ("dev", &dev, "\"1.4.1-dev.812\"", "\"1.4.1\""),
        ("dev", &dev, "\"1.4.1-dev.812\"", "\"1.4.1-dev.811\""),
        ("dev", &dev, "\"versionCode\":812", "\"versionCode\":0"),
        (
            "production",
            &production,
            "\"versionCode\":790",
            "\"versionCode\":2100000001",
        ),
        ("production", &production, "11111111", "1111111G"),
        (
            "production",
            &production,
            "dsp-dispatch-relay-1.4.0.apk",
            "../relay.apk",
        ),
        (
            "production",
            &production,
            "dsp-dispatch-relay-1.4.0.apk",
            "relay.zip",
        ),
        ("production", &production, "b6585ed1f8a6", "B6585ED1F8A6"),
        ("production", &production, "\"size\":14", "\"size\":0"),
        (
            "production",
            &production,
            "\"size\":14",
            "\"size\":14,\"tag\":\"v1\"",
        ),
    ] {
        assert!(text.contains(from), "{from}");
        assert!(
            Build::parse(name, &text.replacen(from, to, 1)).is_err(),
            "{name}: {from} as {to}"
        );
    }

    // Each build has an APK of its own.
    let checkout = Checkout::new(&["dev"]);
    let mut same = entry("production");
    same["asset"] = entry("dev")["asset"].clone();
    same["sha256"] = entry("dev")["sha256"].clone();
    same["size"] = entry("dev")["size"].clone();
    fs::write(checkout.apps.join("production.json"), same.to_string()).unwrap();
    assert!(checkout.pack().is_err());
}

#[test]
fn builds_in_one_checkout_pack_it_one_at_a_time() {
    let checkout = Checkout::new(&["dev"]);
    // While another build of the checkout holds its lock, this one waits its turn, then packs.
    let held = packing(&checkout.root.join(PACKED)).unwrap();
    let (root, apps) = (checkout.root.clone(), checkout.apps.clone());
    let waiting = std::thread::spawn(move || pack(&root, Some(&apps)).is_ok());
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!waiting.is_finished());
    assert!(!checkout.is_packed());
    drop(held);
    assert!(waiting.join().unwrap());
    assert_eq!(
        checkout.packed("dsp-dispatch-relay-1.4.1-dev.812.apk"),
        APKS[0].1
    );
}
