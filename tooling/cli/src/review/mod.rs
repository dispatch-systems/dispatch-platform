//! A change's review, pass by pass. `prepare` judges the change's tier and writes each pass's
//! brief, pinned to the pushed head: the whole change first, then only what changed since the
//! pass before. `take` checks the reviewer left the worktree as it was and starts the answers;
//! `record` checks every finding is answered and writes the PR body's Review line. Each pass
//! keeps its files in the checkout's scratch folder, under `review/<pass>/`.
pub mod findings;
pub mod tier;

use crate::{Result, Runner, change::Clone, pr, require, workspace};
use findings::{Answered, Tag};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tier::Tier;

/// Passes a review stops after, unless more were asked for.
pub const CAP: u32 = 3;

/// A pass as `prepare` pinned it.
#[derive(Serialize, Deserialize)]
pub struct Pass {
    pub pass: u32,
    pub head: String,
    pub base: String,
    /// Where the branch left `main`: what a later pass's diff leaves out once it moved.
    #[serde(default)]
    pub main: String,
    /// The diff the pass reviews, as its brief gives it.
    #[serde(default)]
    pub diff: String,
    /// The files the branch changed against `main` at this head.
    #[serde(default)]
    pub paths: Vec<String>,
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
            require(
                folder(root, n - 1)?.join("record.json").exists(),
                &format!(
                    "Answer pass {} first: `dispatchdev review record --pass {}`.",
                    n - 1,
                    n - 1
                ),
            )?;
            Some(pinned(root, n - 1)?)
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
    let main = git.git(&["merge-base", "origin/main", "HEAD"])?;
    let changed = tier::numstat(&git.git(&[
        "diff",
        "--numstat",
        "--no-renames",
        &format!("{main}..HEAD"),
    ])?);
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
        (None, Some(previous)) if previous.asked => (previous.tier, true),
        (None, _) => (judged.tier, false),
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
        .map_or(main.clone(), |pass| pass.head.clone());
    let paths: Vec<String> = changed.iter().map(|file| file.path.clone()).collect();
    // After `main` was merged in, what changed since the last pass holds `main`'s own changes
    // too: the branch's own are the files it changed then or changes now, so a fix that put a
    // file back as `main` has it stays in.
    let diff = match &previous {
        Some(previous) if previous.main != main => {
            let mut own: Vec<&str> = previous.paths.iter().map(String::as_str).collect();
            for path in &paths {
                if !own.contains(&path.as_str()) {
                    own.push(path);
                }
            }
            format!("git diff {base}..{head} -- {}", own.join(" "))
        }
        _ => format!("git diff {base}..{head}"),
    };
    let security = tier == Tier::Sensitive;
    let pass = Pass {
        pass: n,
        head: head.clone(),
        base: base.clone(),
        main: main.clone(),
        diff,
        paths: paths.clone(),
        tier,
        reasons: judged.reasons,
        asked,
        security,
    };
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let fill = Fill {
        root,
        dir: &dir,
        branch: &branch,
        pass: &pass,
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
    dir: &'a Path,
    branch: &'a str,
    pass: &'a Pass,
    paths: &'a [&'a str],
    runner: &'a dyn Runner,
}
impl Fill<'_> {
    fn brief(&self, review: Review) -> Result<String> {
        let template = fs::read_to_string(self.root.join(review.template()))?;
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
                self.root
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

/// Whether a path is part of what someone sees: a page's code or styles, or an Android screen.
pub fn ui(path: &str) -> bool {
    path.contains("/frontend/")
        || path.ends_with(".css")
        || (path.starts_with("android/") && path.contains("/res/"))
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
    for review in reviews(&pass) {
        let found = dir.join(review.findings());
        let text = fs::read_to_string(&found).map_err(|_| {
            format!(
                "{} is missing: the reviewer hasn't answered.",
                found.display()
            )
        })?;
        let (findings, dropped) = findings::parse(&text)?;
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
    println!(
        "{}",
        workspace::next(&[
            format!(
                "Check each finding against the code and answer it in {}",
                dir.join("answers.md").display()
            ),
            "Commit the fixes and push them with `dispatchdev pr`".to_owned(),
            format!(
                "Then `dispatchdev review record --pass {n} --model <model>{}`",
                if pass.security {
                    " --security-model <model>"
                } else {
                    ""
                }
            ),
        ])
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
    let recorded = Recorded {
        model: models.model.to_owned(),
        security_model: models.security.map(str::to_owned),
        answers,
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
    let line = summary(pass.tier, &passes);
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
    let important_fixed = recorded
        .answers
        .iter()
        .any(|outcome| outcome.tag == "Important" && outcome.answer == "fixed");
    // Commits no answer accounts for were reviewed by no one: the pass is kept, but the review
    // isn't done at this head.
    let unreviewed = !changed && head != pass.head;
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
            "`dispatchdev pr` updates the PR's body; then `dispatchdev review prepare --pass {}` \
             reviews what fixing the Important findings changed",
            n + 1
        ),
        (true, false) => format!(
            "The review stops at pass {CAP}: `dispatchdev pr` updates the PR's body; merge once \
             its checks pass, unless more passes were asked for (--over-cap)"
        ),
        (false, _) => "The review is done: `dispatchdev pr` updates the PR's body; merge once \
                       its checks pass"
            .to_owned(),
    };
    println!("{}", workspace::next(&[next]));
    Ok(())
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
