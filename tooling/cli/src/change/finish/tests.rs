use super::*;
use crate::Native;
use std::sync::Mutex;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
    worktree: PathBuf,
    name: String,
    head: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let root = home.join("repo");
        let remote = home.join("origin");
        fs::create_dir(&root).unwrap();
        let name = format!(
            "finish-{}-{}",
            std::process::id(),
            home.file_name()
                .unwrap()
                .to_string_lossy()
                .replace('.', "")
                .to_lowercase()
        );
        let git = |args: &[&str]| {
            workspace::text(&Native, &[&["git"][..], args].concat(), Some(&root)).unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "base",
            "--allow-empty",
        ]);
        git(&["init", "-q", "--bare", remote.to_str().unwrap()]);
        git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        let worktree = home.join("worktrees").join(&name);
        git(&[
            "worktree",
            "add",
            "-q",
            worktree.to_str().unwrap(),
            "-b",
            &name,
        ]);
        git(&["push", "-q", "origin", &name]);
        let head = git(&["rev-parse", "HEAD"]);
        Self {
            _temp: temp,
            root,
            remote,
            worktree,
            name,
            head,
        }
    }
    fn git(&self, args: &[&str]) -> String {
        workspace::text(&Native, &[&["git"][..], args].concat(), Some(&self.root)).unwrap()
    }
    fn commit(&self) -> String {
        workspace::text(
            &Native,
            &[
                "git",
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-qm",
                "unshipped",
                "--allow-empty",
            ],
            Some(&self.worktree),
        )
        .unwrap();
        self.git(&["rev-parse", &self.name])
    }
    fn reviewer(&self, state: &str) -> Github {
        Github {
            pull: Pull {
                number: 1,
                state: state.into(),
                head_ref_oid: self.head.clone(),
                head_ref_name: self.name.clone(),
                is_cross_repository: false,
            },
            race: Mutex::new(None),
            local_race: Mutex::new(None),
            remote: self.remote.clone(),
        }
    }
}
struct Github {
    pull: Pull,
    race: Mutex<Option<(PathBuf, String, String)>>,
    local_race: Mutex<Option<(String, String)>>,
    remote: PathBuf,
}
impl Runner for Github {
    fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
        if args.first() == Some(&"gh") {
            return Ok(serde_json::to_vec(&[&self.pull])?);
        }
        if args.get(1..3) == Some(&["remote", "get-url"][..]) {
            return Ok(b"https://github.com/dispatch-systems/dispatch-platform.git\n".to_vec());
        }
        if args.get(1) == Some(&"push")
            && let Some((remote, reference, head)) = self.race.lock().unwrap().take()
        {
            Native.command(&["git", "update-ref", &reference, &head], Some(&remote), 30)?;
        }
        if args.get(1..3) == Some(&["update-ref", "-d"][..])
            && let Some((reference, head)) = self.local_race.lock().unwrap().take()
        {
            Native.command(&["git", "update-ref", &reference, &head], cwd, 30)?;
        }
        let args: Vec<_> = args
            .iter()
            .map(|arg| {
                if *arg == "https://github.com/dispatch-systems/dispatch-platform.git" {
                    self.remote.to_str().unwrap()
                } else {
                    *arg
                }
            })
            .collect();
        Native.command(&args, cwd, timeout)
    }
}

#[test]
fn cleanup_rejects_an_unrelated_origin_before_contacting_github() {
    struct NoGithub;
    impl Runner for NoGithub {
        fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
            assert_ne!(
                args.first(),
                Some(&"gh"),
                "an unrelated origin must be refused before a GitHub lookup"
            );
            Native.command(args, cwd, timeout)
        }
    }
    let fixture = Fixture::new();
    let error = finish_plan(&fixture.root, &fixture.name, false, &NoGithub)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Origin must name Dispatch"), "{error}");
    assert!(fixture.worktree.exists());
}

#[test]
fn conditional_local_ref_deletion_preserves_a_commit_added_during_cleanup() {
    let fixture = Fixture::new();
    let github = fixture.reviewer("MERGED");
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"]);
    let newer = fixture.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit-tree",
        &tree,
        "-p",
        &fixture.head,
        "-m",
        "local advanced",
    ]);
    *github.local_race.lock().unwrap() =
        Some((format!("refs/heads/{}", fixture.name), newer.clone()));
    assert!(finish(&fixture.root, &fixture.name, false, &github).is_err());
    assert_eq!(fixture.git(&["rev-parse", &fixture.name]), newer);
}

