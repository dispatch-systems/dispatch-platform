//! Release coordination. A release publishes a commit already on main, under a version
//! stamped into the build CI made of it. GitHub and immutable assets are the source of
//! truth; the small local journal only pins intent across process interruptions.
mod assets;
mod backup;
mod dry_run;
mod local;
mod notes;
mod origins;
mod production;
mod rehearsal;
mod relay;
mod workflow;

use crate::{
    REPOSITORY, Result, artifact,
    io::{self, System},
    management, releases, require,
    updater::{Environment, Updater},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const HELP: &str = "Dispatch release [publish|status|backup|dry-run] [X.Y.Z] --root CHECKOUT
  publish   Release main's head: run its own checks, which build or carry its Production
            relay app and reuse the tests main's checks passed for the commit, require Dev
            serving the commit, publish the release once Production can take it, verify
            Production installs it and report what its log shows (the default). A release
            that brings migrations publishes only after a backup made for it in the last six
            hours, and once a copy of that backup starts under the release's backend on
            Dev in an isolated sandbox.
            --backup takes that backup when it's missing, as backup does, once Production
            passes its checks, then publishes. It stops Production for about a second, so pass
            it only with the owner's approval.
            --after COMMIT first waits for Dev to serve COMMIT, then hands the release to the
            host tool Dev installed with it, when that isn't the one running.
  status    Inspect GitHub, saved assets and public health without changing release state.
  backup    Back Production up for the release over ssh, stopping it for about a second,
            restore and check it on Dev with the prepared candidate, and record it.
  dry-run   Rehearse what only a release runs, for the commit publish would release: its own
            checks, the package, the smoke check and a verified GitHub draft, then delete the
            draft, its checks branch and its files. It never publishes or touches Production.
With no version, publish makes a new release, the latest published version bumped. It sets
aside any release never published, unless that release already holds the same commit, which
it continues. A version publishes, or finishes publishing, exactly that release.
DSP Dispatch: Relay's version goes up, by the same bump, only when the commit changed
android/, Markdown aside, since the last released app; otherwise the release carries that app
unchanged.
Options: --bump patch|minor|major, --commit REV, --notes PATH, --releases PATH
Origins: --dev-origin HTTPS_ORIGIN --production-origin HTTPS_ORIGIN
  Or set devOrigin and productionOrigin in CHECKOUT/config/release.json.
Rerun the same command after fixing a failure. Tags and published assets are never overwritten.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Publish,
    Status,
    Backup,
    DryRun,
}

struct Options {
    stage: Stage,
    version: Option<String>,
    root: PathBuf,
    bump: String,
    commit: Option<String>,
    notes: Option<PathBuf>,
    releases: Option<PathBuf>,
    origins: origins::Origins,
    /// `--backup`: publish takes the backup its migrations need when it has none recent.
    backup: bool,
    /// `--after`: the commit Dev must serve before the release starts.
    after: Option<String>,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut value = Self {
            stage: Stage::Publish,
            version: None,
            root: PathBuf::new(),
            bump: "patch".into(),
            commit: None,
            notes: None,
            releases: None,
            origins: origins::Origins::default(),
            backup: false,
            after: None,
        };
        let mut iter = args.iter().peekable();
        if let Some(stage) = iter.peek().and_then(|s| match s.as_str() {
            "publish" => Some(Stage::Publish),
            "status" => Some(Stage::Status),
            "backup" => Some(Stage::Backup),
            "dry-run" => Some(Stage::DryRun),
            _ => None,
        }) {
            value.stage = stage;
            iter.next();
        }
        let mut seen = BTreeSet::new();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--root"
                | "--bump"
                | "--commit"
                | "--notes"
                | "--releases"
                | "--dev-origin"
                | "--production-origin"
                | "--after" => {
                    require(seen.insert(arg), "Duplicate release option")?;
                    let next = iter.next().ok_or("Release option needs a value")?;
                    require(!next.starts_with('-'), "Release option needs a value")?;
                    match arg.as_str() {
                        "--root" => value.root = std::path::absolute(next)?,
                        "--bump" => value.bump = next.clone(),
                        "--commit" => value.commit = Some(next.clone()),
                        "--after" => value.after = Some(next.clone()),
                        "--notes" => value.notes = Some(std::path::absolute(next)?),
                        "--dev-origin" => value.origins.dev = next.clone(),
                        "--production-origin" => value.origins.production = next.clone(),
                        _ => value.releases = Some(std::path::absolute(next)?),
                    }
                }
                "--backup" => {
                    require(seen.insert(arg), "Duplicate release option")?;
                    value.backup = true;
                }
                _ => {
                    require(value.version.is_none(), "Unexpected release argument")?;
                    releases::version(arg)?;
                    value.version = Some(arg.clone());
                }
            }
        }
        require(value.root.is_absolute(), "--root CHECKOUT is required")?;
        require(
            value.stage == Stage::Publish || (!value.backup && value.after.is_none()),
            "--backup and --after apply only to publish",
        )?;
        value.origins = origins::Origins::load(&value.root, value.origins)?;
        require(
            matches!(value.bump.as_str(), "patch" | "minor" | "major"),
            "Invalid version bump",
        )?;
        Ok(value)
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    format: u32,
    version: String,
    /// The Dev revision a release PR brought to main, before releases came from main.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dev_commit: Option<String>,
    commit: Option<String>,
    /// The `relay-version` its checks run with, decided once for its commit: DSP Dispatch:
    /// Relay's Production version it releases, or empty to carry the last released one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relay_version: Option<String>,
    complete: bool,
}

