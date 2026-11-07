use super::*;
use crate::Native;
use std::process::Command;
use tier::Changed;

#[test]
fn a_change_is_light_standard_or_sensitive_by_the_paths_it_changes() {
    let judge = |paths: &[(&str, usize)]| {
        tier::judge(
            &paths
                .iter()
                .map(|(path, lines)| Changed {
                    path: (*path).to_owned(),
                    lines: *lines,
                })
                .collect::<Vec<_>>(),
        )
    };
    for (path, shown) in [
        (
            "android/relay/src/main/kotlin/com/dspdispatch/relay/ui/SetupScreen.kt",
            true,
        ),
        ("android/relay/src/main/res/values/strings.xml", true),
        (
            "android/relay/src/test/kotlin/com/dspdispatch/relay/ui/SetupTest.kt",
            false,
        ),
        (
            "android/relay/src/main/kotlin/com/dspdispatch/relay/sms/Sender.kt",
            false,
        ),
    ] {
        assert_eq!(ui(path), shown, "{path}");
    }
    for (paths, tier, reasons) in [
        (
            vec![("README.md", 4), ("features/dvic/tests/api/week.rs", 30)],
            Tier::Light,
            vec![],
        ),
        (
            vec![("features/dvic/frontend/Week.test.ts", 9)],
            Tier::Light,
            vec![],
        ),
        // The support worker's prompts and the review's own rules change behavior.
        (
            vec![("services/support-worker/hermes/skills/x.md", 2)],
            Tier::Standard,
            vec![],
        ),
        (vec![("tooling/review/rules.md", 2)], Tier::Standard, vec![]),
        // A Rust module's tests.rs is held only under #[cfg(test)].
        (
            vec![("tooling/cli/src/build/tests.rs", 50)],
            Tier::Light,
            vec![],
        ),
        (
            vec![
                ("mcp/backend/toolbox/grants.rs", 3),
                ("core/collection/backend/connections.rs", 3),
                ("ops/scripts/backup.py", 3),
            ],
            Tier::Sensitive,
            vec![
                "MCP access",
                "collector credentials",
                "the host manager, releases, updates and backups",
            ],
        ),
        (
            vec![("README.md", 1), ("features/dvic/backend/week.rs", 40)],
            Tier::Standard,
            vec![],
        ),
        (
            vec![
                ("features/dvic/migrations/0004_week.sql", 10),
                ("core/accounts/backend/sessions.rs", 5),
                ("features/dvic/api/routes.rs", 3),
                (".github/workflows/checks.yml", 2),
            ],
            Tier::Sensitive,
            vec![
                "migrations",
                "accounts, sessions and MFA",
                "route access",
                "CI",
            ],
        ),
        // Size alone, not counting lockfiles and generated files.
        (
            vec![
                ("features/dvic/backend/week.rs", 999),
                ("package-lock.json", 5000),
            ],
            Tier::Standard,
            vec![],
        ),
        (
            vec![
                ("features/dvic/backend/week.rs", 600),
                ("features/dvic/frontend/Week.tsx", 400),
            ],
            Tier::Sensitive,
            vec!["1000 changed lines"],
        ),
    ] {
        let judged = judge(&paths);
        assert_eq!(judged.tier, tier, "{paths:?}");
        assert_eq!(judged.reasons, reasons, "{paths:?}");
    }
    assert_eq!(
        tier::numstat("3\t1\ta.rs\0-\t-\tlogo.png\0\n1\t0\tdocs/Review process.md\0")
            .iter()
            .map(|file| (file.path.as_str(), file.lines))
            .collect::<Vec<_>>(),
        [("a.rs", 4), ("logo.png", 0), ("docs/Review process.md", 1)]
    );
}

