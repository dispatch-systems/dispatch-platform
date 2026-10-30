//! `dispatchdev pr`: the checkout's PR, opened or updated once its title and body follow the
//! rules: they become the squash commit on `main`, and the release notes take its Summary. Both
//! come from `pr-body.md` in the checkout's scratch folder, whose first line is `# <title>`;
//! `--skeleton --title` starts that file with the title and the sections its type needs.
use crate::{REPOSITORY, Result, Runner, require, workspace};
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
            "Rollout",
            "Verification",
            "Review",
        ],
        ("fix" | "perf", _) => &[
            "Summary",
            "Symptom",
            "Cause",
            "Fix",
            "Verification",
            "Review",
        ],
        ("ci", _) | (_, Some("host" | "release")) => {
            &["Summary", "Change", "Risk", "Verification", "Review"]
        }
        _ => &["Summary", "Why", "Change", "Verification", "Review"],
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
    Ok(())
}

/// Writes the body a PR of `title` starts from to `path`, its sections' headings in order, and
/// answers them. It never writes over a body begun already.
pub fn skeleton(title: &str, path: &Path) -> Result<&'static [&'static str]> {
    let wanted = sections(title)?;
    let text: Vec<String> = std::iter::once(format!("# {title}\n"))
        .chain(wanted.iter().map(|name| format!("## {name}\n")))
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

/// Opens the checkout's PR, or updates the one open, from its scratch folder's `pr-body.md`.
pub fn run(root: &Path, runner: &dyn Runner) -> Result<()> {
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
    git(&["push", "-q", "--force-with-lease", "-u", "origin", &branch])?;
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
    let (number, url) = match open.split_once(' ') {
        Some((number, url)) => {
            gh(&[
                "pr", "edit", number, "--repo", REPOSITORY, "--title", title, "--body", text,
            ])?;
            println!("Updated #{number}: {url}");
            (number.to_owned(), url.to_owned())
        }
        None => {
            let url = gh(&[
                "pr", "create", "--repo", REPOSITORY, "--base", "main", "--head", &branch,
                "--title", title, "--body", text,
            ])?;
            println!("Opened {url}");
            let number = url.rsplit('/').next().unwrap_or_default().to_owned();
            (number, url)
        }
    };
    println!(
        "{}",
        workspace::next(&[
            format!(
                "Its checks run on GitHub ({url}/checks): fix what they find and run `dispatchdev pr` again"
            ),
            format!("Once they pass, `gh pr merge {number} --squash` merges it"),
        ])
    );
    Ok(())
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
            "Rollout"
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
        let fix =
            "## Summary\n\nx\n\n## Symptom\n\n## Cause\n\n## Fix\n\n## Verification\n\n## Review\n";
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
    }
}