struct Release<'a> {
    system: &'a dyn System,
    root: PathBuf,
    directory: PathBuf,
    staging: PathBuf,
    output: PathBuf,
    notes: PathBuf,
    version: String,
    tag: String,
    /// A temporary branch at the release commit, only for dispatching its full checks.
    checks_branch: String,
    origins: origins::Origins,
    /// No version was named: a new release, which takes the place of any never published.
    fresh: bool,
    /// How far a new version goes up: `patch`, `minor` or `major`, the app's too.
    bump: String,
    /// Publishing takes the backup its migrations need when it has none recent.
    take_backup: bool,
}

fn say(message: impl std::fmt::Display) {
    eprintln!("[release] {message}");
}
fn next_version(latest: &str, bump: &str) -> Result<String> {
    let mut parts = releases::version(latest)?;
    let index = match bump {
        "major" => 0,
        "minor" => 1,
        "patch" => 2,
        _ => return Err("Invalid bump".into()),
    };
    parts[index] = parts[index].checked_add(1).ok_or("Version overflow")?;
    parts[index + 1..].fill(0);
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

impl<'a> Release<'a> {
    fn new(system: &'a dyn System, options: &Options, version: &str, directory: &Path) -> Self {
        let release = Self::at(system, &options.root, directory, &options.origins, version);
        Self {
            notes: options.notes.clone().unwrap_or(release.notes.clone()),
            fresh: options.version.is_none(),
            bump: options.bump.clone(),
            take_backup: options.backup,
            ..release
        }
    }
    fn at(
        system: &'a dyn System,
        root: &Path,
        directory: &Path,
        origins: &origins::Origins,
        version: &str,
    ) -> Self {
        Self {
            system,
            root: root.into(),
            output: directory.join(format!("v{version}")),
            notes: directory.join(format!("v{version}-notes.md")),
            directory: directory.into(),
            staging: system.release_staging(),
            version: version.into(),
            tag: format!("v{version}"),
            checks_branch: format!("release-checks/v{version}"),
            origins: origins.clone(),
            fresh: false,
            bump: "patch".into(),
            take_backup: false,
        }
    }
    /// Another version's release in the same directory, with its own notes.
    fn other(&self, version: &str) -> Self {
        Self::at(
            self.system,
            &self.root,
            &self.directory,
            &self.origins,
            version,
        )
    }
    fn command(&self, args: &[&str], cwd: Option<&Path>, timeout: u64) -> Result<String> {
        Ok(String::from_utf8(self.system.command(
            args,
            Some(cwd.unwrap_or(&self.root)),
            timeout,
            None,
        )?)?
        .trim()
        .into())
    }
    fn api(&self, endpoint: &str, args: &[&str]) -> Result<Value> {
        let endpoint = format!("repos/{REPOSITORY}/{endpoint}");
        let mut command = vec!["gh", "api", &endpoint];
        command.extend_from_slice(args);
        Ok(serde_json::from_str(&self.command(&command, None, 120)?)?)
    }
    /// Deletes `endpoint`, which answers with no body for `api` to read.
    fn delete(&self, endpoint: &str) -> Result<()> {
        let endpoint = format!("repos/{REPOSITORY}/{endpoint}");
        self.command(&["gh", "api", &endpoint, "--method", "DELETE"], None, 120)?;
        Ok(())
    }
    fn journal_path(&self) -> PathBuf {
        self.directory.join(format!(".{}-state.json", self.tag))
    }
    fn journal(&self) -> Result<Journal> {
        if !self.journal_path().try_exists()? {
            return Ok(Journal {
                format: 1,
                version: self.version.clone(),
                ..Default::default()
            });
        }
        let value: Journal = serde_json::from_value(io::read_json(&self.journal_path())?)?;
        require(
            value.format == 1
                && value.version == self.version
                && value
                    .dev_commit
                    .as_ref()
                    .is_none_or(|s| artifact::hex(s, 40))
                && value.commit.as_ref().is_none_or(|s| artifact::hex(s, 40))
                && value
                    .relay_version
                    .as_deref()
                    .is_none_or(|s| s.is_empty() || releases::version(s).is_ok()),
            "Invalid release journal",
        )?;
        Ok(value)
    }
    fn save(&self, journal: &Journal) -> Result<()> {
        io::write_json(&self.journal_path(), &serde_json::to_value(journal)?)
    }
    fn record_commit(&self, commit: &str) -> Result<()> {
        require(artifact::hex(commit, 40), "Full release commit required")?;
        let mut journal = self.journal()?;
        require(
            journal.commit.as_deref().is_none_or(|old| old == commit),
            "Release commit changed",
        )?;
        journal.commit = Some(commit.into());
        self.save(&journal)
    }
    fn listed_release(&self) -> Result<Option<Value>> {
        Ok(all_releases(self.system)?
            .into_iter()
            .find(|r| r["tag_name"] == self.tag))
    }
    fn checks(&self, sha: &str, event: &str, branch: Option<&str>, wait: bool) -> Result<Value> {
        let deadline = self.system.monotonic() + Duration::from_secs(if wait { 1800 } else { 0 });
        let mut announced = false;
        loop {
            let runs = io::github(
                self.system,
                &format!(
                    "actions/workflows/checks.yml/runs?event={event}&head_sha={sha}&per_page=100"
                ),
            )?;
            let run = releases::latest_run(
                &runs["workflow_runs"],
                sha,
                event,
                branch,
                event == "pull_request",
            );
            if let Some(run) = run {
                if !announced {
                    say(format!("Checks: {}", io::text(run, "html_url")));
                    announced = true;
                }
                if run["status"] == "completed" {
                    require(
                        releases::passed(run),
                        &format!(
                            "Checks ended with {}: {}. Fix the cause and rerun.",
                            io::text(run, "conclusion"),
                            io::text(run, "html_url")
                        ),
                    )?;
                    return Ok(run.clone());
                }
            }
            require(
                wait && self.system.monotonic() < deadline,
                "Checks have not passed for the selected commit",
            )?;
            self.system.sleep(Duration::from_secs(10));
        }
    }
    fn checked_source(&self, commit: &str) -> Result<Value> {
        require(artifact::hex(commit, 40), "Full source commit required")?;
        let comparison = io::github(self.system, &format!("compare/{commit}...main"))?;
        require(
            matches!(comparison["status"].as_str(), Some("ahead" | "identical")),
            "Source must be merged into main",
        )?;
        self.fully_validated(commit)?
            .ok_or_else(|| "Checks have not passed for the selected commit".into())
    }
    fn execute(&self, stage: Stage, requested: Option<&str>) -> Result<Value> {
        if stage == Stage::Status {
            return self.status();
        }
        io::private_directory(&self.directory)?;
        let _lock = io::ExclusiveLock::try_acquire(&self.directory.join(".release.lock"))?
            .ok_or("Another release command is running")?;
        if stage == Stage::Backup {
            return self.backup();
        }
        if stage == Stage::DryRun {
            return self.dry_run(requested);
        }
        self.git(&["fetch", "--tags", "origin", "main"], None, 300)?;
        if self.fresh {
            self.replace_unpublished(requested)?;
        }
        if !self.listed_release()?.is_some_and(|r| r["draft"] == false) {
            let commit = self.pin(requested)?;
            let prepared = self.prepare(&commit)?;
            self.smoke(&prepared)?;
            self.ensure_draft(&prepared)?;
        }
        // Publication can only consume complete, revalidated preparation.
        let prepared = self.prepared(None)?;
        self.record_commit(&prepared.commit)?;
        let (release, backup) = self.publish(&prepared)?;
        self.verify_production(&prepared, &release)?;
        let (log, slack) = self.check_installed(&release)?;
        self.clean()?;
        let mut journal = self.journal()?;
        journal.complete = true;
        self.save(&journal)?;
        let mut result = json!({"version":self.version,"stage":"complete","release":release["html_url"],
            "runtimeDigest":prepared.runtime_digest,"productionLog":log,
            "upgradeRehearsal":self.rehearsal()?});
        if let Some(saved) = slack {
            result["slackManifest"] = json!(saved);
        }
        if let Some(backup) = backup {
            result["backup"] =
                json!({"destination":backup["destination"],"databases":backup["databases"]});
        }
        Ok(result)
    }
    fn status(&self) -> Result<Value> {
        let prepared = if self.output.try_exists()? {
            match self.prepared(None) {
                Ok(p) => json!({"valid":true,"commit":p.commit,"runtimeDigest":p.runtime_digest}),
                Err(error) => json!({"valid":false,"problem":error.to_string()}),
            }
        } else {
            Value::Null
        };
        let health = self
            .system
            .request(
                &format!("{}/api/health", self.origins.production),
                false,
                true,
                20,
            )
            .and_then(io::json_response)
            .unwrap_or_else(|error| json!({"problem": error.to_string()}));
        Ok(
            json!({"version":self.version,"journal":self.journal()?,"prepared":prepared,
            "release":self.listed_release()?,"production":health}),
        )
    }
    /// A new release takes the place of every release started after the latest published
    /// one and never published, except one pinned to the commit it releases: a rerun after a
    /// failure, which it continues.
    fn replace_unpublished(&self, requested: Option<&str>) -> Result<()> {
        let commit = self.resolve(requested)?;
        for version in unpublished(&self.directory, &all_releases(self.system)?)? {
            let other = self.other(&version);
            if version == self.version && other.journal()?.commit.as_deref() == Some(&commit) {
                continue;
            }
            other.supersede()?;
        }
        Ok(())
    }
}

fn all_releases(system: &dyn System) -> Result<Vec<Value>> {
    let pages: Vec<Vec<Value>> = serde_json::from_slice(&system.command(
        &[
            "gh",
            "api",
            &format!("repos/{REPOSITORY}/releases?per_page=100"),
            "--paginate",
            "--slurp",
        ],
        None,
        120,
        None,
    )?)?;
    Ok(pages.into_iter().flatten().collect())
}
fn latest_version(listed: &[Value]) -> &str {
    listed
        .iter()
        .filter(|r| r["draft"] == false && r["prerelease"] == false)
        .filter_map(|r| r["tag_name"].as_str().and_then(|s| s.strip_prefix('v')))
        .filter_map(|s| releases::version(s).ok().map(|v| (v, s)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, s)| s)
        .unwrap_or("0.0.0")
}
/// The versions after the latest published release that were started and never published:
/// a GitHub draft, an unfinished journal or a prepared directory.
fn unpublished(directory: &Path, listed: &[Value]) -> Result<BTreeSet<String>> {
    let latest = releases::version(latest_version(listed))?;
    let mut found = BTreeSet::new();
    let mut published = BTreeSet::new();
    for release in listed {
        if let Some(v) = release["tag_name"]
            .as_str()
            .and_then(|s| s.strip_prefix('v'))
        {
            if release["draft"] == true {
                found.insert(v.to_owned());
            } else {
                published.insert(v.to_owned());
            }
        }
    }
    if directory.try_exists()? {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".v") && name.ends_with("-state.json") {
                let journal: Journal = serde_json::from_value(io::read_json(&entry.path())?)?;
                require(
                    journal.format == 1 && name == format!(".v{}-state.json", journal.version),
                    "Invalid release journal",
                )?;
                if !journal.complete {
                    found.insert(journal.version);
                }
            } else if entry.file_type()?.is_dir()
                && let Some(version) = name.strip_prefix('v')
            {
                found.insert(version.into());
            }
        }
    }
    found.retain(|v| {
        !published.contains(v) && releases::version(v).is_ok_and(|parts| parts > latest)
    });
    Ok(found)
}

