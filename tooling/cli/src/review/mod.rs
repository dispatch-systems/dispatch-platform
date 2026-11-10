//! A change's review, pass by pass. `prepare` judges the change's tier and writes each pass's
//! brief, pinned to the pushed head: the whole change first, then only what changed since the
//! pass before, with a security review beside it only where that calls for one. `take` checks
//! the reviewer left the worktree as it was and starts the answers; `record` checks every
//! finding is answered and writes the PR body's Review line. Each pass keeps its files in the
//! checkout's scratch folder, under `review/<pass>/`.
pub mod findings;
pub mod tier;

use crate::{Result, Runner, change::Clone, pr, preview, require, workspace};
use findings::{Answered, Tag};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tier::{Later, Tier};

/// Passes a review stops after, unless more were asked for.
pub const CAP: u32 = 3;

/// A pass as `prepare` pinned it.
#[derive(Serialize, Deserialize)]
pub struct Pass {
    pub pass: u32,
    pub head: String,
    pub base: String,
    /// The diff the pass reviews, as its brief gives it.
    #[serde(default)]
    pub diff: String,
    pub tier: Tier,
    pub reasons: Vec<String>,
    /// Whether the tier was asked for rather than judged: later passes keep it.
    #[serde(default)]
    pub asked: bool,
    /// Whether a security review runs beside it.
    pub security: bool,
}
/// A pass's answers as `record` found them, and the models that reviewed it.
#[derive(Serialize, Deserialize)]
pub struct Recorded {
    pub model: String,
    pub security_model: Option<String>,
    pub answers: Vec<Outcome>,
    /// The pushed head when it was recorded.
    #[serde(default)]
    pub head: String,
    /// Whether the review is done at that head: nothing left that needs another pass.
    #[serde(default)]
    pub done: bool,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Outcome {
    pub tag: String,
    pub what: String,
    pub answer: String,
    pub reason: String,
    pub security: bool,
}

/// The two reviews a pass may have: its files, and what they're called.
#[derive(Clone, Copy)]
enum Review {
    Bugs,
    Security,
}
impl Review {
    fn brief(self) -> &'static str {
        match self {
            Self::Bugs => "brief.md",
            Self::Security => "security-brief.md",
        }
    }
    fn template(self) -> &'static str {
        match self {
            Self::Bugs => "tooling/review/brief.md",
            Self::Security => "tooling/review/security.md",
        }
    }
    fn findings(self) -> &'static str {
        match self {
            Self::Bugs => "findings.md",
            Self::Security => "security-findings.md",
        }
    }
    fn answers(self) -> &'static str {
        match self {
            Self::Bugs => "answers.md",
            Self::Security => "security-answers.md",
        }
    }
}
fn reviews(pass: &Pass) -> Vec<Review> {
    match pass.security {
        true => vec![Review::Bugs, Review::Security],
        false => vec![Review::Bugs],
    }
}

