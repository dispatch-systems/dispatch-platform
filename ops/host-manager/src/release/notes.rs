use super::Release;
use crate::{REPOSITORY, Result};
use serde_json::Value;
use std::{collections::BTreeMap, fs};

/// The PRs in the log, as number and title: a squash commit names its PR in the `(#N)`
/// suffix of its subject, and a merge commit from before squash merges in its
/// `Merge pull request #N` subject, with the title on its body's first line.
fn merged_changes(log: &str) -> Result<Vec<(u64, String)>> {
    let squash = regex::Regex::new(r"^(.*) \(#(\d+)\)$")?;
    let merge = regex::Regex::new(r"^Merge pull request #(\d+) from ")?;
    Ok(log
        .split('\u{1e}')
        .filter_map(|entry| {
            let (subject, body) = entry.trim().split_once('\n').unwrap_or((entry.trim(), ""));
            if let Some(c) = squash.captures(subject) {
                return Some((c[2].parse().ok()?, c[1].to_owned()));
            }
            let c = merge.captures(subject)?;
            Some((
                c[1].parse().ok()?,
                body.trim().lines().next().unwrap_or("").trim().to_owned(),
            ))
        })
        .collect())
}

/// A PR title's `type(scope):` prefix, capturing its type and scope.
fn prefix() -> regex::Regex {
    regex::Regex::new(r"^([a-z]+)(?:\(([^)]*)\))?!?: ").unwrap()
}
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
/// A PR title as a release note reads it: without the `type(scope):` prefix, capitalized.
fn plain_title(title: &str) -> String {
    capitalized(&prefix().replace(title, ""))
}