#[test]
fn findings_are_read_by_tag_and_each_takes_only_its_kind_of_answer() {
    assert_eq!(findings::parse("No findings.\n").unwrap(), (vec![], 0));
    let text = "Two findings.\n\n## Important: a.rs:3 reads another DSP's rows\nThe query has no DSP.\n\
                ## Nit: a.rs:9 one\n## Nit: a.rs:10 two\n## Nit: a.rs:11 three\n## Nit: a.rs:12 four\n\
                ## Pre-existing: b.rs:1 drops the error\n";
    let (found, dropped) = findings::parse(text).unwrap();
    assert_eq!(dropped, 1, "a fourth Nit is past the cap");
    assert_eq!(
        found.iter().map(|finding| finding.tag).collect::<Vec<_>>(),
        [
            Tag::Important,
            Tag::Nit,
            Tag::Nit,
            Tag::Nit,
            Tag::PreExisting
        ]
    );
    for refused in [
        "## Bug: a.rs:1 x\n",
        "## Important:\n",
        "Looks fine to me.\n",
    ] {
        assert!(findings::parse(refused).is_err(), "{refused}");
    }
    let skeleton = findings::skeleton(&found);
    assert!(findings::answers(&found, &skeleton).is_err(), "unanswered");
    let answer = |important: &str, nit: &str| {
        skeleton
            .replacen(
                "reads another DSP's rows\nAnswer:",
                &format!("reads another DSP's rows\nAnswer: {important}"),
                1,
            )
            .replace("one\nAnswer:", &format!("one\nAnswer: {nit}"))
            .replace("two\nAnswer:", "two\nAnswer: fixed")
            .replace("three\nAnswer:", "three\nAnswer: fixed")
            .replace("drops the error\nAnswer:", "drops the error\nAnswer: noted")
    };
    let answered = findings::answers(
        &found,
        &answer("fixed", "declined — it matches the API's name"),
    )
    .unwrap();
    assert_eq!(answered[1].answer, "declined");
    assert_eq!(answered[1].reason, "it matches the API's name");
    for (important, nit) in [
        ("declined it's fine", "fixed"),
        ("wrong", "fixed"),
        ("fixed", "declined"),
    ] {
        assert!(
            findings::answers(&found, &answer(important, nit)).is_err(),
            "{important} / {nit}: a tag's other answers, or one without its reason"
        );
    }
}

#[test]
fn the_review_line_counts_each_tag_by_its_answers_across_passes() {
    let outcome = |tag: &str, answer: &str| Outcome {
        tag: tag.to_owned(),
        what: "a.rs:1 x".to_owned(),
        answer: answer.to_owned(),
        reason: String::new(),
        security: false,
    };
    let passes = [
        Recorded {
            model: "gpt-6.1-sol".to_owned(),
            security_model: Some("gpt-daybreak-blue-latest".to_owned()),
            head: String::new(),
            done: true,
            answers: vec![
                outcome("Important", "fixed"),
                outcome("Nit", "fixed"),
                outcome("Nit", "declined"),
                outcome("Pre-existing", "noted"),
            ],
        },
        Recorded {
            model: "gpt-6.1-sol".to_owned(),
            security_model: None,
            head: String::new(),
            done: true,
            answers: vec![outcome("Question", "kept")],
        },
    ];
    assert_eq!(
        summary(Tier::Sensitive, &passes),
        "Tier sensitive · 2 passes · gpt-6.1-sol, security gpt-daybreak-blue-latest · \
         Important 1 (fixed 1) · Nit 2 (fixed 1, declined 1) · Pre-existing 1 · Question 1 (kept 1)"
    );
    let clean = Recorded {
        model: "gpt-6.1-sol".to_owned(),
        security_model: None,
        answers: vec![],
        head: String::new(),
        done: true,
    };
    assert_eq!(
        summary(Tier::Standard, &[clean]),
        "Tier standard · 1 pass · gpt-6.1-sol · Important 0 · Nit 0 · Pre-existing 0"
    );
}