/// The checkout's git, as it stands.
struct Checkout<'a> {
    root: &'a Path,
    runner: &'a dyn Runner,
}
impl Checkout<'_> {
    fn git(&self, args: &[&str]) -> Result<String> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(self.runner, &command, Some(self.root))
    }
    fn head(&self) -> Result<String> {
        self.git(&["rev-parse", "HEAD"])
    }
    /// The branch, committed and pushed as it is: what a reviewer is pinned to.
    fn pushed(&self) -> Result<(String, String)> {
        let branch = self.git(&["branch", "--show-current"])?;
        require(
            !branch.is_empty() && branch != "main",
            "Review a change's own branch, not main.",
        )?;
        require(
            self.git(&["status", "--porcelain"])?.is_empty(),
            "Commit the change first: a review reads what is committed.",
        )?;
        let head = self.head()?;
        let remote = self
            .git(&[
                "rev-parse",
                "--verify",
                "-q",
                &format!("refs/remotes/origin/{branch}"),
            ])
            .unwrap_or_default();
        require(
            remote == head,
            "Push it first with `dispatchdev pr`: a review reads the PR's head.",
        )?;
        Ok((branch, head))
    }
    /// Where `rev` left `main`, and the files it changes against it.
    fn against_main(&self, rev: &str) -> Result<(String, Vec<tier::Changed>)> {
        let main = self.git(&["merge-base", "origin/main", rev])?;
        let changed = self.changed(&format!("{main}..{rev}"))?;
        Ok((main, changed))
    }
    /// The files that differ across `range`.
    fn changed(&self, range: &str) -> Result<Vec<tier::Changed>> {
        Ok(tier::numstat(&self.git(&[
            "diff",
            "--numstat",
            "--no-renames",
            "-z",
            range,
        ])?))
    }
    /// Whether `main` was merged in between `reviewed` and `head`.
    fn main_merged(&self, reviewed: &str, head: &str) -> Result<bool> {
        let left = |rev| self.git(&["merge-base", "origin/main", rev]);
        Ok(left(reviewed)? != left(head)?)
    }
    /// The files the branch itself changed from `reviewed` to `head`: those that differ, or once
    /// `main` was merged in since, those its own commits changed (`COMMITS`). None when one of
    /// those commits merges more than two parents: git shows nothing of what such a merge holds
    /// of its own, so nothing can be told from them.
    fn since(&self, reviewed: &str, head: &str) -> Result<Option<Vec<tier::Changed>>> {
        let range = format!("{reviewed}..{head}");
        if !self.main_merged(reviewed, head)? {
            return Ok(Some(self.changed(&range)?));
        }
        if !self
            .git(&["rev-list", "--min-parents=3", &range, NOT_MAIN])?
            .is_empty()
        {
            return Ok(None);
        }
        let mut args = COMMITS.to_vec();
        args.extend([
            "--format=",
            "--numstat",
            "--no-renames",
            "-z",
            &range,
            NOT_MAIN,
        ]);
        Ok(Some(tier::numstat(&self.git(&args)?)))
    }
}

/// How git lists a branch's own commits over a range that holds a merge of `main`, each with
/// what it changed itself: a merge's is what it holds beyond what git merges unaided. So `main`
/// merged in shows nothing, a merge edited by hand shows the edit, and undoing what `main`
/// brought shows as the commit that did, where the difference across the range would show
/// `main`'s changes and hide that one.
const COMMITS: [&str; 3] = ["log", "--reverse", "--remerge-diff"];
/// Leaves `main`'s own commits out of them.
const NOT_MAIN: &str = "^origin/main";

/// What the commits from `reviewed` to `head` still need of the review that read `reviewed`, or
/// whose answers account for it: nothing for `main` merged in as git merges it unaided or for
/// documentation alone, a pass on the tests for tests alone, and a pass for the rest, as for
/// what can't be told.
pub fn later(root: &Path, reviewed: &str, head: &str, runner: &dyn Runner) -> Result<Later> {
    if reviewed == head {
        return Ok(Later::Nothing);
    }
    Ok(match (Checkout { root, runner }).since(reviewed, head)? {
        Some(since) => tier::later(&since),
        None => Later::Pass,
    })
}

/// The checkout's branch and head, committed and pushed as they are.
pub fn pushed(root: &Path, runner: &dyn Runner) -> Result<(String, String)> {
    Checkout { root, runner }.pushed()
}

