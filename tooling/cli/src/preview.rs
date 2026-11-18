//! What a change shows, approved before its PR opens. `preview` serves the checkout as it is and
//! saves a screenshot of each page the change touches, on the tab or Settings section it touches,
//! each with a link that opens on that page; the server runs on in a session of its own, at the
//! same address each time it starts again, until `preview stop` or `finish` stops it;
//! `preview approve` keeps the UI files the user was shown, by their contents, once they
//! approved them, or with `--unchanged` and why, those of a change that shows nothing new, which
//! the PR then says; and `pr` opens no PR for a change to what a page shows until those are the
//! files it would push.
use crate::{Result, Runner, api, dev, require, review, shot, workspace};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    net::TcpListener,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Where an approval is kept, in the checkout's scratch folder.
const APPROVED: &str = "preview/approved.json";
/// The UI files as the last `preview` showed them, so a change it showed is the user's to answer.
const SHOWN: &str = "preview/shown.json";
/// Why the approved files show nothing new, when they were approved without being shown.
const UNCHANGED: &str = "preview/unchanged.txt";

/// A change's UI files against what the user approved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Approval {
    /// The change touches nothing a page shows.
    None,
    /// The files are the ones approved.
    Approved,
    /// The files are the ones approved, unseen, as showing nothing new, for the reason given.
    Unchanged(String),
    /// Nothing was approved yet.
    Missing,
    /// These differ from what was approved.
    Changed(Vec<String>),
}

/// Each UI file the change touches against `origin/main`, by the hash of its contents, or
/// `deleted`: committed, or as the worktree holds it with `worktree`.
fn files(root: &Path, worktree: bool, runner: &dyn Runner) -> Result<BTreeMap<String, String>> {
    let git = |args: &[&str]| workspace::git(runner, root, args);
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
    if !changed.is_empty() {
        return Ok(Approval::Changed(changed));
    }
    let scratch = workspace::checkout_scratch(root)?;
    Ok(match fs::read_to_string(scratch.join(UNCHANGED)) {
        Ok(why) => Approval::Unchanged(why.trim().to_owned()),
        Err(_) => Approval::Approved,
    })
}

/// Whether the last `preview` showed the UI files as the change commits them now.
pub fn shown(root: &Path, runner: &dyn Runner) -> Result<bool> {
    let path = workspace::checkout_scratch(root)?.join(SHOWN);
    Ok(match fs::read_to_string(&path) {
        Ok(text) => {
            serde_json::from_str::<BTreeMap<String, String>>(&text)? == files(root, false, runner)?
        }
        Err(_) => false,
    })
}

