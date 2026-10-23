//! `dispatchdev review <name>`: a review pass on a change, by a reviewer the agent starts
//! through T3 Code with the brief this writes. Each pass reviews the change's committed head,
//! read-only, against what the owner approved and `tooling/review/rules.md`, and writes its
//! findings to its own folder under the change's scratch folder; the next pass is given the
//! earlier ones. `--done` then checks the reviewer left the worktree as it found it.
use crate::{
    Result, Runner,
    change::Change,
    require,
    workspace::{self, Workspace},
};
use std::{fs, path::Path};

/// The reviewer, as T3 Code's `delegate_task` takes it: the provider instance, its model and
/// the model's options. `priority` is the Fast service tier.
pub const PROVIDER: &str = "codex";
pub const MODEL: &str = "gpt-6-astra";
pub const OPTIONS: &str = r#"{"reasoningEffort":"xhigh","serviceTier":"priority"}"#;

/// What a pass is given, all as paths the reviewer reads itself.
struct Brief<'a> {
    name: &'a str,
    worktree: &'a Path,
    head: &'a str,
    base: &'a str,
    findings: &'a Path,
    decisions: Option<&'a Path>,
    body: Option<&'a Path>,
    /// Each earlier pass: the head it reviewed and its findings.
    earlier: &'a [(String, std::path::PathBuf)],
}
impl Brief<'_> {
    fn text(&self) -> String {
        let Brief {
            name,
            worktree,
            head,
            base,
            ..
        } = self;
        let mut lines = vec![
            format!("# Review of {name} at {head}"),
            String::new(),
            format!(
                "Review a change to Dispatch, read-only. Its worktree is {}, at commit {head} on \
                 branch {name}; the change is what `git diff {base}...{head}` shows there.",
                worktree.display()
            ),
            String::new(),
            format!(
                "- Edit, create and delete nothing in the worktree, and commit, push or write to \
                 GitHub nothing: no comments, reviews or labels. The one file you write is {}.",
                self.findings.display()
            ),
            "- Hold the change to `tooling/review/rules.md` in the worktree.".to_owned(),
        ];
        if let Some(decisions) = self.decisions {
            lines.push(format!(
                "- What the owner approved is in {}. A finding that would change it goes under \
                 Out of scope.",
                decisions.display()
            ));
        }
        if let Some(body) = self.body {
            lines.push(format!(
                "- The PR's body, as written so far: {}.",
                body.display()
            ));
        }
        for (pass, (reviewed, findings)) in self.earlier.iter().enumerate() {
            lines.push(format!(
                "- Pass {} reviewed {reviewed} and found {}; `git log {reviewed}..{head}` shows \
                 what changed since, and the PR body's Review section what was set aside and why. \
                 Say whether each of its findings was settled, and review what changed afresh.",
                pass + 1,
                findings.display()
            ));
        }
        lines.extend(
            [
                "",
                "Write your findings to that file, most important first, and answer with the same:",
                "",
                "## Fix: <path>:<line> <what is wrong, in a sentence>",
                "<How it fails: the inputs or state, and the wrong result.> <What to change.>",
                "",
                "## Suggestion: <path>:<line> <what would be better, in a sentence>",
                "<Why, and what to change.>",
                "",
                "## Out of scope: <path>:<line> <what, in a sentence>",
                "<Why it matters.>",
                "",
                "A fix is a defect: wrong behaviour, a broken rule, a security or privacy problem, or \
                 new behaviour without a test. A suggestion is anything else worth doing within the \
                 change. Check each against the code before writing it, and leave out what you can't \
                 show. With nothing to report, write `No findings.` alone.",
            ]
            .map(str::to_owned),
        );
        lines.join("\n") + "\n"
    }
}

/// How many fixes and suggestions a pass's findings hold.
fn count(findings: &str) -> (usize, usize) {
    let heads = |kind: &str| {
        findings
            .lines()
            .filter(|line| line.starts_with(&format!("## {kind}:")))
            .count()
    };
    (heads("Fix"), heads("Suggestion"))
}

/// The passes that are planned, and each pass's folder, in order: `review/passes` and
/// `review/<n>/` in the change's scratch folder.
struct Passes {
    folder: std::path::PathBuf,
}
impl Passes {
    fn of(change: &Change) -> Self {
        Self {
            folder: change.scratch().join("review"),
        }
    }
    fn planned(&self) -> u32 {
        fs::read_to_string(self.folder.join("passes"))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(1)
    }
    /// The passes started so far.
    fn started(&self) -> u32 {
        (1..)
            .take_while(|pass| self.pass(*pass).is_dir())
            .last()
            .unwrap_or(0)
    }
    fn pass(&self, pass: u32) -> std::path::PathBuf {
        self.folder.join(pass.to_string())
    }
}

