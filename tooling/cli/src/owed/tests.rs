use super::*;

#[test]
fn a_pr_no_longer_open_is_landed_then_finished_or_abandoned() {
    let landed = |result| Landed {
        commit: "m1".to_owned(),
        run: 5,
        result,
    };
    let settled = |repository, state, landing: Option<&Landed>| {
        settled("change", repository, state, Some("m1"), landing)
    };
    let finish = "From outside the worktree, `dispatchdev finish change`";
    let platform = Repository::Platform;
    assert_eq!(
        settled(platform, PrState::Closed, None),
        "From outside the worktree, `dispatchdev finish change --abandon`"
    );
    assert!(settled(platform, PrState::Merged, None).starts_with("`dispatchdev landed`"));
    assert_eq!(
        settled(platform, PrState::Merged, Some(&landed(Outcome::Passed))),
        finish
    );
    assert!(
        settled(platform, PrState::Merged, Some(&landed(Outcome::Failed)))
            .contains("`gh run rerun 5 --failed`")
    );
    // What `landed` left for an earlier merge of the branch doesn't stand for this one.
    let earlier = Landed {
        commit: "m0".to_owned(),
        ..landed(Outcome::Passed)
    };
    assert!(settled(platform, PrState::Merged, Some(&earlier)).starts_with("`dispatchdev landed`"));
    // Nothing runs on the agent docs' main.
    assert_eq!(
        settled(Repository::AgentDocs, PrState::Merged, None),
        finish
    );
}

/// The hook reads this shape.
#[test]
fn an_entry_is_written_as_the_hook_reads_it() {
    let owed = Owed {
        name: "docs".to_owned(),
        path: PathBuf::from("/w/docs"),
        repository: Repository::AgentDocs,
        branch: "docs".to_owned(),
        head: "abc".to_owned(),
        pr: Some(Pr {
            number: 7,
            url: "https://github.com/o/r/pull/7".to_owned(),
            state: PrState::Merged,
        }),
        next: "next".to_owned(),
        whose: Whose::Reviewers,
        steps: vec!["next".to_owned(), "then".to_owned()],
    };
    assert_eq!(
        serde_json::to_string(&owed).unwrap(),
        r#"{"name":"docs","path":"/w/docs","repository":"agents","branch":"docs","head":"abc","pr":{"number":7,"url":"https://github.com/o/r/pull/7","state":"merged"},"next":"next","whose":"reviewers"}"#
    );
}

/// The platform's clone and the agent docs' in `home`, each change's worktree on its branch, and
/// GitHub with each change's PR, or unreachable for `offline`.
struct Machine {
    home: PathBuf,
    prs: BTreeMap<String, String>,
}
impl Runner for Machine {
    fn command(&self, args: &[&str], cwd: Option<&Path>, _: u64) -> Result<Vec<u8>> {
        let cwd = cwd.unwrap_or(&self.home);
        let docs = cwd.starts_with(self.home.join("dispatch-agents"))
            || cwd.starts_with(self.home.join("dispatch-agents-worktrees"));
        let name = cwd.file_name().unwrap().to_string_lossy().into_owned();
        let answer = match args {
            [
                "git",
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ] => match docs {
                true => self.home.join("dispatch-agents/.git"),
                false => self.home.join("platform/repo/.git"),
            }
            .display()
            .to_string(),
            ["git", "remote", "get-url", "origin"] => match docs {
                true => "https://github.com/dispatch-systems/dispatch-agents.git".to_owned(),
                false => "https://github.com/dispatch-systems/dispatch-platform.git".to_owned(),
            },
            ["git", "branch", "--show-current"] => name,
            ["git", "rev-parse", "HEAD"] => "abc".to_owned(),
            ["gh", "pr", "list", ..] => self
                .prs
                .get(&name)
                .cloned()
                .ok_or("error connecting to api.github.com")?,
            _ => return Err(format!("unexpected {args:?}").into()),
        };
        Ok(answer.into_bytes())
    }
}

