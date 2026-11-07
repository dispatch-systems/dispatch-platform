//! What a change shows, approved before its PR opens. `preview` serves the checkout as it is and
//! saves a screenshot of each page the change touches, with a link to click through it;
//! `preview approve` keeps the UI files the user was shown, by their contents, once they
//! approved them, and `pr` opens no PR for a change to what a page shows until those are the
//! files it would push.
use crate::{Result, Runner, dev, require, review, shot, workspace};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Where an approval is kept, in the checkout's scratch folder.
const APPROVED: &str = "preview/approved.json";

/// A change's UI files against what the user approved.
#[derive(Debug, PartialEq, Eq)]
pub enum Approval {
    /// The change touches nothing a page shows.
    None,
    /// The files are the ones approved.
    Approved,
    /// Nothing was approved yet.
    Missing,
    /// These differ from what was approved.
    Changed(Vec<String>),
}

/// Each UI file the change touches against `origin/main`, by the hash of its contents, or
/// `deleted`: committed, or as the worktree holds it with `worktree`.
fn files(root: &Path, worktree: bool, runner: &dyn Runner) -> Result<BTreeMap<String, String>> {
    let git = |args: &[&str]| {
        let mut command = vec!["git"];
        command.extend_from_slice(args);
        workspace::text(runner, &command, Some(root))
    };
    let main = git(&["merge-base", "origin/main", "HEAD"])?;
    let against = match worktree {
        true => main.clone(),
        false => format!("{main}..HEAD"),
    };
    let mut paths: Vec<String> = git(&["diff", "--name-only", "--no-renames", &against])?
        .lines()
        .map(str::to_owned)
        .collect();
    if worktree {
        paths.extend(
            git(&["ls-files", "--others", "--exclude-standard"])?
                .lines()
                .map(str::to_owned),
        );
    }
    let mut files = BTreeMap::new();
    for path in paths.into_iter().filter(|path| review::ui(path)) {
        let hash = match worktree {
            true if root.join(&path).exists() => git(&["hash-object", "--", &path])?,
            false => {
                git(&["rev-parse", "--verify", "-q", &format!("HEAD:{path}")]).unwrap_or_default()
            }
            _ => String::new(),
        };
        let hash = if hash.is_empty() {
            "deleted".to_owned()
        } else {
            hash
        };
        files.insert(path, hash);
    }
    Ok(files)
}

/// The committed change's UI files against what the user approved: a file changed since, or
/// approved and since put back as `main` has it, differs.
pub fn approval(root: &Path, runner: &dyn Runner) -> Result<Approval> {
    let now = files(root, false, runner)?;
    let path = workspace::checkout_scratch(root)?.join(APPROVED);
    let approved: Option<BTreeMap<String, String>> = match fs::read_to_string(&path) {
        Ok(text) => Some(serde_json::from_str(&text)?),
        Err(_) => None,
    };
    let approved = match (approved, now.is_empty()) {
        (None, true) => return Ok(Approval::None),
        (None, false) => return Ok(Approval::Missing),
        (Some(approved), _) => approved,
    };
    let mut changed: Vec<String> = now
        .iter()
        .filter(|(path, hash)| approved.get(*path) != Some(hash))
        .map(|(path, _)| path.clone())
        .collect();
    // An approved file the change no longer touches: put back as main had it, or the same as
    // approved because main has it now.
    for (path, hash) in approved.iter().filter(|(path, _)| !now.contains_key(*path)) {
        let committed = workspace::text(
            runner,
            &[
                "git",
                "rev-parse",
                "--verify",
                "-q",
                &format!("HEAD:{path}"),
            ],
            Some(root),
        )
        .unwrap_or_default();
        let committed = if committed.is_empty() {
            "deleted"
        } else {
            &committed
        };
        if committed != hash {
            changed.push(path.clone());
        }
    }
    changed.sort();
    Ok(match changed.is_empty() {
        true => Approval::Approved,
        false => Approval::Changed(changed),
    })
}

