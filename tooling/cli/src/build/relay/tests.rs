use super::*;
use sha2::Digest;
use std::cell::RefCell;

/// The format as the MacBook's publish writes it: a release of two builds, whose APKs are the
/// bytes `APKS` serves.
const FIXTURE: &str = include_str!("release.json");
/// What each asset's download answers, of the fixture's version.
const APKS: [(&str, &[u8]); 2] = [
    ("dsp-dispatch-relay-dev-1.4.0.apk", b"dev apk"),
    ("dsp-dispatch-relay-1.4.0.apk", b"production apk"),
];
/// The next version's, which `next` names.
const NEXT: [(&str, &[u8]); 2] = [
    ("dsp-dispatch-relay-dev-1.5.0.apk", b"dev apk 1.5.0"),
    ("dsp-dispatch-relay-1.5.0.apk", b"production apk 1.5.0"),
];

/// The release of the next version, 1.5.0, whose APKs are `NEXT`'s.
fn next() -> String {
    let apk = |(asset, bytes): (&str, &[u8])| {
        let sha256 = crate::to_hex(&sha2::Sha256::digest(bytes));
        format!(
            r#"{{"asset": "{asset}", "sha256": "{sha256}", "size": {}}}"#,
            bytes.len()
        )
    };
    format!(
        r#"{{"version": "1.5.0", "versionCode": 15, "builds": {{"dev": {}, "production": {}}}}}"#,
        apk(NEXT[0]),
        apk(NEXT[1])
    )
}