/// Where a change's review stands, by its latest pass.
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    NotStarted,
    /// Its briefs are written; its reviewers haven't answered.
    Prepared(u32),
    /// Its reviewers answered; its findings aren't answered and recorded yet.
    Answering(u32),
    Recorded {
        pass: u32,
        done: bool,
        head: String,
    },
}
pub fn progress(root: &Path) -> Result<Progress> {
    let mut latest = 0;
    while folder(root, latest + 1)?.join("pass.json").exists() {
        latest += 1;
    }
    if latest == 0 {
        return Ok(Progress::NotStarted);
    }
    let dir = folder(root, latest)?;
    if let Some(recorded) = read_json::<Recorded>(&dir.join("record.json"))? {
        return Ok(Progress::Recorded {
            pass: latest,
            done: recorded.done,
            head: recorded.head,
        });
    }
    Ok(match dir.join(Review::Bugs.findings()).exists() {
        true => Progress::Answering(latest),
        false => Progress::Prepared(latest),
    })
}
/// The Pre-existing and Out of scope findings of every recorded pass, for the merge message.
pub fn left(root: &Path) -> Result<Vec<Outcome>> {
    let mut left = vec![];
    let mut n = 1;
    while let Some(recorded) = read_json::<Recorded>(&folder(root, n)?.join("record.json"))? {
        left.extend(
            recorded
                .answers
                .into_iter()
                .filter(|outcome| matches!(outcome.tag.as_str(), "Pre-existing" | "Out of scope")),
        );
        n += 1;
    }
    Ok(left)
}
/// The tier the change's paths give it now.
pub fn judged(root: &Path, runner: &dyn Runner) -> Result<tier::Judged> {
    let (_, changed) = Checkout { root, runner }.against_main("HEAD")?;
    Ok(tier::judge(&changed))
}

/// The folder of pass `pass`.
fn folder(root: &Path, pass: u32) -> Result<PathBuf> {
    Ok(workspace::checkout_scratch(root)?
        .join("review")
        .join(pass.to_string()))
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn pinned(root: &Path, pass: u32) -> Result<Pass> {
    read_json(&folder(root, pass)?.join("pass.json"))?.ok_or_else(|| {
        format!("Pass {pass} isn't prepared: `dispatchdev review prepare --pass {pass}`.").into()
    })
}

/// How `prepare` was asked to run.
pub struct Prepare<'a> {
    pub pass: u32,
    /// A tier to use in place of the one the change's paths give.
    pub tier: Option<&'a str>,
    /// Whether a pass past `CAP` was asked for.
    pub over_cap: bool,
    /// The platform checkout the CLI runs from: a checkout with no review instructions of its
    /// own, such as the agent docs', is reviewed by these.
    pub source: Option<&'a Path>,
}
/// The checkout whose `tooling/review/` instructions a review of `root` follows: its own, else
/// those of the platform checkout the CLI runs from.
fn instructions<'a>(root: &'a Path, source: Option<&'a Path>) -> Result<&'a Path> {
    let has = |checkout: &Path| checkout.join(Review::Bugs.template()).is_file();
    match (has(root), source.filter(|source| has(source))) {
        (true, _) => Ok(root),
        (false, Some(source)) => Ok(source),
        (false, None) => Err(
            "This checkout has no review instructions (tooling/review/): run it \
                              through a platform checkout's dispatchdev."
                .into(),
        ),
    }
}

