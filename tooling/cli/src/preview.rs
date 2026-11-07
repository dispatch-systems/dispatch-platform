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

/// The committed change's UI files against what the user approved.
pub fn approval(root: &Path, runner: &dyn Runner) -> Result<Approval> {
    let now = files(root, false, runner)?;
    if now.is_empty() {
        return Ok(Approval::None);
    }
    let path = workspace::checkout_scratch(root)?.join(APPROVED);
    let approved: BTreeMap<String, String> = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => return Ok(Approval::Missing),
    };
    let changed: Vec<String> = now
        .iter()
        .filter(|(path, hash)| approved.get(*path) != Some(hash))
        .map(|(path, _)| path.clone())
        .collect();
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

/// The pages a change's UI files belong to: a feature's first page, as its `feature.ts` lists
/// its routes, or the DSP's landing page, `None`, for the rest.
fn pages(root: &Path, paths: &[&String]) -> Vec<Option<String>> {
    let mut pages: Vec<Option<String>> = vec![];
    for path in paths {
        let page = path
            .strip_prefix("features/")
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(feature, _)| {
                let routes = fs::read_to_string(
                    root.join(format!("features/{feature}/frontend/feature.ts")),
                )
                .ok()?;
                let id = routes.split("id: '").nth(1)?.split('\'').next()?;
                Some(id.to_owned())
            });
        if !pages.contains(&page) {
            pages.push(page);
        }
    }
    pages
}

/// The checkout's dev server, serving its backend as it is now: the one running, else a new one,
/// once the one it replaces, checked to be the server it recorded, has stopped.
fn serving(root: &Path, runner: &dyn Runner) -> Result<dev::Status> {
    let status = dev::status(root, runner)?;
    if status.running && status.backend_matches == Some(true) {
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
    let mut command = Command::new("npm");
    command
        .args(["run", "dev"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&log)?)
        .stderr(fs::File::create(&log)?.try_clone()?)
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
        if status.running && status.backend_matches == Some(true) {
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
    let pages = match named.is_empty() {
        true => pages(root, &web),
        false => named.iter().map(|page| Some((*page).to_owned())).collect(),
    };
    if !pages.is_empty() {
        let status = serving(root, runner)?;
        for page in &pages {
            for size in ["1440x1000", "390x844"] {
                shot::run(
                    root,
                    &shot::Shot {
                        open: shot::Open {
                            page: page.as_deref(),
                            dsp: None,
                            platform: false,
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
