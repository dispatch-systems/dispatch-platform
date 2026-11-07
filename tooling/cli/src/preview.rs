//! What a change shows, approved before its PR opens. `preview` serves the checkout as it is and
//! saves a screenshot of each page the change touches, on the tab or Settings section it touches,
//! with a link to click through it;
//! `preview approve` keeps the UI files the user was shown, by their contents, once they
//! approved them, and `pr` opens no PR for a change to what a page shows until those are the
//! files it would push.
use crate::{Result, Runner, api, dev, require, review, shot, workspace};
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
/// own pages or a DSP's, on one of its tabs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub id: Option<String>,
    pub platform: bool,
    pub tab: Option<Tab>,
}
/// A tab a page opens on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    /// By its id, as the page's tab strip or the address (`?tab=`) names it.
    Id(String),
    /// A section of Settings, by its id: the tab is the page of Settings the server puts it on.
    Section(String),
}
impl Page {
    /// A page as `--pages` names it: `<id>`, or `platform:<id>` for the platform owner's own,
    /// each with `/<tab>` for one of its tabs.
    pub fn named(name: &str) -> Self {
        let (platform, name) = match name.strip_prefix("platform:") {
            Some(name) => (true, name),
            None => (false, name),
        };
        let (id, tab) = match name.split_once('/') {
            Some((id, tab)) => (id, Some(Tab::Id(tab.to_owned()))),
            None => (name, None),
        };
        Self {
            id: Some(id.to_owned()),
            platform,
            tab,
        }
    }
}

/// A route a `feature.ts` declares: its id, whether it's the platform owner's, and whether it
/// draws a DSP's Settings.
#[derive(Debug, PartialEq, Eq)]
struct Route {
    id: String,
    platform: bool,
    settings: bool,
}

/// The routes a `feature.ts` declares in its `routes: [...]`: the id and scope at each entry's
/// own level, not those of what it holds.
fn routes(text: &str) -> Vec<Route> {
    entries(text, "routes")
        .iter()
        .filter_map(|entry| {
            Some(Route {
                id: field(entry, "id")?,
                platform: field(entry, "scope").as_deref() == Some("platform"),
                settings: entry.contains("hostsSettings: true"),
            })
        })
        .collect()
}
/// The ids of the Settings sections a `feature.ts` draws in its `settingsSections: [...]`.
fn sections(text: &str) -> Vec<String> {
    entries(text, "settingsSections")
        .iter()
        .filter_map(|entry| field(entry, "id"))
        .collect()
}
/// The text of each entry of the list a `feature.ts` declares as `<key>: [...]`, each entry's
/// own: what's nested in it is left out.
fn entries(text: &str, key: &str) -> Vec<String> {
    let opening = format!("{key}: [");
    let Some(start) = text.find(&opening) else {
        return vec![];
    };
    let mut entries = vec![];
    let (mut depth, mut entry) = (0usize, String::new());
    for c in text[start + opening.len() - 1..].chars() {
        match c {
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                if depth == 1 && c == '}' {
                    entries.push(std::mem::take(&mut entry));
                }
                continue;
            }
            _ => {}
        }
        if depth == 2 && !matches!(c, '{') {
            entry.push(c);
        }
    }
    entries
}
/// The quoted value of `name` in an entry's own text.
fn field(entry: &str, name: &str) -> Option<String> {
    let rest = entry.split(&format!("{name}: '")).nth(1)?;
    Some(rest.split('\'').next()?.to_owned())
}
/// A name as file and folder names spell it, and ids: its letters and digits alone, lowercase.
fn plain(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}
/// A component's name as a tab's id: `MealBreaks` is `meal-breaks`.
fn kebab(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() && !id.is_empty() {
            id.push('-');
        }
        id.push(c.to_ascii_lowercase());
    }
    id
}