/// `dispatchdev review prepare`: the change's tier, and pass `pass`'s briefs, unless it needs
/// no review.
pub fn prepare(root: &Path, ask: &Prepare, runner: &dyn Runner) -> Result<()> {
    let n = ask.pass;
    require(n >= 1, "Passes count from 1.")?;
    require(
        n <= CAP || ask.over_cap,
        &format!("A review stops after pass {CAP}; --over-cap runs another when it was asked for."),
    )?;
    let git = Checkout { root, runner };
    let (branch, head) = git.pushed()?;
    let dir = folder(root, n)?;
    require(
        [Review::Bugs, Review::Security]
            .iter()
            .all(|review| !dir.join(review.findings()).exists()),
        &format!("Pass {n} is reviewed already: prepare pass {}.", n + 1),
    )?;
    if let Some(pinned) = read_json::<Pass>(&dir.join("pass.json"))? {
        require(
            pinned.head == head,
            &format!(
                "Pass {n} is pinned to {} already, and its reviewers may be reading it: once \
                 they stop, delete {} to pin it to {} instead.",
                short(&pinned.head),
                dir.display(),
                short(&head)
            ),
        )?;
    }
    let previous = match n {
        1 => None,
        _ => {
            let recorded: Recorded = read_json(&folder(root, n - 1)?.join("record.json"))?
                .ok_or_else(|| {
                    format!(
                        "Answer pass {} first: `dispatchdev review record --pass {}`.",
                        n - 1,
                        n - 1
                    )
                })?;
            Some((pinned(root, n - 1)?, recorded))
        }
    };
    for earlier in 1..n {
        let pass = pinned(root, earlier)?;
        require(
            pass.head != head,
            &format!(
                "Pass {earlier} reviewed {} already: commit what changes first.",
                short(&head)
            ),
        )?;
    }
    let (main, changed) = git.against_main(&head)?;
    require(
        !changed.is_empty(),
        "The branch changes nothing against origin/main.",
    )?;
    let judged = tier::judge(&changed);
    let (tier, asked) = match (ask.tier, &previous) {
        (Some(name), _) => (
            Tier::parse(name).ok_or("A tier is light, standard or sensitive.")?,
            true,
        ),
        // A later pass never reviews less than the one before: a fix that shrank the change
        // still needs what found it.
        (None, Some((previous, _))) => (previous.tier.max(judged.tier), previous.asked),
        (None, None) => (judged.tier, false),
    };
    if tier == Tier::Light {
        println!(
            "{branch} is light (documentation and tests alone): it needs no review. Its PR's \
             Review section opens `Tier light · not reviewed`."
        );
        return Ok(());
    }
    let base = previous
        .as_ref()
        .map_or(main, |(pass, _)| pass.head.clone());
    // A later pass reads what the branch itself changed since the one before, and that sets
    // who reads it: the tests alone when nothing else changed, and the security reviewer only
    // for a sensitive path in it, a fix its own review asked for, or a tier that was asked for.
    // Where that can't be told, it reads all that differs, its security reviewer too.
    let since = match &previous {
        Some(_) => git.since(&base, &head)?,
        None => None,
    };
    let diff = match &since {
        Some(_) if git.main_merged(&base, &head)? => {
            format!("git {} -p {base}..{head} {NOT_MAIN}", COMMITS.join(" "))
        }
        _ => format!("git diff {base}..{head}"),
    };
    let (tests, security) = match &previous {
        None => (false, true),
        Some((_, recorded)) => (
            since
                .as_ref()
                .is_some_and(|since| tier::later(since) == Later::Tests),
            asked
                || since.is_none_or(|since| {
                    since
                        .iter()
                        .any(|file| tier::sensitive(&file.path).is_some())
                })
                || recorded.answers.iter().any(|outcome| {
                    outcome.security && outcome.tag == "Important" && outcome.answer == "fixed"
                }),
        ),
    };
    let security = security && tier == Tier::Sensitive;
    let pass = Pass {
        pass: n,
        head: head.clone(),
        base: base.clone(),
        diff,
        tier,
        reasons: judged.reasons,
        asked,
        security,
    };
    let paths: Vec<&str> = changed.iter().map(|file| file.path.as_str()).collect();
    let fill = Fill {
        root,
        instructions: instructions(root, ask.source)?,
        dir: &dir,
        branch: &branch,
        pass: &pass,
        tests,
        paths: &paths,
        runner,
    };
    fs::create_dir_all(&dir)?;
    let mut written = vec![];
    for review in reviews(&pass) {
        let brief = fill.brief(review)?;
        workspace::write_private(&dir.join(review.brief()), &brief)?;
        written.push(dir.join(review.brief()).display().to_string());
    }
    workspace::write_private(
        &dir.join("pass.json"),
        &serde_json::to_string_pretty(&pass)?,
    )?;
    println!(
        "Pass {n} of {branch} at {}: {tier}{}.",
        short(&head),
        match pass.reasons.is_empty() {
            true => String::new(),
            false => format!(" ({})", pass.reasons.join(", ")),
        }
    );
    if tests {
        println!(
            "Tests alone changed since pass {}: its reviewer reads them alone.",
            n - 1
        );
    }
    if tier == Tier::Sensitive && !pass.security {
        println!(
            "No security review beside it: nothing sensitive changed since pass {}, whose \
             security review asked for no fix.",
            n - 1
        );
    }
    println!("Wrote {}.", written.join(" and "));
    println!(
        "{}",
        workspace::next(&[
            match pass.security {
                true => "Start the reviewer on brief.md and the security reviewer on \
                         security-brief.md, side by side, with the models the skill names"
                    .to_owned(),
                false => "Start the reviewer on brief.md with the model the skill names".to_owned(),
            },
            format!("Once it answers, `dispatchdev review take --pass {n}`"),
        ])
    );
    Ok(())
}