#[test]
fn merged_cleanup_preserves_new_commits_and_a_moved_remote() {
    let fixture = Fixture::new();
    let github = fixture.reviewer("MERGED");
    let newer = fixture.commit();
    let plan = finish_plan(&fixture.root, &fixture.name, false, &github).unwrap();
    assert!(!plan.can_finish());
    assert!(
        plan.blockers
            .iter()
            .any(|reason| reason.contains("unshipped work"))
    );
    assert!(finish(&fixture.root, &fixture.name, false, &github).is_err());
    assert!(fixture.worktree.is_dir());
    assert_eq!(fixture.git(&["rev-parse", &fixture.name]), newer);
    assert!(
        fixture
            .git(&["ls-remote", "origin", &fixture.name])
            .contains(&fixture.head)
    );
    fixture.git(&["push", "-q", "origin", &fixture.name]);
    fixture.git(&["reset", "--hard", &fixture.head]);
    fixture.git(&[
        "update-ref",
        &format!("refs/heads/{}", fixture.name),
        &fixture.head,
    ]);
    let plan = finish_plan(&fixture.root, &fixture.name, false, &github).unwrap();
    assert!(!plan.can_finish());
    assert!(
        plan.blockers
            .iter()
            .any(|reason| reason.starts_with("Origin"))
    );
}

#[test]
fn cleanup_plans_without_mutating_and_refuses_foreign_or_dirty_worktrees() {
    let fixture = Fixture::new();
    let github = fixture.reviewer("MERGED");
    let scratch = workspace::scratch(&fixture.name);
    assert!(!scratch.exists());
    let plan = finish_plan(&fixture.root, &fixture.name, false, &github).unwrap();
    assert!(plan.can_finish(), "{:?}", plan.blockers);
    assert!(!scratch.exists());
    assert!(fixture.worktree.is_dir());
    fs::write(fixture.worktree.join("untracked"), "keep").unwrap();
    let dirty = finish_plan(&fixture.root, &fixture.name, false, &github).unwrap();
    assert!(!dirty.can_finish());
    assert!(finish(&fixture.root, &fixture.name, false, &github).is_err());
    fs::remove_file(fixture.worktree.join("untracked")).unwrap();
    fixture.git(&[
        "-C",
        fixture.worktree.to_str().unwrap(),
        "switch",
        "-qc",
        "another-branch",
    ]);
    let foreign = finish_plan(&fixture.root, &fixture.name, false, &github).unwrap();
    assert!(
        foreign
            .blockers
            .iter()
            .any(|reason| reason.contains("registered worktree"))
    );
}

#[test]
fn cleanup_uses_a_remote_lease_and_allows_explicit_closed_abandonment() {
    let fixture = Fixture::new();
    let github = fixture.reviewer("MERGED");
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"]);
    let newer = fixture.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit-tree",
        &tree,
        "-p",
        &fixture.head,
        "-m",
        "remote advanced",
    ]);
    fixture.git(&[
        "push",
        "-q",
        "origin",
        &format!("{newer}:refs/heads/race-object"),
    ]);
    *github.race.lock().unwrap() = Some((
        fixture.remote.clone(),
        format!("refs/heads/{}", fixture.name),
        newer.clone(),
    ));
    assert!(finish(&fixture.root, &fixture.name, false, &github).is_err());
    assert!(fixture.worktree.is_dir());
    assert_eq!(fixture.git(&["rev-parse", &fixture.name]), fixture.head);
    assert!(
        fixture
            .git(&["ls-remote", "origin", &fixture.name])
            .contains(&newer)
    );
    assert!(
        finish(
            &fixture.root,
            &fixture.name,
            true,
            &fixture.reviewer("OPEN")
        )
        .is_err()
    );
    finish(
        &fixture.root,
        &fixture.name,
        true,
        &fixture.reviewer("CLOSED"),
    )
    .unwrap();
    assert!(!fixture.worktree.exists());
    assert!(fixture.git(&["branch", "--list", &fixture.name]).is_empty());
    assert!(
        fixture
            .git(&["ls-remote", "origin", &fixture.name])
            .is_empty()
    );
}
