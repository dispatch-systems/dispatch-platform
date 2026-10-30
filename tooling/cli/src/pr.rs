//! `dispatchdev pr <name>`: the change's PR, opened or updated once its title and body follow
//! the rules: they become the squash commit on `main`, and the release notes take its summary.
//! Both come from the change's scratch folder's `pr-body.md`, whose first line is `# <title>`,
//! unless `--body` names another file or `--title` gives the title. `--skeleton --title`
//! starts that file with its title and the sections the title's type needs.
use crate::{
    REPOSITORY, Result, Runner,
    change::Change,
    require,
    workspace::{self, Workspace},
};
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
            "Preview links stay in the chat, never on a PR.",
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
/// Where a change's PR body is written, its title on the first line.
pub fn body_file(change: &Change) -> std::path::PathBuf {
    change.scratch().join("pr-body.md")
}
/// A body's title, when its first line is `# <title>`, and the body after that line.
pub fn titled(text: &str) -> (Option<&str>, &str) {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    match first.strip_prefix("# ") {
        Some(title) => (Some(title.trim()), rest.trim_start_matches('\n')),
        None => (None, text),
    }
}

/// `dispatchdev pr <name> --skeleton --title <title>`: the body's title and sections, in the
/// change's scratch folder, where `pr` reads it from.
pub fn start_body(ws: &Workspace, name: &str, title: &str) -> Result<()> {
    let change = Change::existing(ws, name)?;
    change.scratch_dir()?;
    let body = body_file(&change);
    let wanted = skeleton(title, &body)?;
    println!("Wrote {}: {}.", body.display(), wanted.join(", "));
    println!(
        "{}",
        workspace::next(&[format!(
            "Write each section; then `dispatchdev pr {name}` opens the PR."
        )])
    );
    Ok(())
}

/// Opens the change's PR, or updates the one open, from the body in `body` and `title` or the
/// body's own; returns its number.
pub fn run(
    ws: &Workspace,
    name: &str,
    title: Option<&str>,
    body: &std::path::Path,
    runner: &dyn Runner,
) -> Result<u64> {
    let change = Change::existing(ws, name)?;
    let text =
        std::fs::read_to_string(body).map_err(|_| format!("Couldn't read {}.", body.display()))?;
    let (own, text) = titled(&text);
    let title = title.or(own).ok_or_else(|| {
        format!(
            "{} has no title: start it with a line # <title>, or give --title.",
            body.display()
        )
    })?;
    check_body(title, text)?;
    require(
        !change.dirty(runner)?,
        "Commit the change before opening its PR.",
    )?;
    require(
        change.branch(runner)? == name,
        &format!("worktrees/{name} isn't on its branch."),
    )?;
    change.git(
        runner,
        &["push", "-q", "--force-with-lease", "-u", "origin", name],
    )?;
    if let Some(pr) = change.pull(runner)?.filter(|pr| pr.open()) {
        let number = pr.number.to_string();
        workspace::text(
            runner,
            &[
                "gh", "pr", "edit", &number, "--repo", REPOSITORY, "--title", title, "--body", text,
            ],
            None,
        )?;
        println!("Updated #{number}: {}", pr.url);
        Ok(pr.number)
    } else {
        let url = workspace::text(
            runner,
            &[
                "gh", "pr", "create", "--repo", REPOSITORY, "--base", "main", "--head", name,
                "--title", title, "--body", text,
            ],
            None,
        )?;
        println!("Opened {url}");
        url.rsplit('/')
            .next()
            .and_then(|number| number.parse().ok())
            .ok_or_else(|| format!("GitHub opened the PR at {url}, without its number.").into())
    }
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
            "http://preview.test:4101/__preview/abc",
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
