//! `dispatchdev pr <name> --title <title> --body <file>`: the change's PR, opened or updated
//! once its title and body follow the rules: they become the squash commit on `main`, and
//! the release notes take its summary.
use crate::{
    REPOSITORY, Result, Runner, require,
    workspace::{self, Workspace},
};
use serde_json::Value;

const TYPES: &[&str] = &[
    "feat", "fix", "perf", "refactor", "test", "docs", "ci", "build", "chore",
];
const MOST: usize = 72;

/// The sections a body holds, in order, by the kind of change its title names. A change the
/// dashboard shows may end with Screenshots.
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
    let screenshots = headings.last() == Some(&"Screenshots");
    let listed = &headings[..headings.len() - usize::from(screenshots)];
    require(
        listed == wanted,
        &format!(
            "This body's sections are {}; a {} change's are {}{}.",
            if headings.is_empty() {
                "none".to_owned()
            } else {
                headings.join(", ")
            },
            title.split([':', '(']).next().unwrap_or(""),
            wanted.join(", "),
            if wanted[1] == "Problem" || wanted[1] == "Symptom" {
                ", then Screenshots if the dashboard shows it"
            } else {
                ""
            }
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

pub fn run(
    ws: &Workspace,
    name: &str,
    title: &str,
    body: &std::path::Path,
    runner: &dyn Runner,
) -> Result<()> {
    workspace::valid_name(name)?;
    let path = ws.worktree(name);
    require(path.is_dir(), &format!("worktrees/{name} doesn't exist."))?;
    let text =
        std::fs::read_to_string(body).map_err(|_| format!("Couldn't read {}.", body.display()))?;
    check_body(title, &text)?;
    let git = |args: &[&str]| workspace::text(runner, &[&["git"][..], args].concat(), Some(&path));
    require(
        git(&["status", "--porcelain"])?.is_empty(),
        "Commit the change before opening its PR.",
    )?;
    require(
        git(&["rev-parse", "--abbrev-ref", "HEAD"])? == name,
        &format!("worktrees/{name} isn't on its branch."),
    )?;
    git(&["push", "-q", "-u", "origin", name])?;
    let body = body.to_str().ok_or("Non-UTF8 path")?;
    let open: Vec<Value> = serde_json::from_str(&workspace::text(
        runner,
        &[
            "gh",
            "pr",
            "list",
            "--repo",
            REPOSITORY,
            "--head",
            name,
            "--state",
            "open",
            "--json",
            "number,url",
        ],
        None,
    )?)?;
    if let Some(pr) = open.first() {
        let number = pr["number"].to_string();
        workspace::text(
            runner,
            &[
                "gh",
                "pr",
                "edit",
                &number,
                "--repo",
                REPOSITORY,
                "--title",
                title,
                "--body-file",
                body,
            ],
            None,
        )?;
        println!("Updated #{number}: {}", pr["url"].as_str().unwrap_or(""));
    } else {
        let url = workspace::text(
            runner,
            &[
                "gh",
                "pr",
                "create",
                "--repo",
                REPOSITORY,
                "--base",
                "main",
                "--head",
                name,
                "--title",
                title,
                "--body-file",
                body,
            ],
            None,
        )?;
        println!("Opened {url}");
    }
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
    fn a_body_holds_its_kinds_sections_in_order_and_nothing_it_never_may() {
        let fix =
            "## Summary\n\nx\n\n## Symptom\n\n## Cause\n\n## Fix\n\n## Verification\n\n## Review\n";
        assert!(check_body("fix: x", fix).is_ok());
        assert!(check_body("fix: x", &format!("{fix}\n## Screenshots\n")).is_ok());
        let swapped = fix.replace("## Symptom\n\n## Cause", "## Cause\n\n## Symptom");
        assert!(check_body("fix: x", &swapped).is_err());
        assert!(check_body("chore: x", fix).is_err());
        for added in [
            "http://100.64.0.1:4101/__preview/abc",
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