/// Where a PR goes in the release notes, in their order.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Area {
    /// Under the heading its scope names.
    Scope(String),
    /// Under the general heading: a PR with no scope.
    General,
    /// Only counted: a change to tooling, CI, tests, dependencies or how the code is arranged,
    /// or to the release and host tools.
    Counted,
}
const COUNTED_TYPES: [&str; 7] = ["ci", "test", "build", "chore", "docs", "refactor", "ops"];
const COUNTED_SCOPES: [&str; 5] = ["tooling", "ci", "deps", "release", "host"];
const COUNTED: &str = "tooling, CI, tests, dependencies and code structure";
/// A PR's area, from its title's `type(scope):` prefix.
fn area(title: &str) -> Area {
    let Some(prefix) = prefix().captures(title) else {
        return Area::General;
    };
    let scope = prefix.get(2).map(|scope| scope.as_str());
    if COUNTED_TYPES.contains(&&prefix[1]) || scope.is_some_and(|s| COUNTED_SCOPES.contains(&s)) {
        return Area::Counted;
    }
    scope.map_or(Area::General, |scope| Area::Scope(scope.into()))
}
/// A scope as a heading, in words: `driver-match` reads Driver Match, an acronym in capitals.
fn heading(scope: &str) -> String {
    scope
        .split('-')
        .map(|word| match word {
            "dvic" | "mcp" => word.to_uppercase(),
            _ => capitalized(word),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The release notes for the merged PRs, as number, title and the body GitHub served: `app`,
/// the line naming DSP Dispatch: Relay's new version, first; then a heading per area, scopes
/// in order before the general one, with a line per PR, the first paragraph of its Summary
/// section (or of Change, for older PRs) or else its title; then the count of changes to
/// tooling, CI, tests, dependencies and code structure; then the list of every PR.
fn release_notes(
    app: Option<String>,
    pulls: &[(u64, String, Option<String>)],
    tag: &str,
) -> String {
    let mut areas = BTreeMap::<Area, Vec<String>>::new();
    for (_, title, body) in pulls {
        let line = match body.as_deref().and_then(change_summary) {
            Some(summary) => summary,
            None => format!("{}.", plain_title(title).trim_end_matches('.')),
        };
        areas.entry(area(title)).or_default().push(line);
    }
    let section = |name: &str, lines: &[String]| {
        let lines: Vec<_> = lines.iter().map(|line| format!("- {line}")).collect();
        format!("## {name}\n\n{}", lines.join("\n"))
    };
    let mut blocks: Vec<String> = app.into_iter().collect();
    blocks.extend(areas.into_iter().map(|(area, lines)| match area {
        Area::Scope(scope) => section(&heading(&scope), &lines),
        Area::General => section("General", &lines),
        Area::Counted => match lines.len() {
            1 => format!("Plus 1 change to {COUNTED}."),
            n => format!("Plus {n} changes to {COUNTED}."),
        },
    }));
    let links: Vec<_> = pulls
        .iter()
        .map(|(number, ..)| format!("[#{number}](https://github.com/{REPOSITORY}/pull/{number})"))
        .collect();
    blocks.push(match links.len() {
        0 => format!("Release {tag}."),
        1 => format!("Includes {}.", links[0]),
        n => format!(
            "Includes {} and {}.",
            links[..n - 1].join(", "),
            links[n - 1]
        ),
    });
    blocks.join("\n\n") + "\n"
}

/// The first paragraph of a PR body's Summary section, or of its Change section for a body
/// written before the templates had one. Headings and plain section names both count.
fn change_summary(body: &str) -> Option<String> {
    section_paragraph(body, "Summary").or_else(|| section_paragraph(body, "Change"))
}
const SECTIONS: [&str; 11] = [
    "Summary",
    "Problem",
    "Symptom",
    "Cause",
    "Fix",
    "Change",
    "Why",
    "Rollout",
    "Risk",
    "Verification",
    "Review",
];
fn section_paragraph(body: &str, name: &str) -> Option<String> {
    let heading = |line: &str| line.trim_start_matches('#').trim().to_owned();
    let mut lines = body
        .lines()
        .map(str::trim)
        .skip_while(|line| heading(line) != name)
        .skip(1)
        .skip_while(|line| line.is_empty())
        .take_while(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !SECTIONS.contains(&heading(line).as_str())
                && !line.starts_with("Review:")
                && !line.starts_with("Docs:")
        })
        .map(|line| line.trim_start_matches("- ").trim());
    let first = lines.next()?;
    Some(
        std::iter::once(first)
            .chain(lines)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

impl Release<'_> {
    /// The PRs on main's first-parent line after `previous` up to `commit`.
    pub(super) fn changes_after(&self, previous: &str, commit: &str) -> Result<Vec<(u64, String)>> {
        merged_changes(&self.git(
            &[
                "log",
                "--first-parent",
                "--format=%s%n%b%x1e",
                &format!("{previous}..{commit}"),
            ],
            None,
            120,
        )?)
    }
    /// Writes the release notes from the PRs merged since the last release, with the title and
    /// body GitHub serves for each. A PR GitHub doesn't serve keeps the title its commit names.
    pub(super) fn generate_notes(&self, commit: &str) -> Result<()> {
        let changes = match self.previous()? {
            Some(previous) => self.changes_after(&previous, commit)?,
            None => vec![],
        };
        let numbers: Vec<_> = changes.iter().map(|(number, _)| *number).collect();
        let served = self.pull_requests(&numbers)?;
        let pulls: Vec<_> = changes
            .into_iter()
            .map(|(number, title)| match served.get(&number) {
                Some((title, body)) => (number, title.clone(), Some(body.clone())),
                None => (number, title, None),
            })
            .collect();
        fs::write(
            &self.notes,
            release_notes(self.app_note()?, &pulls, &self.tag),
        )?;
        Ok(())
    }
    /// Each PR's title and body, by number, a hundred PRs to a query. A query GitHub fails,
    /// such as one naming a number that isn't a PR, leaves its PRs out.
    fn pull_requests(&self, numbers: &[u64]) -> Result<BTreeMap<u64, (String, String)>> {
        let (owner, name) = REPOSITORY.split_once('/').ok_or("Invalid repository")?;
        let mut pulls = BTreeMap::new();
        for batch in numbers.chunks(100) {
            let fields: Vec<_> = batch
                .iter()
                .map(|number| format!("p{number}: pullRequest(number: {number}) {{ title body }}"))
                .collect();
            let query = format!(
                "query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ {} }} }}",
                fields.join(" ")
            );
            let Some(answer) = self
                .command(
                    &[
                        "gh",
                        "api",
                        "graphql",
                        "-f",
                        &format!("query={query}"),
                        "-f",
                        &format!("owner={owner}"),
                        "-f",
                        &format!("name={name}"),
                    ],
                    None,
                    120,
                )
                .ok()
                .and_then(|answer| serde_json::from_str::<Value>(&answer).ok())
            else {
                continue;
            };
            for number in batch {
                let pull = &answer["data"]["repository"][format!("p{number}")];
                if let (Some(title), Some(body)) = (pull["title"].as_str(), pull["body"].as_str()) {
                    pulls.insert(*number, (title.to_owned(), body.to_owned()));
                }
            }
        }
        Ok(pulls)
    }
}

#[cfg(test)]
mod tests {
    use super::{change_summary, merged_changes, plain_title, release_notes};
    use crate::REPOSITORY;
    #[test]
    fn notes_list_squash_and_merge_commits_of_prs() {
        let log = "fix(host): keep the lock (#80)\n\nThe body.\n\u{1e}\nMerge pull request #76 from owner/fix\n\nFix account\n\u{1e}\nMerge dev\n\u{1e}\nRelease notes\n\u{1e}\nMerge pull request #74 from owner/other\n\u{1e}";
        assert_eq!(
            merged_changes(log).unwrap(),
            [
                (80, "fix(host): keep the lock".to_owned()),
                (76, "Fix account".to_owned()),
                (74, String::new())
            ]
        );
    }
    #[test]
    fn notes_read_titles_and_the_summary_or_the_first_paragraph_of_a_change_section() {
        assert_eq!(plain_title("fix(host): keep the lock"), "Keep the lock");
        assert_eq!(plain_title("ci!: rebuild"), "Rebuild");
        assert_eq!(plain_title("Sort every column"), "Sort every column");
        let body = "Problem\n\nIt broke.\n\nChange\n\nThe lock is held explicitly.\nIt unlocks on drop.\n\nMore detail.\n\nVerification\n\nTests.\n";
        assert_eq!(
            change_summary(body).as_deref(),
            Some("The lock is held explicitly. It unlocks on drop.")
        );
        assert_eq!(
            change_summary("## Change\n\n- one\n- two\n").as_deref(),
            Some("one two")
        );
        assert_eq!(change_summary("Problem\n\nNo change section.\n"), None);
        assert_eq!(change_summary("Change\n\nVerification\n\nx"), None);
        let templated = "## Summary\n\nOwners switch a page per DSP.\n\n## Problem\n\nEvery DSP got every page.\n\n## Change\n\n- A catalog.\n";
        assert_eq!(
            change_summary(templated).as_deref(),
            Some("Owners switch a page per DSP.")
        );
        assert_eq!(change_summary("## Summary\n\n## Symptom\n\nx"), None);
        assert_eq!(
            change_summary("## Summary\n\n## Change\n\nFrom change.\n").as_deref(),
            Some("From change.")
        );
    }
    #[test]
    fn notes_group_prs_by_area_and_count_tooling_ci_and_test_changes() {
        let summary = |text: &str| Some(format!("## Summary\n\n{text}\n\n## Change\n\nMore.\n"));
        let pulls = [
            (
                10,
                "refactor(communications): split the composer".into(),
                None,
            ),
            (9, "ops: run Dev from its own checkout".into(), None),
            (8, "fix(host): keep the lock".into(), None),
            (7, "feat(driver-match): match drivers by phone".into(), None),
            (
                6,
                "ci: cache the build".into(),
                summary("CI caches the build."),
            ),
            (5, "fix(communications): keep a reply's draft".into(), None),
            (4, "feat: one record of every driver".into(), None),
            (3, "test(communications): wait on the reply".into(), None),
            (2, "fix(dvic): open on the latest day".into(), None),
            (
                1,
                "feat(communications): list waiting conversations first".into(),
                summary("Recents lists waiting conversations first."),
            ),
        ];
        let link = |n: u64| format!("[#{n}](https://github.com/{REPOSITORY}/pull/{n})");
        assert_eq!(
            release_notes(
                Some("- DSP Dispatch: Relay 0.0.2.".into()),
                &pulls,
                "v1.0.0"
            ),
            format!(
                "- DSP Dispatch: Relay 0.0.2.\n\n\
                 ## Communications\n\n- Keep a reply's draft.\n- Recents lists waiting conversations first.\n\n\
                 ## Driver Match\n\n- Match drivers by phone.\n\n\
                 ## DVIC\n\n- Open on the latest day.\n\n\
                 ## General\n\n- One record of every driver.\n\n\
                 Plus 5 changes to tooling, CI, tests, dependencies and code structure.\n\n\
                 Includes {}, {}, {}, {}, {}, {}, {}, {}, {} and {}.\n",
                link(10),
                link(9),
                link(8),
                link(7),
                link(6),
                link(5),
                link(4),
                link(3),
                link(2),
                link(1)
            )
        );
    }
}