/// A clone with its own bare origin, its `main` pushed, and the review's templates; its
/// scratch folder goes with it.
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    scratch: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.scratch);
    }
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let origin = base.join("origin.git");
        // Each fixture's own scratch folder: tests run side by side in one process.
        static FIXTURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!(
            "review-test-{}-{}",
            std::process::id(),
            FIXTURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let root = base.join(&name);
        git(
            &base,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );
        git(
            &base,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                root.to_str().unwrap(),
            ],
        );
        let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../review");
        for file in ["brief.md", "security.md", "rules.md"] {
            write(
                &root,
                &format!("tooling/review/{file}"),
                &fs::read_to_string(templates.join(file)).unwrap(),
            );
        }
        write(&root, "features/dvic/backend/week.rs", "fn week() {}\n");
        commit(&root, "base");
        git(&root, &["push", "-q", "-u", "origin", "HEAD:main"]);
        git(&root, &["switch", "-q", "-c", "keep-week"]);
        let scratch = workspace::scratch(&name);
        let fixture = Self {
            _temp: temp,
            root,
            scratch,
        };
        write(
            &fixture.scratch,
            "decisions.md",
            "Keep the week when paging.\n",
        );
        write(
            &fixture.scratch,
            "pr-body.md",
            "# fix(dvic): keep the week when paging\n\n## Summary\n\nPaging keeps the week, not {{rules}}.\n\n\
             ## Symptom\n\nPaging forgot the week.\n\n## Cause\n\nCaused by: unknown\n\n## Fix\n\nx\n\n\
             ## Risk\n\nx\n\n## Verification\n\nx\n\n## Review\n\nNot reviewed: yet\n",
        );
        fixture
    }
    fn prepare(&self, pass: u32) -> Result<()> {
        prepare(
            &self.root,
            &Prepare {
                pass,
                tier: None,
                over_cap: false,
            },
            &Native,
        )
    }
    fn dir(&self, pass: u32) -> PathBuf {
        self.scratch.join("review").join(pass.to_string())
    }
    fn head(&self) -> String {
        Checkout {
            root: &self.root,
            runner: &Native,
        }
        .head()
        .unwrap()
    }
}
fn git(cwd: &Path, args: &[&str]) {
    let done = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
}
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}
fn error(result: Result<()>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn each_pass_is_pinned_to_the_pushed_head_and_later_ones_read_only_what_changed() {
    let fixture = Fixture::new();
    let root = &fixture.root;
    write(
        root,
        "features/dvic/backend/week.rs",
        "fn week() { page() }\n",
    );
    write(
        root,
        "features/dvic/migrations/0004_week.sql",
        "ALTER TABLE x ADD y;\n",
    );
    commit(root, "keep the week");
    assert!(error(fixture.prepare(1)).contains("Push it first"));
    git(root, &["push", "-q", "-u", "origin", "keep-week"]);
    write(root, "loose.txt", "x");
    assert!(error(fixture.prepare(1)).contains("Commit the change first"));
    fs::remove_file(root.join("loose.txt")).unwrap();

    fixture.prepare(1).unwrap();
    let first = fixture.head();
    let pass: Pass = read_json(&fixture.dir(1).join("pass.json"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (pass.tier, pass.security, pass.head.as_str()),
        (Tier::Sensitive, true, first.as_str())
    );
    assert_eq!(pass.reasons, ["migrations"]);
    let brief = fs::read_to_string(fixture.dir(1).join("brief.md")).unwrap();
    assert!(
        brief.contains(&format!("git diff {}..{first}", pass.base)),
        "{brief}"
    );
    assert!(brief.contains("the whole change"), "{brief}");
    assert!(
        brief.contains("> Paging keeps the week, not {{rules}}."),
        "the change's purpose: {brief}"
    );
    assert!(fixture.dir(1).join("security-brief.md").exists());
    // A pass stays pinned to the head its reviewers read, and keeps a lane's results.
    write(
        root,
        "features/dvic/backend/week.rs",
        "fn week() { page(); }\n",
    );
    commit(root, "a later push");
    git(root, &["push", "-q", "origin", "keep-week"]);
    assert!(error(fixture.prepare(1)).contains("is pinned to"));
    git(root, &["reset", "-q", "--hard", &first]);
    git(root, &["push", "-q", "-f", "origin", "keep-week"]);
    write(&fixture.dir(1), "security-findings.md", "No findings.\n");
    assert!(error(fixture.prepare(1)).contains("reviewed already"));

    assert!(error(take(root, 1, &Native)).contains("findings.md is missing"));
    write(
        &fixture.dir(1),
        "findings.md",
        "## Important: features/dvic/backend/week.rs:1 forgets the DSP\nx\n",
    );
    write(
        root,
        "features/dvic/backend/week.rs",
        "edited by the reviewer\n",
    );
    assert!(error(take(root, 1, &Native)).contains("isn't as pass 1 left it"));
    git(root, &["checkout", "--", "."]);
    take(root, 1, &Native).unwrap();

    let models = Models {
        model: "gpt-6.1-sol",
        security: Some("gpt-daybreak-blue-latest"),
    };
    let recorded = || record(root, 1, &models, &Native);
    assert!(error(recorded()).contains("No answer to"));
    let answers = fixture.dir(1).join("answers.md");
    let text = fs::read_to_string(&answers)
        .unwrap()
        .replace("DSP\nAnswer:", "DSP\nAnswer: fixed");
    fs::write(&answers, text).unwrap();
    assert!(
        error(record(
            root,
            1,
            &Models {
                security: None,
                ..models
            },
            &Native
        ))
        .contains("--security-model")
    );
    assert!(error(recorded()).contains("nothing was committed"));
    write(
        root,
        "features/dvic/backend/week.rs",
        "fn week(dsp: Dsp) { page(dsp) }\n",
    );
    commit(root, "keep the DSP");
    assert!(error(recorded()).contains("Push it first"));
    git(root, &["push", "-q", "origin", "keep-week"]);
    recorded().unwrap();
    let body = fs::read_to_string(fixture.scratch.join("pr-body.md")).unwrap();
    assert!(
        body.ends_with(
            "## Review\n\nTier sensitive · 1 pass · gpt-6.1-sol, security \
             gpt-daybreak-blue-latest · Important 1 (fixed 1) · Nit 0 · Pre-existing 0\n"
        ),
        "{body}"
    );

    // Every pass of a sensitive change has its security review beside it.
    fixture.prepare(2).unwrap();
    let pass: Pass = read_json(&fixture.dir(2).join("pass.json"))
        .unwrap()
        .unwrap();
    assert_eq!((pass.base.as_str(), pass.security), (first.as_str(), true));
    let brief = fs::read_to_string(fixture.dir(2).join("brief.md")).unwrap();
    assert!(brief.contains("what changed since pass 1"), "{brief}");
    assert!(
        brief.contains("don't raise again what they settled"),
        "{brief}"
    );
    assert!(fixture.dir(2).join("security-brief.md").exists());
    assert!(
        brief.contains(&format!("`git diff {first}..{}`", pass.head)),
        "{brief}"
    );
    assert!(error(fixture.prepare(3)).contains("Answer pass 2 first"));
    assert!(error(fixture.prepare(4)).contains("--over-cap"));
    write(&fixture.dir(2), "findings.md", "No findings.\n");
    write(&fixture.dir(2), "security-findings.md", "No findings.\n");
    assert!(error(fixture.prepare(2)).contains("reviewed already"));
    take(root, 2, &Native).unwrap();
    record(root, 2, &models, &Native).unwrap();

    // Once main is merged in, a pass reads only the branch's own files of what changed: those it
    // changes, and those it changed before, such as a migration a fix took back out.
    git(root, &["switch", "-q", "main"]);
    write(root, "features/vans/backend/van.rs", "fn van() {}\n");
    commit(root, "a van");
    git(root, &["push", "-q", "origin", "main"]);
    git(root, &["switch", "-q", "keep-week"]);
    git(root, &["merge", "-q", "--no-edit", "main"]);
    fs::remove_file(root.join("features/dvic/migrations/0004_week.sql")).unwrap();
    write(root, "features/dvic/backend/week notes.rs", "// x\n");
    commit(root, "no migration after all");
    git(root, &["push", "-q", "origin", "keep-week"]);
    fixture.prepare(3).unwrap();
    let brief = fs::read_to_string(fixture.dir(3).join("brief.md")).unwrap();
    assert!(
        brief.contains(" -- features/dvic/backend/week.rs"),
        "{brief}"
    );
    assert!(
        brief.contains("features/dvic/migrations/0004_week.sql"),
        "{brief}"
    );
    assert!(
        brief.contains("'features/dvic/backend/week notes.rs'"),
        "{brief}"
    );
    // Without its migration the change is standard, but a later pass reviews no less.
    let pass: Pass = read_json(&fixture.dir(3).join("pass.json"))
        .unwrap()
        .unwrap();
    assert_eq!((pass.tier, pass.security), (Tier::Sensitive, true));
    assert!(!brief.contains("features/vans"), "{brief}");
}

#[test]
fn a_change_of_documentation_and_tests_alone_needs_no_review_unless_asked() {
    let fixture = Fixture::new();
    let root = &fixture.root;
    write(
        root,
        "features/dvic/tests/api/week.rs",
        "#[test] fn week() {}\n",
    );
    write(root, "features/dvic/README.md", "The week.\n");
    commit(root, "test the week");
    git(root, &["push", "-q", "-u", "origin", "keep-week"]);
    fixture.prepare(1).unwrap();
    assert!(!fixture.dir(1).exists());
    let asked = Prepare {
        pass: 1,
        tier: Some("standard"),
        over_cap: false,
    };
    prepare(root, &asked, &Native).unwrap();
    write(&fixture.dir(1), "findings.md", "No findings.\n");
    take(root, 1, &Native).unwrap();
    let models = Models {
        model: "gpt-6.1-sol",
        security: None,
    };
    // A clean pass recorded after later commits is kept, and the pass after it reviews them,
    // at the tier asked for.
    write(
        root,
        "features/dvic/tests/api/week.rs",
        "#[test] fn weeks() {}\n",
    );
    commit(root, "unreviewed");
    git(root, &["push", "-q", "origin", "keep-week"]);
    let reviewed: Pass = read_json(&fixture.dir(1).join("pass.json"))
        .unwrap()
        .unwrap();
    record(root, 1, &models, &Native).unwrap();
    fixture.prepare(2).unwrap();
    let pass: Pass = read_json(&fixture.dir(2).join("pass.json"))
        .unwrap()
        .unwrap();
    assert_eq!((pass.tier, pass.asked), (Tier::Standard, true));
    assert_eq!(pass.base, reviewed.head);
}

#[test]
fn a_change_to_what_a_page_shows_is_held_to_what_the_user_approved() {
    use preview::Approval;
    let fixture = Fixture::new();
    let root = &fixture.root;
    let approval = || preview::approval(root, &Native).unwrap();
    // A template the scaffolder makes pages from isn't what a page shows.
    write(
        root,
        "tooling/scaffold/templates/feature/frontend/Page.tsx.tmpl",
        "x\n",
    );
    commit(root, "a template");
    assert_eq!(approval(), Approval::None);
    write(
        root,
        "features/dvic/frontend/Week.tsx",
        "export const week = 1;\n",
    );
    commit(root, "the week's page");
    assert_eq!(approval(), Approval::Missing);
    // Approved as the worktree showed it, before it was committed.
    write(
        root,
        "features/dvic/frontend/Week.tsx",
        "export const week = 2;\n",
    );
    write(root, "features/dvic/frontend/week.css", ".week {}\n");
    preview::approve(root, &Native).unwrap();
    commit(root, "as shown");
    assert_eq!(approval(), Approval::Approved);
    // A file approved and since put back as main has it no longer looks as approved.
    fs::remove_file(root.join("features/dvic/frontend/week.css")).unwrap();
    commit(root, "without its styles");
    assert_eq!(
        approval(),
        Approval::Changed(vec!["features/dvic/frontend/week.css".to_owned()])
    );
    write(root, "features/dvic/frontend/week.css", ".week {}\n");
    commit(root, "its styles again");
    assert_eq!(approval(), Approval::Approved);
    // Once main has the same styles, the change no longer touches them, and they still look
    // as approved.
    git(root, &["switch", "-q", "main"]);
    write(root, "features/dvic/frontend/week.css", ".week {}\n");
    commit(root, "the week's styles");
    git(root, &["push", "-q", "origin", "main"]);
    git(root, &["switch", "-q", "keep-week"]);
    git(root, &["merge", "-q", "--no-edit", "main"]);
    assert_eq!(approval(), Approval::Approved);

    git(root, &["push", "-q", "-u", "origin", "keep-week"]);
    fixture.prepare(1).unwrap();
    write(
        &fixture.dir(1),
        "findings.md",
        "## Nit: features/dvic/frontend/Week.tsx:1 a clearer name\n",
    );
    take(root, 1, &Native).unwrap();
    let answers = fixture.dir(1).join("answers.md");
    let text = fs::read_to_string(&answers)
        .unwrap()
        .replace("name\nAnswer:", "name\nAnswer: fixed");
    fs::write(&answers, text).unwrap();
    write(
        root,
        "features/dvic/frontend/Week.tsx",
        "export const weeks = 2;\n",
    );
    commit(root, "a clearer name");
    git(root, &["push", "-q", "origin", "keep-week"]);
    let models = Models {
        model: "gpt-6.1-sol",
        security: None,
    };
    record(root, 1, &models, &Native).unwrap();
    assert_eq!(
        approval(),
        Approval::Changed(vec!["features/dvic/frontend/Week.tsx".to_owned()])
    );
    let written = fs::read_to_string(fixture.scratch.join("pr-body.md")).unwrap();
    let body = pr::titled(&written).1;
    assert!(
        body.contains("· the UI changed after its preview: features/dvic/frontend/Week.tsx"),
        "the review's fix is noted, not shown again: {body}"
    );

    // A Nit's fix needs no other pass, so the review is done at this head, and only there.
    let unreviewed = |body: &str| {
        let head = fixture.head();
        crate::merge::unreviewed(root, body, &head, &Native).unwrap()
    };
    assert_eq!(unreviewed(body), None);
    assert_eq!(unreviewed("## Review\n\nNot reviewed: main failed\n"), None);
    let light = unreviewed("## Review\n\nTier light · not reviewed\n").unwrap();
    assert!(light.contains("is standard now"), "{light}");
    write(
        root,
        "features/dvic/frontend/Week.tsx",
        "export const weeks = 3;\n",
    );
    commit(root, "after the review");
    git(root, &["push", "-q", "origin", "keep-week"]);
    let later = unreviewed(body).unwrap();
    assert!(later.contains("before the latest commits"), "{later}");
    // Recording the pass again doesn't vouch for what came after it.
    let again = error(record(root, 1, &models, &Native));
    assert!(again.contains("was recorded at"), "{again}");
}