/// What a brief is written from.
struct Fill<'a> {
    root: &'a Path,
    /// Where the templates and rules come from.
    instructions: &'a Path,
    dir: &'a Path,
    branch: &'a str,
    pass: &'a Pass,
    /// Whether the pass reads tests alone: all that changed since the pass before.
    tests: bool,
    paths: &'a [&'a str],
    runner: &'a dyn Runner,
}
impl Fill<'_> {
    fn brief(&self, review: Review) -> Result<String> {
        let template = fs::read_to_string(self.instructions.join(review.template()))?;
        let scratch = workspace::checkout_scratch(self.root)?;
        let decisions = scratch.join("decisions.md");
        require(
            decisions.exists(),
            &format!(
                "{} is missing: write what was approved there first.",
                decisions.display()
            ),
        )?;
        let scope = match self.pass.pass {
            1 => "the whole change".to_owned(),
            n if self.tests => format!("the tests changed since pass {}, alone", n - 1),
            n => format!(
                "what changed since pass {}, and how it fits the rest of the change",
                n - 1
            ),
        };
        let earlier = match self.pass.pass {
            1 => String::new(),
            n => format!(
                "\nEarlier passes' findings and the answers to them are in {}/1/ to {}/{}/; don't \
                 raise again what they settled.\n",
                scratch.join("review").display(),
                scratch.join("review").display(),
                n - 1
            ),
        };
        let values = [
            ("branch", self.branch.to_owned()),
            ("head", self.pass.head.clone()),
            ("diff", self.pass.diff.clone()),
            ("worktree", self.root.display().to_string()),
            ("scope", scope),
            (
                "findings",
                self.dir.join(review.findings()).display().to_string(),
            ),
            ("purpose", purpose(self.root)?),
            ("decisions", decisions.display().to_string()),
            (
                "rules",
                self.instructions
                    .join("tooling/review/rules.md")
                    .display()
                    .to_string(),
            ),
            ("reading", self.reading()?),
            ("earlier", earlier),
        ];
        fill(&template, &values).ok_or_else(|| {
            format!("{} names a value prepare doesn't fill.", review.template()).into()
        })
    }
    /// The instructions beyond the rules a reviewer reads for this change: `AGENTS.md`, its UI
    /// copy rule for a change to what a page shows, and the bar for tests for one to tests.
    fn reading(&self) -> Result<String> {
        let platform = Clone::of(self.root, self.runner)?
            .root
            .parent()
            .map(Path::to_path_buf)
            .ok_or("A clone without a folder")?;
        let agents = platform.join("AGENTS.md");
        let tests =
            platform.join(".claude/skills/dispatch-development/references/writing-tests.md");
        let mut lines = vec![];
        if agents.exists() {
            lines.push(format!("- {}: how code here is written.", agents.display()));
            if self.paths.iter().any(|path| ui(path)) {
                lines.push("- Its UI Copy section, for what the changed pages show.".to_string());
            }
        }
        if tests.exists() && self.paths.iter().any(|path| tier::test(path)) {
            lines.push(format!("- {}: the bar a test is held to.", tests.display()));
        }
        if lines.is_empty() {
            lines.push("- nothing beyond the rules.".to_owned());
        }
        Ok(lines.join("\n"))
    }
}

