use super::*;
use std::collections::BTreeMap;

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
    pub(super) fn git(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<String> {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        self.command(&command, cwd, timeout)
    }
    /// The newest published stable release before this one.
    pub(super) fn previous(&self) -> Result<Option<String>> {
        let listed: Vec<_> = all_releases(self.system)?
            .into_iter()
            .filter(|r| r["tag_name"] != self.tag)
            .collect();
        let latest = latest_version(&listed);
        Ok((latest != "0.0.0").then(|| format!("v{latest}")))
    }
    /// The commit `--commit` names, or main's head.
    pub(super) fn resolve(&self, requested: Option<&str>) -> Result<String> {
        self.git(
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", requested.unwrap_or("origin/main")),
            ],
            None,
            120,
        )
    }
    /// The commit this release publishes: main's head, or the commit on main `--commit`
    /// names. A resumed release keeps its pin; `--commit` replaces it only while nothing
    /// has been prepared from it, such as after its full checks failed.
    pub(super) fn pin(&self, requested: Option<&str>) -> Result<String> {
        let mut journal = self.journal()?;
        let commit = match (&journal.commit, requested) {
            (Some(pinned), None) => pinned.clone(),
            (pinned, requested) => {
                let commit = self.resolve(requested)?;
                if let Some(pinned) = pinned
                    && *pinned != commit
                {
                    require(
                        !self.output.try_exists()?,
                        "The release is prepared from its pinned commit; resume without --commit",
                    )?;
                    say(format!("Replacing the unprepared release commit {pinned}"));
                }
                commit
            }
        };
        require(artifact::hex(&commit, 40), "Full release commit required")?;
        self.git(
            &["merge-base", "--is-ancestor", &commit, "origin/main"],
            None,
            120,
        )
        .map_err(|_| "The release commit must be on main")?;
        if journal.commit.as_ref() != Some(&commit) {
            if let Some(previous) = self.previous()? {
                let comparison =
                    io::github(self.system, &format!("compare/{previous}...{commit}"))?;
                require(
                    comparison["status"] == "ahead",
                    &format!("Main has nothing new since {previous}"),
                )?;
                let changes = self.changes_after(&previous, &commit)?;
                if !changes.is_empty() {
                    say(format!(
                        "Included changes:\n{}",
                        changes
                            .iter()
                            .map(|(number, title)| format!("- #{number} {title}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
            }
            journal.commit = Some(commit.clone());
            // The app's version is decided for the commit it's built from.
            journal.relay_version = None;
            self.save(&journal)?;
        }
        say(format!("Releasing main commit {commit}"));
        Ok(commit)
    }
    /// The PRs on main's first-parent line after `previous` up to `commit`.
    fn changes_after(&self, previous: &str, commit: &str) -> Result<Vec<(u64, String)>> {
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
    /// Whether `run` validated the whole suite on `commit`: its `platform` gate passed, which a
    /// partial manual run skips, and its `core` job passed, which a scoped run of the former
    /// workflow skipped, or was skipped for main's push run of the commit, which passed it.
    fn full_run(&self, run: &Value, commit: &str) -> Result<bool> {
        let jobs = io::github(
            self.system,
            &format!("actions/runs/{}/jobs?filter=latest&per_page=100", run["id"]),
        )?;
        let ended = |name: &str, conclusion: &str| {
            jobs["jobs"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|job| job["name"] == name && job["conclusion"] == conclusion)
        };
        Ok(ended("platform", "success")
            && (ended("core", "success")
                || (ended("core", "skipped") && self.tested(commit)?.is_some())))
    }
    /// Main's push run of the checks on exactly `commit`, if its newest passed: every suite ran
    /// on the commit there, so the release's own run skips those that test its source.
    fn tested(&self, commit: &str) -> Result<Option<Value>> {
        let runs = io::github(
            self.system,
            &format!("actions/workflows/checks.yml/runs?event=push&head_sha={commit}&per_page=100"),
        )?;
        Ok(
            releases::latest_run(&runs["workflow_runs"], commit, "push", Some("main"), false)
                .filter(|run| releases::passed(run))
                .cloned(),
        )
    }
    /// The release's own run of the checks on exactly `commit`, dispatched on its temporary
    /// branch, if its newest passed the full suite, or the package's checks on main's passed
    /// suites. Main's push run alone never counts: only a release's run builds Production's app.
    pub(super) fn fully_validated(&self, commit: &str) -> Result<Option<Value>> {
        let runs = io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={commit}&per_page=100"
            ),
        )?;
        if let Some(run) = releases::latest_run(
            &runs["workflow_runs"],
            commit,
            "workflow_dispatch",
            Some(&self.checks_branch),
            false,
        ) && releases::passed(run)
            && self.full_run(run, commit)?
        {
            return Ok(Some(run.clone()));
        }
        Ok(None)
    }
    /// Requires the full suite on exactly `commit`, in the release's own run: dispatched on a
    /// temporary branch at the commit with `relay`, the app version it builds (empty to carry
    /// the last released app), and awaited. Once main's push run of the commit passed, the
    /// release's run names it and builds, packs and checks the package alone. A rerun awaits the
    /// run already dispatched.
    pub(super) fn full_suite(&self, commit: &str, relay: &str) -> Result<()> {
        let url = self.dispatch_full(commit, relay)?;
        say(format!("Full checks: {url}"));
        // The temporary branch only ever serves a dispatched run.
        self.remove_checks_branch()
    }
    fn dispatch_full(&self, commit: &str, relay: &str) -> Result<String> {
        let runs = io::github(
            self.system,
            &format!(
                "actions/workflows/checks.yml/runs?event=workflow_dispatch&head_sha={commit}&per_page=100"
            ),
        )?;
        if releases::latest_run(
            &runs["workflow_runs"],
            commit,
            "workflow_dispatch",
            Some(&self.checks_branch),
            false,
        )
        .is_none()
        {
            let tested = self.tested(commit)?;
            let relay = format!("relay-version={relay}");
            let reused = tested
                .as_ref()
                .map(|run| format!("tested-run={}", run["id"]));
            let mut command = vec![
                "gh",
                "workflow",
                "run",
                "checks.yml",
                "--repo",
                REPOSITORY,
                "--ref",
                &self.checks_branch,
                "-f",
                &relay,
            ];
            if let Some(reused) = &reused {
                command.extend(["-f", reused]);
            }
            say(match &tested {
                Some(run) => format!(
                    "Starting the release's checks, reusing the suites main's checks passed: {}",
                    io::text(run, "html_url")
                ),
                None => "Starting the release's checks".into(),
            });
            self.checks_branch_at(commit)?;
            self.command(&command, None, 120)?;
        }
        // A failure stays for inspection; rerunning its failed jobs lets this resume.
        let run = self.checks(commit, "workflow_dispatch", Some(&self.checks_branch), true)?;
        require(
            self.full_run(&run, commit)?,
            "The dispatched checks did not run the full suite",
        )?;
        Ok(io::text(&run, "html_url"))
    }
    fn checks_ref(&self) -> Result<Option<String>> {
        let name = format!("refs/heads/{}", self.checks_branch);
        let refs = self.api(
            &format!("git/matching-refs/heads/{}", self.checks_branch),
            &[],
        )?;
        Ok(refs
            .as_array()
            .ok_or("Invalid branch refs")?
            .iter()
            .find(|r| r["ref"] == name)
            .map(|r| io::text(&r["object"], "sha")))
    }
    fn checks_branch_at(&self, commit: &str) -> Result<()> {
        match self.checks_ref()? {
            Some(sha) if sha == commit => {}
            Some(_) => {
                self.api(
                    &format!("git/refs/heads/{}", self.checks_branch),
                    &[
                        "--method",
                        "PATCH",
                        "-f",
                        &format!("sha={commit}"),
                        "-F",
                        "force=true",
                    ],
                )?;
            }
            None => {
                self.api(
                    "git/refs",
                    &[
                        "-f",
                        &format!("ref=refs/heads/{}", self.checks_branch),
                        "-f",
                        &format!("sha={commit}"),
                    ],
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn remove_checks_branch(&self) -> Result<()> {
        if self.checks_ref()?.is_some() {
            self.command(
                &[
                    "gh",
                    "api",
                    &format!("repos/{REPOSITORY}/git/refs/heads/{}", self.checks_branch),
                    "--method",
                    "DELETE",
                ],
                None,
                120,
            )?;
        }
        Ok(())
    }
    pub(super) fn clean(&self) -> Result<()> {
        self.remove_checks_branch()?;
        // Preserve custom notes and any edits made after their published copy.
        if self.notes == self.directory.join(format!("{}-notes.md", self.tag))
            && self.notes.try_exists()?
            && self.output.join("notes.md").try_exists()?
            && fs::read(&self.notes)? == fs::read(self.output.join("notes.md"))?
        {
            fs::remove_file(&self.notes)?;
        }
        Ok(())
    }
    /// Sets aside a release that was never published, so another can take its place. Its
    /// GitHub draft is deleted: a draft has no tag, and Production never sees one. Its notes,
    /// prepared directory and journal move to `superseded/` rather than being deleted.
    pub(super) fn supersede(&self) -> Result<()> {
        if let Some(release) = self.listed_release()? {
            require(
                release["draft"] == true,
                "A published release is never superseded",
            )?;
            self.command(
                &[
                    "gh",
                    "api",
                    &format!("repos/{REPOSITORY}/releases/{}", release["id"]),
                    "--method",
                    "DELETE",
                ],
                None,
                120,
            )?;
        }
        self.remove_checks_branch()?;
        let pinned = self
            .journal()?
            .commit
            .map_or("unpinned".into(), |c| c[..8].to_owned());
        let superseded = self.directory.join("superseded");
        let mut target = superseded.join(format!("{}-{pinned}", self.tag));
        for n in 2.. {
            if !target.try_exists()? {
                break;
            }
            target = superseded.join(format!("{}-{pinned}-{n}", self.tag));
        }
        io::private_directory(&target)?;
        // Only the directory and journal mark a release unpublished, so they move last: a
        // run interrupted here leaves this release for the next one to set aside.
        for path in [self.notes.clone(), self.output.clone(), self.journal_path()] {
            if path.try_exists()? {
                fs::rename(&path, target.join(path.file_name().ok_or("Invalid path")?))?;
            }
        }
        say(format!(
            "Set aside the unpublished {} in {}",
            self.tag,
            target.display()
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