/// A checkout, its Git folder its own or another's, and the downloads it asks for.
struct Checkout {
    _temp: tempfile::TempDir,
    root: PathBuf,
    /// The repository's shared Git folder.
    git: PathBuf,
    /// Each URL fetched, in order.
    fetched: RefCell<Vec<String>>,
    /// What a download answers in place of an asset's own bytes.
    tampered: Option<&'static [u8]>,
    /// Whether every download fails, as offline.
    offline: bool,
    /// Whether every download is over the most a release may name, as curl refuses it.
    oversized: bool,
    /// What happens while the second APK downloads, once.
    meanwhile: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl Checkout {
    fn new(release: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let git = temp.path().join("repo/.git");
        fs::create_dir_all(&git).unwrap();
        Self::at(temp, git, release)
    }
    /// A worktree of `other`'s repository, sharing its Git folder.
    fn beside(other: &Checkout, release: &str) -> Self {
        Self::at(
            tempfile::tempdir().unwrap(),
            other.git.clone(),
            Some(release),
        )
    }
    fn at(temp: tempfile::TempDir, git: PathBuf, release: Option<&str>) -> Self {
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join("android/relay")).unwrap();
        if let Some(release) = release {
            fs::write(root.join(RELEASE), release).unwrap();
        }
        Self {
            _temp: temp,
            root,
            git,
            fetched: RefCell::new(Vec::new()),
            tampered: None,
            offline: false,
            oversized: false,
            meanwhile: RefCell::new(None),
        }
    }
    fn packed(&self, name: &str) -> Vec<u8> {
        fs::read(self.root.join(PACKED).join(name)).unwrap()
    }
    fn cache(&self) -> PathBuf {
        self.git.join(CACHE)
    }
    fn cached(&self) -> usize {
        fs::read_dir(self.cache()).map_or(0, |entries| entries.count())
    }
    fn release(&self, text: &str) {
        fs::write(self.root.join(RELEASE), text).unwrap();
    }
}
impl Runner for Checkout {
    fn command(&self, args: &[&str], _cwd: Option<&Path>, _timeout: u64) -> Result<Vec<u8>> {
        match args[0] {
            "git" => Ok(self.git.to_str().unwrap().as_bytes().to_vec()),
            "curl" if self.offline => Err("Could not resolve host: github.com".into()),
            "curl" if self.oversized => {
                Err("curl --fail --silent failed: curl: (63) Maximum file size exceeded".into())
            }
            "curl" => {
                let url = *args.last().unwrap();
                let output = args[args.iter().position(|a| *a == "--output").unwrap() + 1];
                let (_, bytes) = APKS
                    .iter()
                    .map(|apk| ("1.4.0", apk))
                    .chain(NEXT.iter().map(|apk| ("1.5.0", apk)))
                    .find(|(version, (asset, _))| {
                        url.ends_with(&format!("/relay-v{version}/{asset}"))
                    })
                    .map(|(_, apk)| *apk)
                    .expect("an asset of the release's pre-release");
                fs::write(output, self.tampered.unwrap_or(bytes))?;
                let count = {
                    let mut fetched = self.fetched.borrow_mut();
                    fetched.push(url.to_owned());
                    fetched.len()
                };
                if count % 2 == 0
                    && let Some(meanwhile) = self.meanwhile.borrow_mut().take()
                {
                    meanwhile();
                }
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
    pack(&checkout.root, &checkout, true).unwrap();
    assert_eq!(checkout.fetched.borrow().len(), 2);
    for (asset, bytes) in APKS {
        assert_eq!(checkout.packed(asset), bytes);
    }
    assert_eq!(checkout.packed("release.json"), FIXTURE.as_bytes());

    // Packed already, it's left as it is; packed again, the cache's copies serve.
    pack(&checkout.root, &checkout, true).unwrap();
    fs::remove_dir_all(checkout.root.join(PACKED)).unwrap();
    pack(&checkout.root, &checkout, true).unwrap();
    assert_eq!(checkout.fetched.borrow().len(), 2);
    assert_eq!(checkout.packed(APKS[1].0), APKS[1].1);
}

#[test]
fn an_apk_unlike_its_release_is_refused_and_nothing_packed() {
    let mut checkout = Checkout::new(Some(FIXTURE));
    checkout.tampered = Some(b"other apk");
    // A debug build too.
    assert!(pack(&checkout.root, &checkout, false).is_err());
    assert!(!checkout.root.join(PACKED).exists());
    assert_eq!(checkout.cached(), 0);
}

#[test]
fn without_a_release_no_app_is_packed() {
    let checkout = Checkout::new(Some(FIXTURE));
    pack(&checkout.root, &checkout, true).unwrap();
    fs::remove_file(checkout.root.join(RELEASE)).unwrap();
    pack(&checkout.root, &checkout, true).unwrap();
    assert!(!checkout.root.join(PACKED).exists());
}

#[test]
fn a_debug_build_that_cant_download_goes_on_without_the_app_and_a_release_build_fails() {
    let mut checkout = Checkout::new(Some(FIXTURE));
    pack(&checkout.root, &checkout, false).unwrap();
    checkout.offline = true;
    checkout.release(&next());
    assert!(pack(&checkout.root, &checkout, true).is_err());
    pack(&checkout.root, &checkout, false).unwrap();
    assert!(!checkout.root.join(PACKED).exists());

    // A release unlike the format still fails it.
    checkout.release(&next().replace("1.5.0", "1.5"));
    assert!(pack(&checkout.root, &checkout, false).is_err());

    // And so does an APK larger than any release may name, which curl refuses to download.
    checkout.offline = false;
    checkout.oversized = true;
    checkout.release(&next());
    let refused = pack(&checkout.root, &checkout, false).unwrap_err();
    assert!(refused.to_string().contains("is larger than"), "{refused}");
}

#[test]
fn a_kept_apk_is_reused_only_with_the_size_its_release_names() {
    let checkout = Checkout::new(Some(FIXTURE));
    pack(&checkout.root, &checkout, false).unwrap();
    let resized = FIXTURE.replacen("\"size\": 14", "\"size\": 15", 1);
    assert_ne!(resized, FIXTURE);
    checkout.release(&resized);

    // Not the folder packed with it, nor the cache: the download is 14 bytes, and refused.
    fs::write(checkout.root.join(PACKED).join("release.json"), &resized).unwrap();
    assert!(pack(&checkout.root, &checkout, false).is_err());
    fs::remove_dir_all(checkout.root.join(PACKED)).unwrap();
    assert!(pack(&checkout.root, &checkout, false).is_err());
}

#[test]
fn builds_in_worktrees_share_the_cache_without_failing_each_other() {
    // While one worktree's build downloads its second APK, another's packs the next version and
    // prunes the cache, with two APKs used later than the first's: the first's, held, stays.
    let first = Checkout::new(Some(FIXTURE));
    store::directory(&first.cache()).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    for name in ["0", "1"] {
        fs::File::create(first.cache().join(format!("{}.apk", name.repeat(64))))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(later))
            .unwrap();
    }
    let second = Checkout::beside(&first, &next());
    let dev = entry(&first.cache(), &Release::parse(FIXTURE).unwrap().builds.dev);
    *first.meanwhile.borrow_mut() = Some(Box::new(move || {
        pack(&second.root, &second, true).unwrap();
        assert_eq!(second.packed(NEXT[1].0), NEXT[1].1);
        assert!(dev.exists(), "the first build's APK, held, was pruned");
    }));
    pack(&first.root, &first, true).unwrap();
    for (asset, bytes) in APKS {
        assert_eq!(first.packed(asset), bytes);
    }

    // An APK removed while a build holds it, as by a build that took no lock, still packs.
    let checkout = Checkout::new(Some(FIXTURE));
    let cache = checkout.cache();
    *checkout.meanwhile.borrow_mut() = Some(Box::new(move || {
        for found in fs::read_dir(&cache).unwrap() {
            let path = found.unwrap().path();
            if path.extension().is_some_and(|e| e == "apk") {
                fs::remove_file(path).unwrap();
            }
        }
    }));
    pack(&checkout.root, &checkout, true).unwrap();
    for (asset, bytes) in APKS {
        assert_eq!(checkout.packed(asset), bytes);
    }
}