/// `template` with each `{{key}}` in it replaced by its value, read once from the template alone
/// so a value's own text stays as it is; None if it names a key `values` lacks.
fn fill(template: &str, values: &[(&str, String)]) -> Option<String> {
    let mut text = String::new();
    let mut rest = template;
    while let Some(at) = rest.find("{{") {
        let (before, after) = rest.split_at(at);
        let end = after.find("}}")?;
        let key = &after[2..end];
        text.push_str(before);
        text.push_str(&values.iter().find(|(name, _)| *name == key)?.1);
        rest = &after[end + 2..];
    }
    text.push_str(rest);
    Some(text)
}

/// Whether a path is part of what someone sees: a page's code or styles, or an Android app's
/// screens, their Compose code under `ui/` or its resources, but not their tests, nor the
/// templates `tooling/` makes them from.
pub fn ui(path: &str) -> bool {
    !path.starts_with("tooling/")
        && !tier::test(path)
        && (path.contains("/frontend/")
            || path.ends_with(".css")
            || (path.starts_with("android/")
                && path.contains("/src/main/")
                && (path.contains("/res/") || path.contains("/ui/"))))
}

/// What the change is for, from its PR body: the title, its Summary, and its Problem, Symptom
/// or Why.
fn purpose(root: &Path) -> Result<String> {
    let path = workspace::checkout_scratch(root)?.join("pr-body.md");
    let text = fs::read_to_string(&path).map_err(|_| {
        format!(
            "{} is missing: the brief takes the change's purpose from it.",
            path.display()
        )
    })?;
    let (title, body) = pr::titled(&text);
    let mut parts = vec![title.unwrap_or_default().to_owned()];
    for name in ["Summary", "Problem", "Symptom", "Why"] {
        if let Some(section) = pr::section(body, name).filter(|text| !text.is_empty()) {
            parts.push(section.to_owned());
        }
    }
    Ok(parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(|part| format!("> {}", part.replace('\n', "\n> ")))
        .collect::<Vec<_>>()
        .join("\n>\n"))
}

/// `dispatchdev review take`: once the reviewers answered, checks they left the worktree as it
/// was and starts the answers to their findings.
pub fn take(root: &Path, n: u32, runner: &dyn Runner) -> Result<()> {
    let pass = pinned(root, n)?;
    let dir = folder(root, n)?;
    let git = Checkout { root, runner };
    require(
        git.git(&["status", "--porcelain"])?.is_empty() && git.head()? == pass.head,
        &format!(
            "The worktree isn't as pass {n} left it, at {} with nothing uncommitted: see what \
             the reviewer changed before taking its findings.",
            short(&pass.head)
        ),
    )?;
    let mut any = false;
    for review in reviews(&pass) {
        let found = dir.join(review.findings());
        let text = fs::read_to_string(&found).map_err(|_| {
            format!(
                "{} is missing: the reviewer hasn't answered.",
                found.display()
            )
        })?;
        let (findings, dropped) = findings::parse(&text)?;
        any |= !findings.is_empty();
        let answers = dir.join(review.answers());
        if !answers.exists() {
            workspace::write_private(&answers, &findings::skeleton(&findings))?;
        }
        let count = |tag| findings.iter().filter(|finding| finding.tag == tag).count();
        println!(
            "{}: {} Important, {} Nit, {} Pre-existing, {} Question, {} Out of scope{}.",
            found.display(),
            count(Tag::Important),
            count(Tag::Nit),
            count(Tag::PreExisting),
            count(Tag::Question),
            count(Tag::OutOfScope),
            match dropped {
                0 => String::new(),
                dropped => format!(", and {dropped} Nits past the cap left unanswered"),
            }
        );
    }
    let record = format!(
        "`dispatchdev review record --pass {n} --model <model>{}`",
        if pass.security {
            " --security-model <model>"
        } else {
            ""
        }
    );
    println!(
        "{}",
        workspace::next(&match any {
            true => vec![
                format!(
                    "Check each finding against the code and answer it in {}",
                    dir.join("answers.md").display()
                ),
                format!("Commit the fixes and {}", publishing(root).0),
                format!("Then {record}"),
            ],
            false => vec![format!("Nothing to answer: {record}")],
        })
    );
    Ok(())
}