/// `dispatchdev preview approve`: the UI files as the worktree holds them, kept as what the user
/// approved. Run only once they said so; or, `unchanged`, kept unseen with why they show
/// nothing new, for `pr` to tell the user in the PR.
pub fn approve(root: &Path, unchanged: Option<&str>, runner: &dyn Runner) -> Result<()> {
    let files = files(root, true, runner)?;
    require(
        !files.is_empty(),
        "The change touches nothing a page shows: there is nothing to approve.",
    )?;
    let scratch = workspace::checkout_scratch(root)?;
    let why = scratch.join(UNCHANGED);
    match unchanged.map(str::trim) {
        Some(reason) => {
            require(
                !reason.is_empty() && !reason.contains('\n'),
                "--unchanged says on one line why nothing a page shows changed.",
            )?;
            workspace::write_private(&why, reason)?;
        }
        None if why.exists() => fs::remove_file(&why)?,
        None => {}
    }
    workspace::write_private(
        &scratch.join(APPROVED),
        &serde_json::to_string_pretty(&files)?,
    )?;
    println!(
        "Kept {} UI file{} as {}: {}.",
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        match unchanged {
            Some(_) => "showing nothing new",
            None => "approved",
        },
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
/// A Settings section an owner draws, by its id, with the modules its `load` imports.
struct Section {
    id: String,
    modules: Vec<PathBuf>,
}
/// The Settings sections the `feature.ts` at `file` draws in its `settingsSections: [...]`, each
/// written there or a name it imports from another file.
fn sections(file: &Path, text: &str) -> Vec<Section> {
    items(text, "settingsSections")
        .iter()
        .filter_map(|item| match item.starts_with('{') {
            true => section(file, text, item),
            false => {
                let (from, text) = imported(file, text, item)?;
                section(&from, &text, object(&text, item)?)
            }
        })
        .collect()
}
/// A section as `item`, in the file at `file` that holds `text`, declares it: its id, and what
/// its `load` imports, there or in the function it names.
fn section(file: &Path, text: &str, item: &str) -> Option<Section> {
    let id = field(item, "id")?;
    let mut modules = references(file, item);
    if modules.is_empty() {
        let name = match item.split("load: ").nth(1) {
            Some(rest) => rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()?
                .to_owned(),
            None => "load".to_owned(),
        };
        let start = text.find(&format!("const {name} = "))?;
        let statement = text[start..].split(";\n").next()?;
        modules = references(file, statement);
    }
    Some(Section { id, modules })
}
/// The file `name` is imported from in the file at `file`, which holds `text`, and what it holds.
fn imported(file: &Path, text: &str, name: &str) -> Option<(PathBuf, String)> {
    text.split("import {").skip(1).find_map(|import| {
        let (names, rest) = import.split_once("} from '")?;
        if !names.split(',').any(|each| each.trim() == name) {
            return None;
        }
        let from = resolve(file.parent()?, rest.split('\'').next()?)?;
        let text = fs::read_to_string(&from).ok()?;
        Some((from, text))
    })
}
/// The object `const <name>` is declared as in `text`, from its `{` to its `}`.
fn object<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("const {name}"))?;
    let open = start + text[start..].find('{')?;
    let mut depth = 0;
    for (at, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open..=open + at]);
                }
            }
            _ => {}
        }
    }
    None
}
/// Each item of the list `<key>: [...]` in `text`, whole, as written between its commas.
fn items(text: &str, key: &str) -> Vec<String> {
    let opening = format!("{key}: [");
    let Some(start) = text.find(&opening) else {
        return vec![];
    };
    let mut items = vec![];
    let (mut depth, mut quote, mut item) = (0usize, None, String::new());
    for c in text[start + opening.len()..].chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"' | '`') => quote = Some(c),
            (None, '[' | '{' | '(') => depth += 1,
            (None, ']' | '}' | ')') if depth == 0 => break,
            (None, ']' | '}' | ')') => depth -= 1,
            (None, ',') if depth == 0 => {
                items.push(std::mem::take(&mut item));
                continue;
            }
            _ => {}
        }
        item.push(c);
    }
    items.push(item);
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}
/// The files a module's text imports by a relative path, `import ... from`, a stylesheet's
/// `import` and `import(...)`, as the file at `file` reaches them.
fn references(file: &Path, text: &str) -> Vec<PathBuf> {
    imports(file, text, &["from '", "import '", "import('"])
}
/// The files a module's text loads when they're needed, `import(...)`: a page's, as its owner's
/// `feature.ts` loads it.
fn loaded(file: &Path, text: &str) -> Vec<PathBuf> {
    imports(file, text, &["import('"])
}
/// The files a module's text imports by a relative path after one of `keywords`.
fn imports(file: &Path, text: &str, keywords: &[&str]) -> Vec<PathBuf> {
    let Some(folder) = file.parent() else {
        return vec![];
    };
    keywords
        .iter()
        .flat_map(|keyword| {
            text.match_indices(keyword)
                .map(move |(at, _)| at + keyword.len())
        })
        .filter_map(|at| text[at..].split('\'').next())
        .filter(|spec| spec.starts_with('.'))
        .filter_map(|spec| resolve(folder, spec))
        .collect()
}
/// The file a relative import names from `folder`: as written, or for a `.js`, the TypeScript
/// it's built from.
fn resolve(folder: &Path, spec: &str) -> Option<PathBuf> {
    let path = workspace::normal(&folder.join(spec));
    let bare = path.with_extension("");
    [
        path.clone(),
        bare.with_extension("ts"),
        bare.with_extension("tsx"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
/// Every file in `owner`'s folder that `modules` reach by their imports, themselves too.
fn reach(owner: &Path, modules: &[PathBuf]) -> HashSet<PathBuf> {
    let mut reached = HashSet::new();
    let mut next = modules.to_vec();
    while let Some(file) = next.pop() {
        if !file.starts_with(owner) || !reached.insert(file.clone()) {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&file) {
            next.extend(references(&file, &text));
        }
    }
    reached
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

/// The page a UI file belongs to. A file a Settings section of its owner's reaches by its imports,
/// and nothing else its owner's `feature.ts` loads when needed does, as its pages, shows that
/// section; so does any of an
/// owner with sections and no routes, its first section. Otherwise, of the owner's routes, the one
/// its folder under `frontend/` is named for, else the first, on the tab a folder under `tabs/` or
/// a `<Name>Tab` file is named for; the DSP's landing page when its owner has none.
fn page(root: &Path, path: &str) -> Page {
    let landing = Page::default();
    let Some((owner, inside)) = path.split_once("/frontend/") else {
        return landing;
    };
    let manifest = root.join(owner).join("frontend/feature.ts");
    let Ok(text) = fs::read_to_string(&manifest) else {
        return landing;
    };
    let routes = routes(&text);
    let sections = sections(&manifest, &text);
    let (owner, file) = (
        workspace::normal(&root.join(owner)),
        workspace::normal(&root.join(path)),
    );
    let drawn: Vec<&PathBuf> = sections.iter().flat_map(|s| &s.modules).collect();
    let others: Vec<PathBuf> = loaded(&manifest, &text)
        .into_iter()
        .filter(|module| !drawn.contains(&module))
        .collect();
    let section = match reach(&owner, &others).contains(&file) {
        true => None,
        false => sections
            .iter()
            .find(|section| reach(&owner, &section.modules).contains(&file)),
    }
    .or(sections.first().filter(|_| routes.is_empty()));
    if let Some(section) = section {
        return Page {
            tab: Some(Tab::Section(section.id.clone())),
            ..landing
        };
    }
    let folder = inside.split('/').next().unwrap_or_default();
    let stem = Path::new(inside)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
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

/// The server `preview` started last, kept in the checkout's scratch folder once it ends too, so
/// the next one serves at the same address and the links sent before it still open.
const SERVER: &str = "preview/server.json";
/// What `preview` kept of the server it started.
#[derive(Deserialize, Serialize)]
struct Started {
    /// Its session's leader, the process `preview` started, by its pid, and when it started and
    /// its command line as `ps` told them once it served.
    pid: i32,
    started: String,
    command: String,
    host: String,
    port: u16,
    /// What follows `/__preview/` in the link that signs the owner in.
    entry: String,
}
impl Started {
    fn origin(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}
/// The server `preview` started in a checkout, while it runs.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Server {
    pub pid: i32,
    pub origin: String,
}
impl Server {
    /// How `status` and `owed` tell it, so no server runs on unnoticed.
    pub fn line(&self) -> String {
        format!(
            "Dev server: {} (pid {}), until `dispatchdev preview stop` or `finish` stops it",
            self.origin, self.pid
        )
    }
}
fn started(scratch: &Path) -> Option<Started> {
    serde_json::from_str(&fs::read_to_string(scratch.join(SERVER)).ok()?).ok()
}
/// The server `preview` started that still runs, the process its scratch folder at `scratch`
/// names, by when it started and its command line, not a later one given its pid.
fn running(scratch: &Path, runner: &dyn Runner) -> Option<Started> {
    started(scratch).filter(|started| {
        dev::identity(started.pid, runner).is_some_and(|now| {
            (now.started.as_str(), now.command.as_str())
                == (started.started.as_str(), started.command.as_str())
        })
    })
}
/// The server `preview` started in the checkout whose scratch folder is `scratch`, while it runs.
pub fn server(scratch: &Path, runner: &dyn Runner) -> Option<Server> {
    running(scratch, runner).map(|started| Server {
        pid: started.pid,
        origin: started.origin(),
    })
}
/// Starts `command` in a session of its own, both its streams in `log`: the end of the command
/// or the agent's turn that started it signals neither it nor what it starts.
pub(crate) fn detached(command: &mut Command, log: &Path) -> Result<Child> {
    // One file for both streams, so neither writes over the other.
    let output = fs::File::create(log)?;
    command
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    // SAFETY: setsid is async-signal-safe, and nothing else runs between fork and exec.
    unsafe {
        command.pre_exec(|| match libc::setsid() {
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    Ok(command.spawn()?)
}
/// Keeps the server whose session `pid` leads, serving at `host:port` with `entry`, as the one
/// `preview` started.
pub(crate) fn keep(
    scratch: &Path,
    pid: i32,
    host: &str,
    port: u16,
    entry: &str,
    runner: &dyn Runner,
) -> Result<()> {
    let process = dev::identity(pid, runner)
        .ok_or_else(|| format!("The dev server {pid} stopped as it started."))?;
    let started = Started {
        pid,
        started: process.started,
        command: process.command,
        host: host.to_owned(),
        port,
        entry: entry.to_owned(),
    };
    workspace::write_private(
        &scratch.join(SERVER),
        &serde_json::to_string_pretty(&started)?,
    )
}
/// Stops the checkout's dev server: the one `preview` started, found by the record in `scratch`
/// and checked to be that process, with everything its session started; then one started by
/// hand, as `dev::stop` checks it. Answers the process it stopped, if one ran.
pub fn stop(root: &Path, scratch: &Path, runner: &dyn Runner) -> Result<Option<i32>> {
    let leader = running(scratch, runner).map(|started| started.pid);
    if let Some(pid) = leader {
        // Its session is its process group, which a session's leader never leaves.
        unsafe { libc::kill(-pid, libc::SIGTERM) };
        let stopping = Instant::now();
        // Until no process is left in the group.
        while unsafe { libc::kill(-pid, 0) } == 0 {
            require(
                stopping.elapsed() < Duration::from_secs(30),
                &format!("The dev server {pid} didn't stop: stop it, then run this again."),
            )?;
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let by_hand = dev::stop(root, runner)?;
    Ok(leader.or(by_hand))
}
/// `dispatchdev preview stop`.
pub fn stop_serving(root: &Path, runner: &dyn Runner) -> Result<()> {
    match stop(root, &workspace::checkout_scratch(root)?, runner)? {
        Some(pid) => println!("Stopped the dev server {pid}."),
        None => println!("No dev server runs in this checkout."),
    }
    Ok(())
}
/// The port a new server listens on: the last one's, so its links still open, unless something
/// else holds it now; then any.
fn port(host: &str, earlier: Option<&Started>) -> u16 {
    earlier
        .map(|earlier| earlier.port)
        .filter(|port| TcpListener::bind((host, *port)).is_ok())
        .unwrap_or(0)
}

/// The checkout's dev server, serving its backend as it is now: the one running, else a new one,
/// once the one it replaces, checked to be the server it recorded, has stopped. A new one runs
/// apart from this command, at the address the last one served at, until `finish` or `preview
/// stop` stops it.
fn serving(root: &Path, runner: &dyn Runner) -> Result<dev::Status> {
    // Only a server started from the checkout's files as they are now, its pages' code aside,
    // serves it: a newer build on disk says nothing of what an older server runs.
    let status = dev::status(root, runner)?;
    if status.running && status.source_matches == Some(true) {
        return Ok(status);
    }
    // The one `preview` started too, though its record in the checkout is gone.
    let scratch = workspace::checkout_scratch(root)?;
    if let Some(pid) = stop(root, &scratch, runner)? {
        println!("Stopped the dev server {pid}: it served other code.");
    }
    // Served on the tailnet's address, so the link opens on the user's phone or laptop.
    let host = workspace::text(runner, &["tailscale", "ip", "-4"], Some(root))
        .ok()
        .and_then(|host| host.lines().next().map(str::to_owned))
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    let earlier = started(&scratch);
    let port = port(&host, earlier.as_ref());
    let log = scratch.join("dev-server.log");
    let mut command = Command::new("npm");
    command
        .args(["run", "dev"])
        .current_dir(root)
        .env("DISPATCH_DEV_HOST", &host)
        .env("DISPATCH_DEV_PORT", port.to_string());
    if let Some(earlier) = &earlier {
        command.env("DISPATCH_DEV_ENTRY", &earlier.entry);
    }
    let mut child = detached(&mut command, &log)?;
    println!("Starting the dev server; its log is {}.", log.display());
    let starting = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if let Some(exit) = child.try_wait()? {
            return Err(format!("The dev server stopped ({exit}): see {}.", log.display()).into());
        }
        let status = dev::status(root, runner)?;
        if status.running
            && status.source_matches == Some(true)
            && let (Some(origin), Some(link)) = (&status.origin, &status.link)
        {
            let entry = link
                .strip_prefix(&format!("{origin}/__preview/"))
                .ok_or("The dev server's link isn't its own.")?;
            let served = origin
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse().ok())
                .ok_or("The dev server's address has no port.")?;
            keep(&scratch, child.id() as i32, &host, served, entry, runner)?;
            if let Some(earlier) = earlier.filter(|earlier| earlier.origin() != *origin) {
                println!(
                    "It serves at {origin}, not {}, which something else holds: the links sent \
                     before don't open.",
                    earlier.origin()
                );
            }
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
/// it touches, or `pages`, wide and on a phone, each with a link that opens on it.
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
            let shoot = |size| {
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
                )
            };
            let opened = shoot(shot::DESKTOP)?;
            shoot(shot::PHONE)?;
            // The link signs in and the browser keeps its fragment, so it opens on the page.
            if let Some(link) = &status.link {
                println!("Its link, for the chat alone:\n{link}{opened}");
            }
        }
    }
    let android = files.keys().any(|path| path.starts_with("android/"));
    // Its screenshots are what the agent sends next, in the same reply: the last step the CLI
    // sees of showing the change. An app's screens come from the emulator, which it never sees.
    if !pages.is_empty() && !android {
        let shown = workspace::checkout_scratch(root)?.join(SHOWN);
        workspace::write_private(&shown, &serde_json::to_string_pretty(&files)?)?;
    }
    let mut next = vec![];
    if android {
        next.push("It changes an app's screens too: show them from the emulator".to_owned());
    }
    next.push(
        "Show the user each page's screenshots with its link under them; make what they ask and \
         run this again. Once they approve, `dispatchdev preview approve`"
            .to_owned(),
    );
    println!("{}", workspace::next(&next));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Native;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn the_server_preview_starts_runs_in_a_session_of_its_own_until_stopped_with_what_it_started() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        for args in [
            &["git", "init", "-qb", "main"][..],
            &[
                "git",
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "x",
                "--allow-empty",
            ],
        ] {
            Native.command(args, Some(&root), 30).unwrap();
        }
        let scratch = root.join("scratch");
        let log = root.join("dev-server.log");
        // A server and what it starts, which outlive the command that started them.
        let child = detached(
            Command::new("sh").args(["-c", "echo serving; sleep 30 & wait"]),
            &log,
        )
        .unwrap();
        let pid = child.id() as i32;
        let ended = std::thread::spawn(move || {
            let mut child = child;
            child.wait().unwrap()
        });
        assert_eq!(unsafe { libc::getsid(pid) }, pid, "a session of its own");
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let entry = "0b7a2c1e-5d4f-4e3a-9c8b-7a6f5e4d3c2b";
        keep(&scratch, pid, "127.0.0.1", port, entry, &Native).unwrap();
        assert_eq!(
            server(&scratch, &Native),
            Some(Server {
                pid,
                origin: format!("http://127.0.0.1:{port}")
            })
        );
        // The next server takes the last one's port, unless something else holds it.
        let earlier = started(&scratch).unwrap();
        assert_eq!(earlier.entry, entry);
        assert_eq!(super::port("127.0.0.1", Some(&earlier)), 0);
        drop(held);
        // A process another test is starting can hold a copy of a socket a moment after it
        // closed, the one each look opens too.
        let deadline = Instant::now() + Duration::from_secs(10);
        while super::port("127.0.0.1", Some(&earlier)) != port {
            assert!(
                Instant::now() < deadline,
                "the port it was let go of is taken again"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(super::port("127.0.0.1", None), 0);
        // A process with its pid that started at another time isn't the server.
        let path = scratch.join(SERVER);
        let kept = fs::read_to_string(&path).unwrap();
        let mut other: serde_json::Value = serde_json::from_str(&kept).unwrap();
        other["started"] = "Mon Jan 1 00:00:00 2024".into();
        workspace::write_private(&path, &other.to_string()).unwrap();
        assert_eq!(server(&scratch, &Native), None);
        assert_eq!(stop(&root, &scratch, &Native).unwrap(), None);
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0, "left running");
        workspace::write_private(&path, &kept).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fs::read_to_string(&log).unwrap().contains("serving") {
            assert!(Instant::now() < deadline, "its output goes to its log");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(stop(&root, &scratch, &Native).unwrap(), Some(pid));
        assert_eq!(ended.join().unwrap().signal(), Some(libc::SIGTERM));
        assert_eq!(
            unsafe { libc::kill(-pid, 0) },
            -1,
            "nothing it started is left"
        );
        assert_eq!(server(&scratch, &Native), None);
        // Its address is kept for the next one.
        assert_eq!(started(&scratch).unwrap().port, port);
    }
    #[test]
    fn a_ui_file_shows_the_page_tab_or_settings_section_its_owner_draws_it_on() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let file = |path: &str, text: &str| {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        };
        let owner =
            |path: &str, declared: &str| file(&format!("{path}/frontend/feature.ts"), declared);
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
        // A section loads its module inline, by a function it names, or as `load`, and is written
        // in the list or imported by name; a file it reaches is its own unless a page reaches it.
        owner(
            "core/collection",
            "const load = () =>\n  Promise.all([import('./index.js'), ...cards()]).then(([m]) => m);\n\
             const loadCollectors = () => import('./collectors/index.js');\n\
             export const feature = {\n  settingsSections: [\n\
               { id: 'collection.connections', load, render: ({ view }) => tab({ view }) },\n\
               { id: 'collection.collectors', load: loadCollectors },\n  ],\n};",
        );
        file(
            "core/collection/frontend/index.ts",
            "import './connections.css';\nexport { ConnectionsTab } from './ConnectionsTab.js';",
        );
        file("core/collection/frontend/ConnectionsTab.tsx", "");
        file("core/collection/frontend/connections.css", "");
        file(
            "core/collection/frontend/collectors/index.ts",
            "export { Schedule } from './Schedule.js';",
        );
        file("core/collection/frontend/collectors/Schedule.tsx", "");
        owner(
            "features/communications",
            "const load = () => import('./index.js');\n\
             const loadQuickReplies = () => import('./replies/QuickRepliesSettings.js');\n\
             export const feature = {\n  routes: [{ id: 'communications', scope: 'dsp', preload: load }],\n\
               settingsSections: [\n\
                 { id: 'communications.alerts', load: () => import('./alerts/Alerts.js') },\n\
                 { id: 'communications.quick_replies', load: loadQuickReplies },\n  ],\n};",
        );
        file(
            "features/communications/frontend/index.ts",
            "export { Page } from './page/Page.js';",
        );
        file(
            "features/communications/frontend/page/Page.tsx",
            "import { QuickReplies } from '../replies/QuickReplies.js';\n\
             import { notices } from '../alerts/notices.js';",
        );
        file(
            "features/communications/frontend/replies/QuickReplies.tsx",
            "import './quick-replies.css';",
        );
        file(
            "features/communications/frontend/replies/quick-replies.css",
            "",
        );
        file(
            "features/communications/frontend/replies/QuickRepliesSettings.tsx",
            "",
        );
        file(
            "features/communications/frontend/alerts/Alerts.tsx",
            "import { notices } from './notices.js';\nimport './alerts.css';",
        );
        file("features/communications/frontend/alerts/alerts.css", "");
        file("features/communications/frontend/alerts/notices.ts", "");
        owner(
            "core/accounts",
            "import { profileSection, securitySection } from './settings/sections.js';\n\
             export const feature = { settingsSections: [profileSection, securitySection] };",
        );
        file(
            "core/accounts/frontend/settings/sections.ts",
            "const loadProfile = () => import('./ProfileBadge.js');\n\
             const loadSecurity = () => import('./SecuritySettings.js');\n\
             export const profileSection: CustomSection = { id: 'accounts.profile', load: loadProfile };\n\
             export const securitySection: CustomSection = { id: 'accounts.security', load: loadSecurity };",
        );
        file("core/accounts/frontend/settings/ProfileBadge.tsx", "");
        file(
            "core/accounts/frontend/settings/SecuritySettings.tsx",
            "import { PasswordPanel } from './PasswordPanel.js';",
        );
        file("core/accounts/frontend/settings/PasswordPanel.tsx", "");
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
            (
                "core/collection/frontend/connections.css",
                section("collection.connections"),
            ),
            // An owner with sections and no pages shows its first for a file none reaches.
            (
                "core/collection/frontend/useVerification.ts",
                section("collection.connections"),
            ),
            (
                "features/communications/frontend/replies/QuickRepliesSettings.tsx",
                section("communications.quick_replies"),
            ),
            (
                "features/communications/frontend/alerts/alerts.css",
                section("communications.alerts"),
            ),
            // The page's own, though a section's module is beside it or reaches it too.
            (
                "features/communications/frontend/replies/QuickReplies.tsx",
                at("communications", false, None),
            ),
            (
                "features/communications/frontend/replies/quick-replies.css",
                at("communications", false, None),
            ),
            (
                "features/communications/frontend/alerts/notices.ts",
                at("communications", false, None),
            ),
            (
                "core/accounts/frontend/settings/PasswordPanel.tsx",
                section("accounts.security"),
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