/// `dispatchdev review <name> [--passes <n>]`: the brief of the change's next pass. `passes`
/// sets how many there are to be, one unless told.
pub fn start(ws: &Workspace, name: &str, passes: Option<u32>, runner: &dyn Runner) -> Result<()> {
    let change = Change::existing(ws, name)?;
    require(
        !change.dirty(runner)?,
        "Commit the change before reviewing it: a pass reviews its committed head.",
    )?;
    require(
        change.branch(runner)? == name,
        &format!("worktrees/{name} isn't on its branch."),
    )?;
    let reviews = Passes::of(&change);
    let started = reviews.started();
    if started > 0 {
        require(
            reviews.pass(started).join("findings.md").is_file(),
            &format!(
                "Pass {started} has no findings yet: wait for its reviewer, then run dispatchdev \
                 review {name} --done."
            ),
        )?;
    }
    if let Some(passes) = passes {
        workspace::write_private(&reviews.folder.join("passes"), &format!("{passes}\n"))?;
    }
    let planned = reviews.planned();
    let pass = started + 1;
    require(
        pass <= planned,
        &format!("All {planned} of {name}'s passes are done; --passes asks for more, or ship it."),
    )?;
    let head = change.git(runner, &["rev-parse", "HEAD"])?;
    let base = change.git(runner, &["merge-base", "origin/main", "HEAD"])?;
    let folder = reviews.pass(pass);
    let earlier: Vec<_> = (1..pass)
        .map(|earlier| {
            let folder = reviews.pass(earlier);
            let reviewed = fs::read_to_string(folder.join("head")).unwrap_or_default();
            (reviewed.trim().to_owned(), folder.join("findings.md"))
        })
        .collect();
    let (decisions, body) = (
        change.scratch().join("decisions.md"),
        change.scratch().join("pr-body.md"),
    );
    let brief = Brief {
        name,
        worktree: &change.path,
        head: &head,
        base: &base,
        findings: &folder.join("findings.md"),
        decisions: decisions.is_file().then_some(decisions.as_path()),
        body: body.is_file().then_some(body.as_path()),
        earlier: &earlier,
    };
    workspace::write_private(&folder.join("head"), &format!("{head}\n"))?;
    workspace::write_private(&folder.join("brief.md"), &brief.text())?;
    println!(
        "Pass {pass} of {planned} on {}: its brief is {}.",
        &head[..head.len().min(8)],
        folder.join("brief.md").display()
    );
    println!(
        "{}",
        workspace::next(&[
            format!(
                "Start its reviewer in the background with T3's delegate_task: provider {PROVIDER}, \
                 model {MODEL}, options {OPTIONS}, the brief as its task."
            ),
            format!("Once it answers, `dispatchdev review {name} --done`."),
        ])
    );
    Ok(())
}

/// `dispatchdev review <name> --done`: the latest pass, once its reviewer answered: the
/// worktree must be as the pass found it, or the pass doesn't count and goes.
pub fn done(ws: &Workspace, name: &str, runner: &dyn Runner) -> Result<()> {
    let change = Change::existing(ws, name)?;
    let reviews = Passes::of(&change);
    let pass = reviews.started();
    require(
        pass > 0,
        &format!("{name} has no review pass; start one with dispatchdev review {name}."),
    )?;
    let folder = reviews.pass(pass);
    let reviewed = fs::read_to_string(folder.join("head"))?;
    let head = change.git(runner, &["rev-parse", "HEAD"])?;
    if head != reviewed.trim() || change.dirty(runner)? {
        fs::remove_dir_all(&folder)?;
        return Err(format!(
            "worktrees/{name} changed during pass {pass}, so the pass doesn't count and is gone. \
             Put the worktree back at {} with nothing uncommitted, then run the pass again.",
            reviewed.trim()
        )
        .into());
    }
    let findings = fs::read_to_string(folder.join("findings.md")).map_err(|_| {
        format!(
            "Pass {pass}'s reviewer wrote no {}; write its answer there.",
            folder.join("findings.md").display()
        )
    })?;
    let (fixes, suggestions) = count(&findings);
    let planned = reviews.planned();
    println!(
        "Pass {pass} of {planned}: {}.",
        match (fixes, suggestions) {
            (0, 0) => "no findings".to_owned(),
            _ => format!("{fixes} to fix, {suggestions} suggested"),
        }
    );
    println!(
        "{}",
        workspace::next(&[if pass < planned && fixes + suggestions > 0 {
            format!(
                "Make them, commit, and run `dispatchdev review {name}` for pass {}.",
                pass + 1
            )
        } else {
            "Make them, commit, write the Review section, and ship it.".to_owned()
        }])
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_brief_pins_the_head_and_names_what_the_reviewer_reads() {
        let earlier = [(
            "1111111".to_owned(),
            PathBuf::from("/tmp/dispatch-fix-week/review/1/findings.md"),
        )];
        let brief = Brief {
            name: "fix-week",
            worktree: Path::new("/workspace/worktrees/fix-week"),
            head: "2222222",
            base: "0000000",
            findings: Path::new("/tmp/dispatch-fix-week/review/2/findings.md"),
            decisions: Some(Path::new("/tmp/dispatch-fix-week/decisions.md")),
            body: None,
            earlier: &earlier,
        }
        .text();
        assert!(brief.starts_with("# Review of fix-week at 2222222\n"));
        assert!(brief.contains("`git diff 0000000...2222222`"));
        assert!(
            brief
                .contains("The one file you write is /tmp/dispatch-fix-week/review/2/findings.md.")
        );
        assert!(brief.contains("/tmp/dispatch-fix-week/decisions.md"));
        assert!(!brief.contains("The PR's body"));
        assert!(brief.contains("Pass 1 reviewed 1111111"));
        assert!(brief.contains("`git log 1111111..2222222`"));
    }

    #[test]
    fn findings_count_their_fixes_and_suggestions() {
        let findings = "## Fix: a.rs:3 x\nbecause\n\n## Suggestion: b.rs:9 y\n\n## Fix: c.rs:1 z\n\
            ## Out of scope: d.rs:2 w\n";
        assert_eq!(count(findings), (2, 1));
        assert_eq!(count("No findings.\n"), (0, 0));
    }
}
