//! `dispatchdev pr`: the checkout's PR, opened or updated once its title and body follow the
//! rules: they become the squash commit on `main`, and the release notes take its Summary. Both
//! come from `pr-body.md` in the checkout's scratch folder, whose first line is `# <title>`;
//! `--skeleton --title` starts that file with the title and the sections its type needs, and
//! `--check` checks all of it, running `dispatchdev check` when it hasn't passed on the committed
//! tree, without pushing.
use crate::{REPOSITORY, Result, Runner, build::Environment, check, preview, require, workspace};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path};

const TYPES: &[&str] = &[
    "feat", "fix", "perf", "refactor", "test", "docs", "ci", "build", "chore",
];
const MOST: usize = 72;

/// The sections a body holds, in order, by the kind of change its title names.
pub fn sections(title: &str) -> Result<&'static [&'static str]> {
    let (head, summary) = title.split_once(": ").ok_or(
        "A title is type(scope): what it does, such as fix(dvic): keep the week when paging.",
    )?;
    let (kind, scope) = match head.split_once('(') {
        Some((kind, rest)) => (
            kind,
            Some(rest.strip_suffix(')').ok_or("A scope closes with ).")?),
        ),
        None => (head, None),
    };
    require(
        TYPES.contains(&kind),
        &format!("A title's type is one of {}.", TYPES.join(", ")),
    )?;
    if let Some(scope) = scope {
        require(
            !scope.is_empty()
                && scope
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "A scope is an owner's name, or host, ci, release, tooling or deps: lowercase, with hyphens.",
        )?;
    }
    require(
        title.chars().count() <= MOST,
        &format!(
            "A title is at most {MOST} characters; this one has {}.",
            title.chars().count()
        ),
    )?;
    require(
        !summary.trim().is_empty() && summary == summary.trim() && !summary.ends_with('.'),
        "A title says what the change does, without a trailing period.",
    )?;
    Ok(match (kind, scope) {
        ("feat", _) => &[
            "Summary",
            "Problem",
            "Change",
            "Risk",
            "Verification",
            "Review",
        ],
        ("fix" | "perf", _) => &[
            "Summary",
            "Symptom",
            "Cause",
            "Fix",
            "Risk",
            "Verification",
            "Review",
        ],
        ("ci", _) | (_, Some("host" | "release")) => {
            &["Summary", "Change", "Risk", "Verification", "Review"]
        }
        _ => &["Summary", "Why", "Change", "Risk", "Verification", "Review"],
    })
}
/// The text of the body's section `name`, trimmed, if it has one.
pub fn section<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let heading = format!("## {name}");
    let start = body
        .match_indices(&heading)
        .find(|(at, _)| {
            (*at == 0 || body[..*at].ends_with('\n'))
                && body[at + heading.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c == '\n')
        })?
        .0
        + heading.len();
    let rest = &body[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(rest[..end].trim())
}
/// The Review section's first line while the change's review is still to come: `review record`
/// replaces it, and `merge` treats it as not reviewed.
pub const PENDING: &str = "Under review";
/// Whether `line` opens a Review section: the line `review record` writes, `Tier light · not
/// reviewed` for a change that needs none, `Not reviewed:` and why for one merged without a
/// review, or `PENDING` until its review is recorded.
fn review_line(line: &str) -> bool {
    line == PENDING
        || line == "Tier light · not reviewed"
        || ["Tier standard · ", "Tier sensitive · "]
            .iter()
            .any(|start| line.starts_with(start))
        || line
            .strip_prefix("Not reviewed:")
            .is_some_and(|why| !why.trim().is_empty())
}
/// `body` with its Review section opening with `line`, in place of the line it opened with.
pub fn with_review_line(body: &str, line: &str) -> Result<String> {
    let heading = "\n## Review\n";
    let at = body
        .find(heading)
        .ok_or("The PR body has no ## Review section.")?
        + heading.len();
    let (before, rest) = body.split_at(at);
    let rest = rest.trim_start_matches('\n');
    let rest = match rest.lines().next() {
        Some(first) if review_line(first) => &rest[first.len()..],
        _ => rest,
    };
    let rest = rest.trim_start_matches('\n');
    Ok(match rest.is_empty() {
        true => format!("{before}\n{line}\n"),
        false => format!("{before}\n{line}\n\n{rest}"),
    })
}
/// What's wrong with a body for a change of `title`, if anything.
pub fn check_body(title: &str, body: &str) -> Result<()> {
    let wanted = sections(title)?;
    let headings: Vec<&str> = body
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(str::trim)
        .collect();
    require(
        headings == wanted,
        &format!(
            "This body's sections are {}; a {} change's are {}.",
            if headings.is_empty() {
                "none".to_owned()
            } else {
                headings.join(", ")
            },
            title.split([':', '(']).next().unwrap_or(""),
            wanted.join(", "),
        ),
    )?;
    let lower = body.to_lowercase();
    for (sign, rule) in [
        (
            "/__preview/",
            "The dev server's sign-in links stay in the chat, never on a PR.",
        ),
        ("<!--", "A body holds no HTML comments."),
        ("- [ ]", "A body holds no checklists."),
        ("- [x]", "A body holds no checklists."),
        (
            "co-authored-by",
            "Everything on GitHub is under the user's name: no co-author lines.",
        ),
        (
            "generated with",
            "Everything on GitHub is under the user's name: no generated-with lines.",
        ),
    ] {
        require(!lower.contains(sign), rule)?;
    }
    require(
        section(body, "Risk").is_some_and(|risk| !risk.is_empty()),
        "Risk says what could break and how to undo it.",
    )?;
    if wanted.contains(&"Cause") {
        let cause = section(body, "Cause").unwrap_or_default();
        let by = cause
            .lines()
            .find_map(|line| line.trim().strip_prefix("Caused by: "))
            .unwrap_or_default();
        require(
            by == "unknown"
                || by
                    .strip_prefix('#')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            "Cause names the PR that caused it on a line of its own: `Caused by: #<n>`, or \
             `Caused by: unknown`.",
        )?;
    }
    require(
        section(body, "Review")
            .and_then(|review| review.lines().next())
            .is_some_and(review_line),
        "Review opens with `Under review` until `dispatchdev review record` writes its line, \
         `Tier light · not reviewed`, or `Not reviewed:` and why.",
    )?;
    Ok(())
}