/// The models that reviewed a pass, as `record` was told them.
pub struct Models<'a> {
    pub model: &'a str,
    pub security: Option<&'a str>,
}

/// `dispatchdev review record`: every finding of pass `n` answered and its fixes pushed, the
/// answers kept, and the PR body's Review line written from every pass so far.
pub fn record(root: &Path, n: u32, models: &Models, runner: &dyn Runner) -> Result<()> {
    let pass = pinned(root, n)?;
    require(
        models.security.is_some() == pass.security,
        match pass.security {
            true => "Pass it --security-model too: a security review ran beside this pass.",
            false => "No security review ran beside this pass: drop --security-model.",
        },
    )?;
    let dir = folder(root, n)?;
    let mut answers = vec![];
    for review in reviews(&pass) {
        let (found, _) = findings::parse(&fs::read_to_string(dir.join(review.findings()))?)?;
        let path = dir.join(review.answers());
        let text = fs::read_to_string(&path).map_err(|_| {
            format!(
                "{} is missing: `dispatchdev review take --pass {n}` starts it.",
                path.display()
            )
        })?;
        answers.extend(
            findings::answers(&found, &text)?
                .into_iter()
                .map(|answered| outcome(answered, matches!(review, Review::Security))),
        );
    }
    let git = Checkout { root, runner };
    let (_, head) = git.pushed()?;
    // A pass is recorded at one head: recording it again later would vouch for what came after.
    if let Some(earlier) =
        read_json::<Recorded>(&dir.join("record.json"))?.filter(|earlier| !earlier.head.is_empty())
    {
        require(
            earlier.head == head,
            &format!(
                "Pass {n} was recorded at {}: what came after is reviewed by `dispatchdev review \
                 prepare --pass {}`.",
                short(&earlier.head),
                n + 1
            ),
        )?;
    }
    let changed = answers
        .iter()
        .any(|outcome| matches!(outcome.answer.as_str(), "fixed" | "changed"));
    require(
        !changed || head != pass.head,
        &format!(
            "Answers say fixed, but nothing was committed since {}.",
            short(&pass.head)
        ),
    )?;
    let important_fixed = answers
        .iter()
        .any(|outcome| outcome.tag == "Important" && outcome.answer == "fixed");
    // Commits no answer accounts for were reviewed by no one, and all but `main` merged in and
    // documentation need it: the pass is kept, but the review isn't done at this head.
    let unreviewed = !changed && later(root, &pass.head, &head, runner)? != Later::Nothing;
    let recorded = Recorded {
        model: models.model.to_owned(),
        security_model: models.security.map(str::to_owned),
        answers,
        done: !unreviewed && (!important_fixed || n >= CAP),
        head: head.clone(),
    };
    workspace::write_private(
        &dir.join("record.json"),
        &serde_json::to_string_pretty(&recorded)?,
    )?;
    let mut passes = vec![];
    for k in 1..=n {
        let recorded: Recorded = read_json(&folder(root, k)?.join("record.json"))?
            .ok_or_else(|| format!("Pass {k} has no record: record it first."))?;
        passes.push(recorded);
    }
    let mut line = summary(pass.tier, &passes);
    if let preview::Approval::Changed(paths) = preview::approval(root, runner)? {
        line.push_str(&format!(
            " · the UI changed after its preview: {}",
            paths.join(", ")
        ));
    }
    let body = workspace::checkout_scratch(root)?.join("pr-body.md");
    let text = fs::read_to_string(&body)?;
    workspace::write_private(&body, &pr::with_review_line(&text, &line)?)?;
    println!("{line}");
    for outcome in recorded
        .answers
        .iter()
        .filter(|outcome| matches!(outcome.tag.as_str(), "Pre-existing" | "Out of scope"))
    {
        println!("  {}: {}", outcome.tag, outcome.what);
    }
    let (_, update, merge) = publishing(root);
    let next = match (important_fixed || unreviewed, n < CAP) {
        (true, true) if unreviewed => format!(
            "Commits since pass {n}'s {} were reviewed by no one: `dispatchdev review prepare \
             --pass {}` reviews them",
            short(&pass.head),
            n + 1
        ),
        (true, false) if unreviewed => format!(
            "Commits since pass {n}'s {} were reviewed by no one, and the review stops at pass \
             {CAP}: another needs --over-cap, once it was asked for",
            short(&pass.head)
        ),
        (true, true) => format!(
            "{update}; then `dispatchdev review prepare --pass {}` reviews what fixing the \
             Important findings changed",
            n + 1
        ),
        (true, false) => format!(
            "The review stops at pass {CAP}: {update}; {merge}, unless more passes were asked \
             for (--over-cap)"
        ),
        (false, _) => format!("The review is done: {update}; {merge}"),
    };
    println!("{}", workspace::next(&[next]));
    Ok(())
}
/// How the checkout's PR takes its new body and merges: through `dispatchdev` on the platform,
/// through `gh` in a checkout without the CLI, such as the agent docs', as their skill says.
/// Each is a step: pushing fixes, updating the body, merging.
fn publishing(root: &Path) -> (&'static str, &'static str, &'static str) {
    match root.join("tooling/cli/dispatchdev").is_file() {
        true => (
            "push them with `dispatchdev pr`",
            "`dispatchdev pr` updates the PR's body",
            "`dispatchdev merge` merges it once its checks pass",
        ),
        false => (
            "push them with `git push`",
            "`gh pr edit <n> --body \"$(tail -n +3 <scratch>/pr-body.md)\"` updates the PR's body",
            "`gh pr merge <n> --squash --match-head-commit <head>` merges it",
        ),
    }
}
fn outcome(answered: Answered, security: bool) -> Outcome {
    Outcome {
        tag: answered.finding.tag.to_string(),
        what: answered.finding.what,
        answer: answered.answer.to_owned(),
        reason: answered.reason,
        security,
    }
}

