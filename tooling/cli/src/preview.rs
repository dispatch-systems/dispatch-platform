//! What a change shows, approved before its PR opens. `preview` serves the checkout as it is and
//! saves a screenshot of each page the change touches, on the tab or Settings section it touches,
//! with a link to click through it;
//! `preview approve` keeps the UI files the user was shown, by their contents, once they
//! approved them, and `pr` opens no PR for a change to what a page shows until those are the
//! files it would push.
use crate::{Result, Runner, api, dev, require, review, shot, workspace};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    os::unix::process::CommandExt,
    path::{Component, Path, PathBuf},
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
    let path = normal(&folder.join(spec));
    let bare = path.with_extension("");
    [
        path.clone(),
        bare.with_extension("ts"),
        bare.with_extension("tsx"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
/// A path with its `.` and `..` worked out, so a file reached two ways is named one way.
fn normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            part => normal.push(part),
        }
    }
    normal
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
    let (owner, file) = (normal(&root.join(owner)), normal(&root.join(path)));
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

/// The checkout's dev server, serving its backend as it is now: the one running, else a new one,
/// once the one it replaces, checked to be the server it recorded, has stopped.
fn serving(root: &Path, runner: &dyn Runner) -> Result<dev::Status> {
    // Only a server started from the checkout as it is now serves it: a newer build on disk
    // says nothing of what an older server runs.
    let status = dev::status(root, runner)?;
    if status.running && status.source_matches == Some(true) {
        return Ok(status);
    }
    if status.running
        && let Some(pid) = dev::stop(root, runner)?
    {
        println!("Stopped the dev server {pid}: it served other code.");
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