/// Writes the body a PR of `title` starts from to `path`, its sections' headings in order, and
/// answers them. It never writes over a body begun already.
pub fn skeleton(title: &str, path: &Path) -> Result<&'static [&'static str]> {
    let wanted = sections(title)?;
    let text: Vec<String> = std::iter::once(format!("# {title}\n"))
        .chain(wanted.iter().map(|name| match *name {
            "Review" => format!("## {name}\n\n{PENDING}\n"),
            _ => format!("## {name}\n"),
        }))
        .collect();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => format!(
                "{} exists already: write in it, or delete it to start again.",
                path.display()
            ),
            _ => format!("Couldn't write {}: {error}", path.display()),
        })?;
    file.write_all(text.join("\n").as_bytes())?;
    Ok(wanted)
}
/// A body's title, when its first line is `# <title>`, and the body after that line.
pub fn titled(text: &str) -> (Option<&str>, &str) {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    match first.strip_prefix("# ") {
        Some(title) => (Some(title.trim()), rest.trim_start_matches('\n')),
        None => (None, text),
    }
}

/// `dispatchdev pr --skeleton --title <title>`: the body's title and sections, in the checkout's
/// scratch folder, where `pr` reads it from.
pub fn start_body(root: &Path, title: &str) -> Result<()> {
    let body = workspace::checkout_scratch(root)?.join("pr-body.md");
    let wanted = skeleton(title, &body)?;
    println!("Wrote {}: {}.", body.display(), wanted.join(", "));
    println!(
        "{}",
        workspace::next(&["Write each section; then `dispatchdev pr` opens the PR.".to_owned()])
    );
    Ok(())
}

/// `dispatchdev pr`: publishes the checkout's PR, then says what's next.
pub fn run(root: &Path, only_check: bool, env: &Environment, runner: &dyn Runner) -> Result<()> {
    let Some(published) = publish(root, only_check, env, runner)? else {
        return Ok(());
    };
    let mut next = vec![format!(
        "Its checks run on GitHub ({}/checks): fix what they find and run `dispatchdev pr` again",
        published.url
    )];
    if published.pending {
        next.push(
            "`dispatchdev review prepare --pass 1` judges how deep its review goes".to_owned(),
        );
    }
    next.push("Once they pass and its review is done, `dispatchdev merge` merges it".to_owned());
    println!("{}", workspace::next(&next));
    Ok(())
}

/// A PR as `publish` left it.
pub struct Published {
    pub url: String,
    /// Whether its review is still to come: only then does it need starting.
    pending: bool,
}