/// The Review line of a PR reviewed in `passes` at `tier`: the tier, the passes and their
/// models, then each tag's findings by how they were answered.
pub fn summary(tier: Tier, passes: &[Recorded]) -> String {
    let mut models: Vec<String> = vec![];
    for pass in passes {
        let mut add = |model: String| {
            if !models.contains(&model) {
                models.push(model);
            }
        };
        add(pass.model.clone());
        if let Some(security) = &pass.security_model {
            add(format!("security {security}"));
        }
    }
    let mut parts = vec![
        format!("Tier {tier}"),
        match passes.len() {
            1 => "1 pass".to_owned(),
            n => format!("{n} passes"),
        },
        models.join(", "),
    ];
    let all: Vec<&Outcome> = passes.iter().flat_map(|pass| &pass.answers).collect();
    for (tag, always) in [
        ("Important", true),
        ("Nit", true),
        ("Pre-existing", true),
        ("Question", false),
        ("Out of scope", false),
    ] {
        let of: Vec<&&Outcome> = all.iter().filter(|outcome| outcome.tag == tag).collect();
        if of.is_empty() && !always {
            continue;
        }
        let mut answers: Vec<(&str, usize)> = vec![];
        for outcome in &of {
            match answers
                .iter_mut()
                .find(|(answer, _)| *answer == outcome.answer)
            {
                Some((_, count)) => *count += 1,
                None => answers.push((&outcome.answer, 1)),
            }
        }
        parts.push(match answers.as_slice() {
            [] | [("noted", _)] => format!("{tag} {}", of.len()),
            _ => format!(
                "{tag} {} ({})",
                of.len(),
                answers
                    .iter()
                    .map(|(answer, count)| format!("{answer} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    parts.join(" · ")
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

#[cfg(test)]
mod tests;