/// PATH with `home`'s `.local/bin` first, where Dev keeps `gh`, when `path` lacks it: a login
/// shell puts it there, and plain ssh doesn't.
fn search_path(path: &OsStr, home: &Path) -> Option<OsString> {
    let local = home.join(".local/bin");
    let mut paths: Vec<_> = std::env::split_paths(path).collect();
    if paths.contains(&local) {
        return None;
    }
    paths.insert(0, local);
    std::env::join_paths(paths).ok()
}

/// How long `--after` waits for Dev to serve its commit: main's checks run on it first, then
/// Dev installs it.
const DEV_INSTALLS: u64 = 1800;

/// `--after`: waits until Dev serves `commit` with the host tool its build brings, then hands
/// the release, its arguments less `--after`, to that tool when it isn't the one running: Dev
/// replaces its tool on each install, and one already running stays the one it started as.
fn after(commit: &str, args: &[String], options: &Options, system: &dyn System) -> Result<()> {
    let dev = Updater::new(&options.root, Environment::Dev, system).map_err(|error| {
        format!("--after waits for Dev, so --root must be Dev's checkout: {error}")
    })?;
    say(format!("Waiting for Dev to serve {commit}"));
    dev.wait(commit, DEV_INSTALLS)?;
    let installed = management::directory(&dev).join("dispatch-host");
    if artifact::file_hash(&installed)? == artifact::file_hash(&system.current_exe()?)? {
        return Ok(());
    }
    say(format!(
        "Dev installed a new host tool with it; handing the release to {}",
        installed.display()
    ));
    let mut forwarded = vec!["host", "release"];
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--after" {
            rest.next();
        } else {
            forwarded.push(arg);
        }
    }
    let error = system.exec(&installed, &forwarded);
    Err(format!(
        "Dev's host tool {} didn't start: {error}",
        installed.display()
    )
    .into())
}

