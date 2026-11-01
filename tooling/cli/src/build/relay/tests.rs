use super::*;
use std::cell::RefCell;

/// The format as the MacBook's publish writes it: a release of two builds, whose APKs are the
/// bytes `APKS` serves.
const FIXTURE: &str = include_str!("release.json");
/// What each asset's download answers.
const APKS: [(&str, &[u8]); 2] = [
    ("dsp-dispatch-relay-dev-1.4.0.apk", b"dev apk"),
    ("dsp-dispatch-relay-1.4.0.apk", b"production apk"),
];

/// A checkout whose Git folder is its own, and the downloads it asks for.
struct Checkout {
    _temp: tempfile::TempDir,
    root: PathBuf,
    /// Each URL fetched, in order.
    fetched: RefCell<Vec<String>>,
    /// What a download answers in place of an asset's own bytes.
    tampered: Option<&'static [u8]>,
}
impl Checkout {
    fn new(release: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("android/relay")).unwrap();
        if let Some(release) = release {
            fs::write(root.join(RELEASE), release).unwrap();
        }
        Self {
            _temp: temp,
            root,
            fetched: RefCell::new(Vec::new()),
            tampered: None,
        }
    }
    fn packed(&self, name: &str) -> Vec<u8> {
        fs::read(self.root.join(PACKED).join(name)).unwrap()
    }
    fn cached(&self) -> usize {
        fs::read_dir(self.root.join(".git").join(CACHE)).map_or(0, |entries| entries.count())
    }
}
impl Runner for Checkout {
    fn command(&self, args: &[&str], _cwd: Option<&Path>, _timeout: u64) -> Result<Vec<u8>> {
        match args[0] {
            "git" => Ok(self.root.join(".git").to_str().unwrap().as_bytes().to_vec()),
            "curl" => {
                let url = *args.last().unwrap();
                let output = args[args.iter().position(|a| *a == "--output").unwrap() + 1];
                let (_, bytes) = APKS
                    .iter()
                    .find(|(asset, _)| url.ends_with(&format!("/relay-v1.4.0/{asset}")))
                    .expect("an asset of the release's pre-release");
                fs::write(output, self.tampered.unwrap_or(bytes))?;
                self.fetched.borrow_mut().push(url.to_owned());
                Ok(vec![])
            }
            other => panic!("Unexpected command {other}"),
        }
    }
}

#[test]
fn a_release_is_read_as_its_format_has_it_and_refused_otherwise() {
    let release = Release::parse(FIXTURE).unwrap();
    assert_eq!(
        (release.version.as_str(), release.version_code),
        ("1.4.0", 14)
    );
    assert_eq!(release.builds.production.size, 14);
    assert_eq!(
        release.url(&release.builds.dev),
        format!(
            "https://github.com/{REPOSITORY}/releases/download/relay-v1.4.0/dsp-dispatch-relay-dev-1.4.0.apk"
        )
    );
    for (from, to) in [
        ("\"1.4.0\"", "\"1.4\""),
        ("\"versionCode\": 14", "\"versionCode\": 0"),
        (
            "dsp-dispatch-relay-1.4.0.apk",
            "../dsp-dispatch-relay-1.4.0.apk",
        ),
        (
            "dsp-dispatch-relay-1.4.0.apk",
            "dsp-dispatch-relay-1.4.0.zip",
        ),
        (
            "dsp-dispatch-relay-1.4.0.apk",
            "dsp-dispatch-relay-dev-1.4.0.apk",
        ),
        ("5b48232ef3b9", "5B48232EF3B9"),
        ("\"size\": 14", "\"size\": 0"),
        (
            "\"versionCode\"",
            "\"tag\": \"relay-v1.4.0\", \"versionCode\"",
        ),
    ] {
        assert!(FIXTURE.contains(from), "{from}");
        assert!(
            Release::parse(&FIXTURE.replacen(from, to, 1)).is_err(),
            "{from} as {to}"
        );
    }
}

#[test]
fn each_apk_is_fetched_once_checked_and_packed_with_its_release() {
    let checkout = Checkout::new(Some(FIXTURE));
    pack(&checkout.root, &checkout).unwrap();
    assert_eq!(checkout.fetched.borrow().len(), 2);
    for (asset, bytes) in APKS {
        assert_eq!(checkout.packed(asset), bytes);
    }
    assert_eq!(checkout.packed("release.json"), FIXTURE.as_bytes());

    // Packed already, it's left as it is; packed again, the cache's copies serve.
    pack(&checkout.root, &checkout).unwrap();
    fs::remove_dir_all(checkout.root.join(PACKED)).unwrap();
    pack(&checkout.root, &checkout).unwrap();
    assert_eq!(checkout.fetched.borrow().len(), 2);
    assert_eq!(checkout.packed(APKS[1].0), APKS[1].1);
}

#[test]
fn an_apk_unlike_its_release_is_refused_and_nothing_packed() {
    let mut checkout = Checkout::new(Some(FIXTURE));
    checkout.tampered = Some(b"other apk");
    assert!(pack(&checkout.root, &checkout).is_err());
    assert!(!checkout.root.join(PACKED).exists());
    assert_eq!(checkout.cached(), 0);
}

#[test]
fn without_a_release_no_app_is_packed() {
    let checkout = Checkout::new(Some(FIXTURE));
    pack(&checkout.root, &checkout).unwrap();
    fs::remove_file(checkout.root.join(RELEASE)).unwrap();
    pack(&checkout.root, &checkout).unwrap();
    assert!(!checkout.root.join(PACKED).exists());
}