/// `dispatchdev preview approve`: the UI files as the worktree holds them, kept as what the user
/// approved. Run only once they said so.
pub fn approve(root: &Path, runner: &dyn Runner) -> Result<()> {
    let files = files(root, true, runner)?;
    require(
        !files.is_empty(),
        "The change touches nothing a page shows: there is nothing to approve.",
    )?;
    let path = workspace::checkout_scratch(root)?.join(APPROVED);
    workspace::write_private(&path, &serde_json::to_string_pretty(&files)?)?;
    println!(
        "Kept {} UI file{} as approved: {}.",
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        files.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    println!(
        "{}",
        workspace::next(&["Commit them, then `dispatchdev pr` opens the PR".to_owned()])
    );
    Ok(())
}

/// How long a dev server has to start: a first build of the backend takes minutes.
const STARTING: Duration = Duration::from_secs(600);

/// A page to show: a route's id, `None` for the DSP's landing page, on the platform owner's
/// own pages or a DSP's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub id: Option<String>,
    pub platform: bool,
}
impl Page {
    /// A page as `--pages` names it: `<id>`, or `platform:<id>` for the platform owner's own.
    pub fn named(name: &str) -> Self {
        match name.strip_prefix("platform:") {
            Some(id) => Self {
                id: Some(id.to_owned()),
                platform: true,
            },
            None => Self {
                id: Some(name.to_owned()),
                platform: false,
            },
        }
    }
}

/// The routes a `feature.ts` declares in its `routes: [...]`, each by its id and whether it's
/// the platform owner's: the id and scope at each entry's own level, not those of what it holds.
fn routes(text: &str) -> Vec<(String, bool)> {
    let Some(start) = text.find("routes: [") else {
        return vec![];
    };
    let mut routes = vec![];
    let (mut depth, mut entry) = (0usize, String::new());
    for c in text[start + "routes: ".len()..].chars() {
        match c {
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                if depth == 1 && c == '}' {
                    routes.extend(route(&entry));
                    entry.clear();
                }
                continue;
            }
            _ => {}
        }
        // Only an entry's own text: what's nested in it is left out.
        if depth == 2 && !matches!(c, '{') {
            entry.push(c);
        }
    }
    routes
}
/// A route entry's id and whether its scope is the platform owner's.
fn route(entry: &str) -> Option<(String, bool)> {
    let field = |name: &str| {
        let rest = entry.split(&format!("{name}: '")).nth(1)?;
        Some(rest.split('\'').next()?.to_owned())
    };
    Some((field("id")?, field("scope").as_deref() == Some("platform")))
}

/// The page a UI file belongs to: of its owner's routes, the one its folder under `frontend/` is
/// named for, else the first; the DSP's landing page when its owner has none.
fn page(root: &Path, path: &str) -> Page {
    let landing = Page {
        id: None,
        platform: false,
    };
    let Some((owner, inside)) = path.split_once("/frontend/") else {
        return landing;
    };
    let Ok(text) = fs::read_to_string(root.join(owner).join("frontend/feature.ts")) else {
        return landing;
    };
    let routes = routes(&text);
    let folder = inside.split('/').next().unwrap_or_default();
    routes
        .iter()
        .find(|(id, _)| id == folder)
        .or(routes.first())
        .map_or(landing, |(id, platform)| Page {
            id: Some(id.clone()),
            platform: *platform,
        })
}