/// Opens the checkout's PR, or updates the one open, from its scratch folder's `pr-body.md`,
/// once its title and body follow the rules and `dispatchdev check` passes on the committed
/// tree, which it runs unless it passed there already; with `only_check`, says whether they do
/// and pushes nothing.
pub fn publish(
    root: &Path,
    only_check: bool,
    env: &Environment,
    runner: &dyn Runner,
) -> Result<Option<Published>> {
    let body = workspace::checkout_scratch(root)?.join("pr-body.md");
    let text = std::fs::read_to_string(&body).map_err(|_| {
        format!(
            "{} is missing: `dispatchdev pr --skeleton --title \"<title>\"` starts it.",
            body.display()
        )
    })?;
    let (title, text) = titled(&text);
    let title = title.ok_or_else(|| format!("{} doesn't start with # <title>.", body.display()))?;
    check_body(title, text)?;
    let git = |args: &[&str]| {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(root))
    };
    let branch = git(&["branch", "--show-current"])?;
    require(
        !branch.is_empty() && branch != "main",
        "Open a PR from a change's own branch, not main.",
    )?;
    require(
        git(&["status", "--porcelain"])?.is_empty(),
        "Commit the change before opening its PR.",
    )?;
    match check::passed(root, env, runner)? {
        Some(true) => {}
        Some(false) => {
            check::here(root, "pr", runner)?;
            let plan = check::plan(root, runner)?;
            require(
                check::execute(root, &plan, env, runner)?,
                "A check failed: the PR waits for `dispatchdev check` to pass.",
            )?;
            // It passed on the tree it started with: a commit made while it ran isn't that one.
            require(
                check::passed(root, env, runner)? == Some(true),
                "The checkout changed while `dispatchdev check` ran: `dispatchdev pr` checks it \
                 as it is now.",
            )?;
        }
        None => {
            return Err(
                "This checkout keeps no record of `dispatchdev check` passing: unset \
                        CARGO_TARGET_DIR, RUSTFLAGS and CARGO_BUILD_*, then run it."
                    .into(),
            );
        }
    }
    let gh = |args: &[&str]| {
        let mut command = vec!["gh"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(root))
    };
    let open = gh(&[
        "pr",
        "list",
        "--repo",
        REPOSITORY,
        "--head",
        &branch,
        "--state",
        "open",
        "--json",
        "number,url",
        "--jq",
        ".[0] | select(.) | \"\\(.number) \\(.url)\"",
    ])?;
    // A change to what a page shows opens its PR only once the user approved how it looks.
    if open.is_empty() {
        match preview::approval(root, runner)? {
            preview::Approval::None | preview::Approval::Approved => {}
            preview::Approval::Missing => {
                return Err(
                    "This change alters what a page shows: show the user first with \
                            `dispatchdev preview`, and once they approve, `dispatchdev preview \
                            approve`."
                        .into(),
                );
            }
            preview::Approval::Changed(paths) => {
                return Err(format!(
                    "{} changed since the user approved the preview: show them again with \
                     `dispatchdev preview`.",
                    paths.join(", ")
                )
                .into());
            }
        }
    }
    if only_check {
        println!("The title, the body and `dispatchdev check` pass: `dispatchdev pr` pushes it.");
        return Ok(None);
    }
    git(&["push", "-q", "--force-with-lease", "-u", "origin", &branch])?;
    let url = match open.split_once(' ') {
        Some((number, url)) => {
            gh(&[
                "pr", "edit", number, "--repo", REPOSITORY, "--title", title, "--body", text,
            ])?;
            println!("Updated #{number}: {url}");
            url.to_owned()
        }
        None => {
            let url = gh(&[
                "pr", "create", "--repo", REPOSITORY, "--base", "main", "--head", &branch,
                "--title", title, "--body", text,
            ])?;
            println!("Opened {url}");
            url
        }
    };
    // A recorded or skipped review is done.
    let pending = section(text, "Review").and_then(|review| review.lines().next()) == Some(PENDING);
    Ok(Some(Published { url, pending }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_title_names_its_type_scope_and_change_within_72_characters() {
        assert_eq!(
            sections("fix(dvic): keep the week when paging").unwrap()[3],
            "Fix"
        );
        assert_eq!(
            sections("feat(timecard): add a total row").unwrap()[3],
            "Risk"
        );
        assert_eq!(
            sections("ci: run the browser shards in parallel").unwrap()[2],
            "Risk"
        );
        assert_eq!(
            sections("chore(host): rotate the backup key").unwrap()[2],
            "Risk"
        );
        assert_eq!(
            sections("refactor(tooling): rebuild dispatch-ci").unwrap()[1],
            "Why"
        );
        for bad in [
            "keep the week when paging",
            "fixed(dvic): keep the week",
            "fix(DVIC): keep the week",
            "fix(dvic: keep the week",
            "fix(dvic): keep the week when paging.",
            "fix(dvic):  keep the week",
            &format!("fix: {}", "a".repeat(68)),
        ] {
            assert!(sections(bad).is_err(), "{bad}");
        }
        assert!(sections(&format!("fix: {}", "a".repeat(67))).is_ok());
    }
    #[test]
    fn a_skeleton_never_writes_over_a_body_begun_already() {
        let temp = tempfile::tempdir().unwrap();
        let body = temp.path().join("pr-body.md");
        skeleton("fix(dvic): keep the week when paging", &body).unwrap();
        let begun = std::fs::read_to_string(&body).unwrap();
        assert!(begun.starts_with("# fix(dvic): keep the week when paging\n\n## Summary\n"));
        assert!(
            begun.ends_with("## Review\n\nUnder review\n"),
            "a review to come: {begun}"
        );
        assert_eq!(
            titled(&begun),
            (
                Some("fix(dvic): keep the week when paging"),
                begun.split_once("\n\n").unwrap().1
            )
        );
        assert_eq!(titled("## Summary\n"), (None, "## Summary\n"));
        std::fs::write(&body, "## Summary\n\nKept the week.\n").unwrap();
        let refused = skeleton("fix(dvic): keep the week when paging", &body).unwrap_err();
        assert!(refused.to_string().contains("exists already"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(&body).unwrap(),
            "## Summary\n\nKept the week.\n"
        );
    }
    #[test]
    fn a_body_holds_its_kinds_sections_in_order_and_nothing_it_never_may() {
        let fix = "## Summary\n\nx\n\n## Symptom\n\n## Cause\n\nCaused by: #812\n\n## Fix\n\n\
                   ## Risk\n\nUndo: revert it.\n\n## Verification\n\n## Review\n\nNot reviewed: main failed\n";
        assert!(check_body("fix: x", fix).is_ok());
        assert!(check_body("fix: x", &format!("{fix}\n## Screenshots\n")).is_err());
        let swapped = fix.replace("## Symptom\n\n## Cause", "## Cause\n\n## Symptom");
        assert!(check_body("fix: x", &swapped).is_err());
        assert!(check_body("chore: x", fix).is_err());
        for added in [
            "http://localhost:4101/__preview/abc",
            "<!-- note -->",
            "- [ ] screenshots",
            "Co-Authored-By: someone",
            "Generated with a tool",
        ] {
            assert!(
                check_body("fix: x", &format!("{fix}{added}\n")).is_err(),
                "{added}"
            );
        }
        for (from, to) in [
            ("Caused by: #812", "Caused by: unknown"),
            ("Not reviewed: main failed", "Under review"),
            ("Not reviewed: main failed", "Tier light · not reviewed"),
            (
                "Not reviewed: main failed",
                "Tier standard · 1 pass · gpt-6.1-sol · Important 0 · Nit 0 · Pre-existing 0",
            ),
        ] {
            assert!(check_body("fix: x", &fix.replace(from, to)).is_ok(), "{to}");
        }
        for (from, to) in [
            ("Caused by: #812", "Caused by #812"),
            ("Caused by: #812", "Caused by: a refactor"),
            ("Undo: revert it.", ""),
            ("Not reviewed: main failed", "Not reviewed:"),
            ("Not reviewed: main failed", "Reviewed twice."),
        ] {
            assert!(
                check_body("fix: x", &fix.replace(from, to)).is_err(),
                "{to}"
            );
        }
    }
    #[test]
    fn the_review_line_replaces_the_one_the_section_opened_with() {
        let body = "## Risk\n\nx\n\n## Review\n\nNot reviewed: yet\n\nPass 1 found the DSP.\n";
        let line = "Tier standard · 1 pass · m · Important 1 (fixed 1) · Nit 0 · Pre-existing 0";
        let written = with_review_line(body, line).unwrap();
        assert_eq!(
            written,
            format!("## Risk\n\nx\n\n## Review\n\n{line}\n\nPass 1 found the DSP.\n")
        );
        assert_eq!(with_review_line(&written, line).unwrap(), written);
        assert_eq!(section(&written, "Risk"), Some("x"));
        assert!(with_review_line("## Risk\n\nx\n", line).is_err());
    }
}