/// The page a UI file belongs to. A file its owner's Settings section is named for, by its
/// folder under `frontend/` or its own name, shows that section; so does any of an owner with
/// sections and no routes, its first section. Otherwise, of the owner's routes, the one its folder
/// is named for, else the first, on the tab a folder under `tabs/` or a `<Name>Tab` file is named
/// for; the DSP's landing page when its owner has none.
fn page(root: &Path, path: &str) -> Page {
    let landing = Page::default();
    let Some((owner, inside)) = path.split_once("/frontend/") else {
        return landing;
    };
    let Ok(text) = fs::read_to_string(root.join(owner).join("frontend/feature.ts")) else {
        return landing;
    };
    let routes = routes(&text);
    let sections = sections(&text);
    let folder = inside.split('/').next().unwrap_or_default();
    let stem = Path::new(inside)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let named = ["Tab", "Page", "Section", "Settings"]
        .iter()
        .find_map(|suffix| stem.strip_suffix(suffix))
        .unwrap_or(stem);
    let section = sections
        .iter()
        .find(|id| {
            let what = plain(id.rsplit('.').next().unwrap_or(id));
            what == plain(folder) || what == plain(named)
        })
        .or(sections.first().filter(|_| routes.is_empty()));
    if let Some(section) = section {
        return Page {
            tab: Some(Tab::Section(section.clone())),
            ..landing
        };
    }
    let Some(route) = routes.iter().find(|r| r.id == folder).or(routes.first()) else {
        return landing;
    };
    let tab = match inside.strip_prefix("tabs/") {
        Some(rest) => Path::new(rest)
            .components()
            .next()
            .and_then(|first| Path::new(first.as_os_str()).file_stem()?.to_str())
            .map(str::to_owned),
        None => stem.strip_suffix("Tab").map(kebab),
    };
    Page {
        id: Some(route.id.clone()),
        platform: route.platform,
        tab: tab.map(Tab::Id),
    }
}