/// The checkout's dev server, serving its backend as it is now: the one running, else a new one,
/// once the one it replaces, checked to be the server it recorded, has stopped.
fn serving(root: &Path, runner: &dyn Runner) -> Result<dev::Status> {
    // Only a server started from the checkout as it is now serves it: a newer build on disk
    // says nothing of what an older server runs.
    let status = dev::status(root, runner)?;
    if status.running && status.source_matches == Some(true) {
        return Ok(status);
    }
    if let Some(process) = status.process.as_ref().filter(|_| status.running) {
        println!(
            "Stopping the dev server {}: it serves other code.",
            process.pid
        );
        // `dev::status` has just checked the process is the server it recorded.
        unsafe { libc::kill(process.pid, libc::SIGTERM) };
        let stopping = Instant::now();
        while dev::status(root, runner)?.running {
            require(
                stopping.elapsed() < Duration::from_secs(30),
                &format!(
                    "The dev server {} didn't stop: stop it, then run this again.",
                    process.pid
                ),
            )?;
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    let log = workspace::checkout_scratch(root)?.join("dev-server.log");
    // One file for both streams, so neither writes over the other.
    let output = fs::File::create(&log)?;
    let mut command = Command::new("npm");
    command
        .args(["run", "dev"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        // Its own process group, so it outlives this command.
        .process_group(0);
    // Served on the tailnet's address, so the link opens on the user's phone or laptop.
    if let Ok(host) = workspace::text(runner, &["tailscale", "ip", "-4"], Some(root))
        && let Some(host) = host.lines().next().filter(|host| !host.is_empty())
    {
        command.env("DISPATCH_DEV_HOST", host);
    }
    let mut child = command.spawn()?;
    println!("Starting the dev server; its log is {}.", log.display());
    let starting = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if let Some(exit) = child.try_wait()? {
            return Err(format!("The dev server stopped ({exit}): see {}.", log.display()).into());
        }
        let status = dev::status(root, runner)?;
        if status.running && status.source_matches == Some(true) {
            return Ok(status);
        }
        require(
            starting.elapsed() < STARTING,
            &format!(
                "The dev server didn't start in time: see {}.",
                log.display()
            ),
        )?;
    }
}

/// `dispatchdev preview`: the checkout's change as it is, served, with a screenshot of each page
/// it touches, or `pages`, wide and on a phone, and the link to click through it.
pub fn run(root: &Path, named: &[&str], runner: &dyn Runner) -> Result<()> {
    let files = files(root, true, runner)?;
    require(
        !files.is_empty() || !named.is_empty(),
        "The change touches nothing a page shows: --pages names pages to show anyway.",
    )?;
    let web: Vec<&String> = files
        .keys()
        .filter(|path| !path.starts_with("android/"))
        .collect();
    let mut pages: Vec<Page> = vec![];
    let wanted: Vec<Page> = match named.is_empty() {
        true => web.iter().map(|path| page(root, path)).collect(),
        false => named.iter().map(|name| Page::named(name)).collect(),
    };
    for page in wanted {
        if !pages.contains(&page) {
            pages.push(page);
        }
    }
    if !pages.is_empty() {
        let status = serving(root, runner)?;
        for page in &pages {
            for size in ["1440x1000", "390x844"] {
                shot::run(
                    root,
                    &shot::Shot {
                        open: shot::Open {
                            page: page.id.as_deref(),
                            dsp: None,
                            platform: page.platform,
                            tab: None,
                        },
                        who: None,
                        size,
                        theme: "light",
                    },
                    runner,
                )?;
            }
        }
        if let Some(link) = &status.link {
            println!("Link, for the chat alone:\n{link}");
        }
    }
    let mut next = vec![];
    if files.keys().any(|path| path.starts_with("android/")) {
        next.push("It changes an app's screens too: show them from the emulator".to_owned());
    }
    next.push(
        "Show the user the screenshots and the link; make what they ask and run this again. Once \
         they approve, `dispatchdev preview approve`"
            .to_owned(),
    );
    println!("{}", workspace::next(&next));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_ui_file_shows_the_page_its_owner_routes_it_to() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let owner = |path: &str, routes: &str| {
            let file = root.join(path).join("frontend/feature.ts");
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, routes).unwrap();
        };
        owner(
            "features/timecard",
            "routes: [{ id: 'paycom', scope: 'dsp', label: 'Timecard' }]",
        );
        // Ids outside `routes`, and those of what a route holds, aren't pages.
        owner(
            "core/support",
            "accountActions: [{ id: 'ai-support', label: 'AI Support' }],\n\
             routes: [{ id: 'support', scope: 'platform', tabs: [{ id: 'inbox', scope: 'dsp' }] }]",
        );
        owner(
            "core/collection",
            "settings: [{ id: 'collection.connections', label: 'Connections' }]",
        );
        owner(
            "core/platform_owner",
            "routes: [{ id: 'dsps', scope: 'platform' }, { id: 'jobs', scope: 'platform' }]",
        );
        let at = |id: &str, platform| Page {
            id: Some(id.to_owned()),
            platform,
        };
        for (path, shown) in [
            (
                "features/timecard/frontend/Calendar.tsx",
                at("paycom", false),
            ),
            (
                "core/platform_owner/frontend/jobs/JobsPage.tsx",
                at("jobs", true),
            ),
            (
                "core/platform_owner/frontend/shared/Table.tsx",
                at("dsps", true),
            ),
            (
                "core/support/frontend/owner/SupportPage.tsx",
                at("support", true),
            ),
            (
                "core/collection/frontend/ConnectionsPage.tsx",
                Page {
                    id: None,
                    platform: false,
                },
            ),
            (
                "core/shell/frontend/styles/tokens.css",
                Page {
                    id: None,
                    platform: false,
                },
            ),
        ] {
            assert_eq!(page(root, path), shown, "{path}");
        }
        assert_eq!(Page::named("platform:dsps"), at("dsps", true));
        assert_eq!(Page::named("paycom"), at("paycom", false));
    }
}