pub fn run(args: &[String], system: &dyn System) -> Result<Value> {
    if args.iter().any(|s| matches!(s.as_str(), "--help" | "-h")) {
        println!("{HELP}");
        return Ok(Value::Null);
    }
    if let (Some(path), Some(home)) = (std::env::var_os("PATH"), std::env::var_os("HOME"))
        && let Some(path) = search_path(&path, Path::new(&home))
    {
        // SAFETY: the release sets it before it runs a command or starts a thread.
        unsafe { std::env::set_var("PATH", path) };
    }
    let options = Options::parse(args)?;
    if let Some(commit) = &options.after {
        after(commit, args, &options, system)?;
    }
    let common = String::from_utf8(system.command(
        &[
            "git",
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ],
        Some(&options.root),
        120,
        None,
    )?)?;
    let platform = Path::new(common.trim())
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid checkout")?
        .to_path_buf();
    let listed = all_releases(system)?;
    let latest = latest_version(&listed);
    let directory = options
        .releases
        .clone()
        .unwrap_or_else(|| platform.join("releases"));
    let version = match &options.version {
        Some(version) => version.clone(),
        // Status shows the newest release, published or not.
        None if options.stage == Stage::Status => unpublished(&directory, &listed)?
            .into_iter()
            .max_by_key(|v| releases::version(v).ok())
            .unwrap_or_else(|| latest.into()),
        None => next_version(latest, &options.bump)?,
    };
    if options.stage != Stage::Status {
        require(
            listed
                .iter()
                .any(|r| r["tag_name"] == format!("v{version}"))
                || releases::version(&version)? > releases::version(latest)?,
            "Version must be newer than the latest release",
        )?;
    }
    Release::new(system, &options, &version, &directory)
        .execute(options.stage, options.commit.as_deref())
}

#[cfg(test)]
mod tests;