/// The route that draws a DSP's Settings, of every owner's.
fn settings_route(root: &Path) -> Option<String> {
    let owners = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|kind| !kind.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|kind| fs::read_dir(kind.path()).ok())
        .flatten()
        .flatten();
    owners
        .filter_map(|owner| fs::read_to_string(owner.path().join("frontend/feature.ts")).ok())
        .flat_map(|text| routes(&text))
        .find(|route| route.settings)
        .map(|route| route.id)
}
/// The page of Settings the server puts `section` on, as the owner sees the first DSP's.
fn settings_page(root: &Path, section: &str, runner: &dyn Runner) -> Result<Option<String>> {
    let server = api::Server::find(root)?;
    let (_, session) = api::sign_in(&server, api::OWNER, None, runner)?;
    let dsp = session["dsps"][0]["id"]
        .as_str()
        .ok_or("The dev server has no DSP to open.")?;
    let answer = api::request(
        &server,
        api::Call {
            method: "GET",
            path: "/api/dsp/settings",
            dsp: Some(dsp),
            who: None,
            data: None,
        },
        runner,
    )?;
    require(
        answer.status == 200,
        &format!("Settings answered {}: {}", answer.status, answer.body),
    )?;
    let settings: serde_json::Value = serde_json::from_str(&answer.body)?;
    Ok(placed(&settings, section))
}
/// The page a Settings answer puts `section` on.
fn placed(settings: &serde_json::Value, section: &str) -> Option<String> {
    settings["sections"]
        .as_array()?
        .iter()
        .find(|each| each["id"] == section)?["page"]
        .as_str()
        .map(str::to_owned)
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
            let (id, tab) = match &page.tab {
                Some(Tab::Section(section)) => {
                    let id = settings_route(root).ok_or("No route draws a DSP's Settings.")?;
                    let tab = settings_page(root, section, runner)?;
                    if tab.is_none() {
                        println!(
                            "Settings shows the owner no section {section}: shown on Settings' \
                             first page."
                        );
                    }
                    (Some(id), tab)
                }
                Some(Tab::Id(tab)) => (page.id.clone(), Some(tab.clone())),
                None => (page.id.clone(), None),
            };
            for size in ["1440x1000", "390x844"] {
                shot::run(
                    root,
                    &shot::Shot {
                        open: shot::Open {
                            page: id.as_deref(),
                            dsp: None,
                            platform: page.platform,
                            tab: tab.as_deref(),
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
    fn a_ui_file_shows_the_page_tab_or_settings_section_its_owner_draws_it_on() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let owner = |path: &str, declared: &str| {
            let file = root.join(path).join("frontend/feature.ts");
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, declared).unwrap();
        };
        owner(
            "features/timecard",
            "routes: [{ id: 'paycom', scope: 'dsp', label: 'Timecard' }]",
        );
        owner(
            "features/team",
            "routes: [{ id: 'team', scope: 'dsp', render: ({ view }) => draw({ view }) }]",
        );
        // Ids outside `routes`, and those of what a route holds, aren't pages.
        owner(
            "core/support",
            "accountActions: [{ id: 'ai-support', label: 'AI Support' }],\n\
             routes: [{ id: 'support', scope: 'platform', tabs: [{ id: 'inbox', scope: 'dsp' }] }]",
        );
        owner(
            "core/collection",
            "settingsSections: [\n\
               { id: 'collection.connections', render: ({ view }) => tab({ view }) },\n\
               { id: 'collection.collectors', load },\n\
             ],",
        );
        owner(
            "features/communications",
            "routes: [{ id: 'communications', scope: 'dsp' }],\n\
             settingsSections: [{ id: 'communications.display' }, { id: 'communications.quick_replies' }],",
        );
        owner(
            "features/settings",
            "routes: [{ id: 'settings', scope: 'dsp', hostsSettings: true }]",
        );
        owner(
            "core/platform_owner",
            "routes: [{ id: 'dsps', scope: 'platform' }, { id: 'jobs', scope: 'platform' }]",
        );
        let at = |id: &str, platform, tab: Option<&str>| Page {
            id: Some(id.to_owned()),
            platform,
            tab: tab.map(|tab| Tab::Id(tab.to_owned())),
        };
        let section = |id: &str| Page {
            tab: Some(Tab::Section(id.to_owned())),
            ..Page::default()
        };
        for (path, shown) in [
            (
                "features/timecard/frontend/TimecardPage.tsx",
                at("paycom", false, None),
            ),
            (
                "features/timecard/frontend/tabs/employees/EmployeeList.tsx",
                at("paycom", false, Some("employees")),
            ),
            (
                "features/team/frontend/RolesTab.tsx",
                at("team", false, Some("roles")),
            ),
            (
                "core/platform_owner/frontend/jobs/JobsPage.tsx",
                at("jobs", true, None),
            ),
            (
                "core/platform_owner/frontend/shared/Table.tsx",
                at("dsps", true, None),
            ),
            (
                "core/support/frontend/owner/SupportPage.tsx",
                at("support", true, None),
            ),
            (
                "core/collection/frontend/ConnectionsTab.tsx",
                section("collection.connections"),
            ),
            (
                "core/collection/frontend/collectors/Schedule.tsx",
                section("collection.collectors"),
            ),
            // An owner with sections and no pages shows its first.
            (
                "core/collection/frontend/connections.css",
                section("collection.connections"),
            ),
            (
                "features/communications/frontend/display/DisplaySettings.tsx",
                section("communications.display"),
            ),
            (
                "features/communications/frontend/inbox/Inbox.tsx",
                at("communications", false, None),
            ),
            ("core/shell/frontend/styles/tokens.css", Page::default()),
        ] {
            assert_eq!(page(root, path), shown, "{path}");
        }
        assert_eq!(settings_route(root).as_deref(), Some("settings"));
        assert_eq!(Page::named("platform:dsps"), at("dsps", true, None));
        assert_eq!(Page::named("paycom"), at("paycom", false, None));
        assert_eq!(
            Page::named("paycom/meal-breaks"),
            at("paycom", false, Some("meal-breaks"))
        );
        assert_eq!(
            Page::named("platform:support/inbox"),
            at("support", true, Some("inbox"))
        );
    }
    #[test]
    fn a_settings_section_opens_on_the_page_the_server_puts_it_on() {
        let settings = serde_json::json!({"pages": [{"id": "connections"}], "sections": [
            {"id": "collection.connections", "page": "connections"},
            {"id": "timecard.roster", "page": "timecard"},
        ]});
        assert_eq!(
            placed(&settings, "timecard.roster").as_deref(),
            Some("timecard")
        );
        assert_eq!(placed(&settings, "slack.workspaces"), None);
    }
}