#[test]
fn each_change_in_either_clone_is_told_on_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().to_path_buf();
    let prefix = format!("owed-test-{}", std::process::id());
    let change = |suffix: &str| format!("{prefix}-{suffix}");
    let worktree = |folder: &str, name: &str, git: bool| {
        let path = home.join(folder).join(name);
        fs::create_dir_all(&path).unwrap();
        if git {
            fs::write(path.join(".git"), "gitdir: elsewhere").unwrap();
        }
        path
    };
    fs::create_dir_all(home.join("platform/repo/.git")).unwrap();
    fs::create_dir_all(home.join("dispatch-agents/.git")).unwrap();
    worktree("platform/worktrees", &change("merged"), true);
    worktree("platform/worktrees", &change("landing"), true);
    worktree("platform/worktrees", &change("offline"), true);
    // A folder that is no worktree is no change.
    worktree("platform/worktrees", &change("stray"), false);
    let closed = worktree("dispatch-agents-worktrees", &change("closed"), true);
    // Beside the change's own PR, an open one of another branch and one from a fork's branch
    // of the same name, which aren't the change's.
    let pr = |name: &str, state: &str, merged: &str| {
        format!(
            r#"[{{"number":9,"url":"u9","state":"OPEN","headRefName":"other","isCrossRepository":false,"mergeCommit":null}},
                {{"number":8,"url":"u8","state":"OPEN","headRefName":"{name}","isCrossRepository":true,"mergeCommit":null}},
                {{"number":7,"url":"u7","state":"{state}","headRefName":"{name}","isCrossRepository":false,"mergeCommit":{merged}}}]"#
        )
    };
    let machine = Machine {
        prs: BTreeMap::from([
            (
                change("merged"),
                pr(&change("merged"), "MERGED", r#"{"oid":"m1"}"#),
            ),
            (
                change("landing"),
                pr(&change("landing"), "MERGED", r#"{"oid":"m1"}"#),
            ),
            (change("closed"), pr(&change("closed"), "CLOSED", "null")),
        ]),
        home: home.clone(),
    };
    // `landed` passed on the commit one of them merged as.
    let scratch = workspace::scratch(&change("merged")).unwrap();
    workspace::write_private(
        &scratch.join(ci::LANDED),
        r#"{"commit":"m1","run":5,"result":"passed"}"#,
    )
    .unwrap();
    let env = BTreeMap::from([("HOME".to_owned(), home.display().to_string())]);
    let from = home.join("platform/repo");
    let owed = owed(&from, &[], &env, SystemTime::now(), &machine);
    let _ = fs::remove_dir_all(&scratch);
    let owed = owed.unwrap();
    let told: Vec<_> = owed
        .iter()
        .map(|owed| {
            (
                owed.name.strip_prefix(&prefix).unwrap(),
                owed.repository,
                owed.pr.as_ref().map(|pr| (pr.number, pr.state)),
                owed.whose,
            )
        })
        .collect();
    let merged = Some((7, PrState::Merged));
    assert_eq!(
        told,
        [
            ("-landing", Repository::Platform, merged, Whose::Agent),
            ("-merged", Repository::Platform, merged, Whose::Agent),
            ("-offline", Repository::Platform, None, Whose::Agent),
            (
                "-closed",
                Repository::AgentDocs,
                Some((7, PrState::Closed)),
                Whose::Agent
            ),
        ]
    );
    let next: Vec<&str> = owed.iter().map(|owed| owed.next.as_str()).collect();
    assert!(next[0].starts_with("`dispatchdev landed`"), "{next:?}");
    assert!(next[1].contains("`dispatchdev finish "), "{next:?}");
    assert_eq!(next[2], "error connecting to api.github.com");
    assert!(next[3].ends_with(" --abandon`"), "{next:?}");
    assert_eq!(owed[3].path, closed);
    // Only the changes named.
    let named = super::owed(
        &from,
        &[&change("closed")],
        &env,
        SystemTime::now(),
        &machine,
    );
    assert_eq!(named.unwrap().len(), 1);
    assert!(super::owed(&from, &["Not a name"], &env, SystemTime::now(), &machine).is_err());
}

#[test]
fn a_worktrees_folder_that_cant_be_read_is_an_error_not_nothing_owed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let clone = Clone {
        root: temp.path().join("repo"),
        worktrees: temp.path().join("worktrees"),
    };
    let clones = [(clone, Repository::Platform)];
    // No worktrees folder: no changes.
    assert!(changes(&clones).unwrap().is_empty());
    fs::create_dir(&clones[0].0.worktrees).unwrap();
    fs::set_permissions(&clones[0].0.worktrees, fs::Permissions::from_mode(0o000)).unwrap();
    let unreadable = changes(&clones);
    fs::set_permissions(&clones[0].0.worktrees, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        unreadable
            .unwrap_err()
            .to_string()
            .contains("Couldn't read"),
        "an unreadable folder isn't an empty one"
    );
}
