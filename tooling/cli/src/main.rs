use dispatchdev_cli::{
    Native, Result, Runner, api, build,
    change::Change,
    check, ci, finish, logs,
    machine::Machine,
    observation, pr, preview, require, review, scaffold, ship, shot, start, status, test, verify,
    workspace::{Retried, Workspace},
};
use std::{collections::BTreeMap, path::PathBuf};

/// A command: how it is written, what it does, and what it takes. The help is written from
/// these, and each command's line is read by its own.
struct Spec {
    name: &'static str,
    /// How it is written, in the help's left column, a line or more.
    usage: &'static [&'static str],
    /// What it does, wrapped into the help's right column.
    about: &'static str,
    flags: &'static [&'static str],
    /// Options that take a value, given as `--option value` or `--option=value`.
    options: &'static [&'static str],
    /// How many arguments it takes at most.
    arguments: usize,
    /// Run by the checkout's own `tooling/scaffold/cli.ts`, which reads everything but
    /// `--root` itself.
    passed: bool,
}
const fn spec(name: &'static str, usage: &'static [&'static str], about: &'static str) -> Spec {
    Spec {
        name,
        usage,
        about,
        flags: &[],
        options: &[],
        arguments: 0,
        passed: false,
    }
}
impl Spec {
    const fn flags(mut self, flags: &'static [&'static str]) -> Self {
        self.flags = flags;
        self
    }
    const fn options(mut self, options: &'static [&'static str]) -> Self {
        self.options = options;
        self
    }
    const fn arguments(mut self, arguments: usize) -> Self {
        self.arguments = arguments;
        self
    }
    const fn passed(mut self) -> Self {
        self.passed = true;
        self
    }
    /// Its usage on one line: `preview <name> [--page <page>] [--restart | --stop]`.
    fn line(&self) -> String {
        self.usage
            .iter()
            .flat_map(|part| part.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ")
    }
    /// Its rows of the help: its usage beside what it does.
    fn rows(&self) -> Vec<String> {
        let mut about = vec![String::new()];
        for word in self.about.split_whitespace() {
            let last = about.last_mut().expect("a line");
            if !last.is_empty() && last.len() + 1 + word.len() > ABOUT {
                about.push(word.to_owned());
            } else {
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(word);
            }
        }
        (0..self.usage.len().max(about.len()))
            .map(|row| {
                let usage = self.usage.get(row).copied().unwrap_or("");
                let about = about.get(row).map(String::as_str).unwrap_or("");
                // A usage too long for its column has its line to itself.
                if usage.len() + 2 > USAGE && !about.is_empty() {
                    format!("  {usage}\n{:USAGE$} {about}", "")
                } else {
                    format!("  {usage:<width$} {about}", width = USAGE - 2)
                        .trim_end()
                        .to_owned()
                }
            })
            .collect()
    }
}
/// The help's columns: the usage's width, and the description's.
const USAGE: usize = 35;
const ABOUT: usize = 60;

const COMMANDS: &[Spec] = &[
    spec(
        "start",
        &["start <name> [--from <ref>]", "      [--preview]"],
        "A worktree and branch for the change from origin/main, its packages installed and its \
         scratch folder made, once the changes whose PRs merged are removed as finish would; \
         --preview, on dispatch-dev alone, also starts its preview's build without waiting for \
         it.",
    )
    .flags(&["--preview"])
    .options(&["--from"])
    .arguments(1),
    spec(
        "preview",
        &[
            "preview <name> [--page <page>]",
            "        [--tab <tab>]",
            "        [--dsp <dsp>]",
            "        [--platform]",
            "        [--restart | --stop]",
        ],
        "Start its preview, or with --restart after a backend edit, and print the link that \
         signs in by itself: to --page, in the fixture's DSP or the one --dsp names by its name \
         or id, or with --platform on the platform owner's own pages, on the page's --tab. It \
         runs on dispatch-dev alone.",
    )
    .flags(&["--platform", "--restart", "--stop"])
    .options(&["--page", "--dsp", "--tab"])
    .arguments(1),
    spec(
        "api",
        &[
            "api <name> <method> <path>",
            "    [--dsp <dsp>] [--as <email>]",
            "    [--data <json>]",
        ],
        "One signed-in call to its preview: as the owner, or --as a demo account; --dsp opens \
         a DSP by its name or id. It runs on dispatch-dev alone.",
    )
    .options(&["--dsp", "--as", "--data"])
    .arguments(3),
    spec(
        "shot",
        &[
            "shot <name> [--page <page>]",
            "     [--tab <tab>]",
            "     [--dsp <dsp> | --platform]",
            "     [--as <email>]",
            "     [--size <w>x<h>]",
            "     [--theme <light | dark>]",
        ],
        "A PNG of a page of its preview, saved in its scratch folder, whose path it prints: \
         opened as preview's link opens it, as the owner, or --as a demo account at the address \
         of the DSP --dsp names, signed in as api signs in, once a run of the preview. The \
         window is --size, 1440x1000 unless told, in --theme, light unless told. It runs on \
         dispatch-dev alone.",
    )
    .flags(&["--platform"])
    .options(&["--page", "--tab", "--dsp", "--as", "--size", "--theme"])
    .arguments(1),
    spec(
        "logs",
        &[
            "logs <name | dev> [--errors]",
            "     [--since <time>] [--follow]",
        ],
        "What its preview, or Dev, logged in its current run, an event a line and without the \
         build's output; --since 10m reads across runs, --errors keeps warnings and failures. \
         It runs on dispatch-dev alone.",
    )
    .flags(&["--errors", "--follow"])
    .options(&["--since"])
    .arguments(1),
    spec(
        "test",
        &[
            "test [--all | --owner <owner>]",
            "     [--only <kind>]",
            "     [--file <path>] [--keep]",
        ],
        "The tests the diff touches, or --all of them, one line each, stopping at the first \
         failure: for fixing a test, as the merge queue runs them. --owner runs one owner's of \
         every kind, as feature:<name>, collector:<site>, core:<part>, integration:<name>, app, \
         mcp or a name alone, or --only its rust, api, frontend, browser or native ones, or one \
         --file of them. Each run builds in a folder of its own, deleted when it ends unless \
         --keep. It runs on Linux alone: elsewhere, ci runs every check on GitHub.",
    )
    .flags(&["--all", "--keep"])
    .options(&["--owner", "--only", "--file"]),
    spec(
        "verify",
        &["verify <feature>", "       [--removability]"],
        "A feature in progress, committed or not: whether generate would change anything, \
         what a generator left to write, the rules, TypeScript, the changed files' format and \
         lint, then its own tests as test --owner runs them. --removability also leaves it \
         out as the weekly check does: on GitHub once the branch is pushed as it is, else in a \
         copy of the checkout. It runs on Linux alone: elsewhere, ci runs every check on GitHub.",
    )
    .flags(&["--removability"])
    .arguments(1),
    spec(
        "check",
        &["check [--plan]", "      [--allow-concurrent]"],
        "What stops the branch, then the rule checks, the TypeScript linter and clippy side by \
         side, clippy in the checkout's own build. They pass again without running on a \
         committed tree they passed on, as clippy does on Rust inputs it passed on; ci and the \
         merge queue run the tests. --plan only names them. It runs on Linux alone: elsewhere, \
         ci runs every check on GitHub.",
    )
    .flags(&["--plan", "--allow-concurrent"]),
    spec(
        "ci",
        &["ci <name> [--rerun-failed]", "   [--removability <feature>]"],
        "The full checks on GitHub on its committed branch, while its preview is tried: push \
         it, run them without the gate with the rule checks here beside them on Linux, and \
         follow them to the end, listing every failed job. Jobs GitHub stopped itself run again \
         once. ship stops on a head they failed and cancels them if still running. \
         --rerun-failed runs the failed jobs again, for a failure that isn't the change's own. \
         --removability runs the weekly removability check instead, for that feature alone.",
    )
    .flags(&["--rerun-failed"])
    .options(&["--removability"])
    .arguments(1),
    spec(
        "pr",
        &["pr <name> [--title <title>]", "   [--body <file> | --skeleton]"],
        "Check the title and body, push the branch, and open the PR or update it. The body is \
         the scratch folder's pr-body.md, or --body's file, and its first line, # <title>, \
         gives the title unless --title does. --skeleton --title instead starts pr-body.md with \
         the title and the sections its type needs, unless it's there already.",
    )
    .flags(&["--skeleton"])
    .options(&["--title", "--body"])
    .arguments(1),
    spec(
        "review",
        &["review <name> [--passes <n>]", "       [--done]"],
        "Only when a review is asked for: the brief of its next pass, on its committed head, \
         for the reviewer started through T3 (GPT-6 Astra, xhigh, Fast). Another pass follows \
         only one that found something to fix, and reviews only the fixes; --passes sets the \
         most, two unless told. --done, once the reviewer answered, checks it left the worktree \
         as it found it and counts its findings.",
    )
    .flags(&["--done"])
    .options(&["--passes"])
    .arguments(1),
    spec(
        "ship",
        &["ship <name>"],
        "Once told to merge it, by its name or its PR's number: check its PR body and that it \
         merges cleanly, push it, open or update its PR from pr-body.md, queue it as soon as \
         GitHub admits it and wait until it merges, the rule checks running here meanwhile on \
         Linux unless the checks on GitHub passed on this commit, then remove what the change \
         made as finish does, and wait for Dev. Run again after a stop, it picks up where it \
         stopped. It exits with 1 to fix and ship again, 2 for the owner to decide, and 3 once \
         merged.",
    )
    .arguments(1),
    spec(
        "finish",
        &["finish <name> [--abandon]"],
        "What ship does after the merge, for a change it didn't see through, or with --abandon \
         for one that never merged: its preview, port, worktree, build, branches and scratch \
         folder go; then wait for Dev.",
    )
    .flags(&["--abandon"])
    .arguments(1),
    spec(
        "status",
        &["status [--json]"],
        "Worktrees with their previews, PRs and reviews, open PRs, what Dev runs, free disk; \
         --json writes the same as JSON, to pick a change up after a break.",
    )
    .flags(&["--json"]),
    spec(
        "browser",
        &[
            "browser <dev | production>",
            "        --out <absolute-file>",
        ],
        "Mint 15-minute, read-only dashboard access into a new private JSON file. Its parent \
         directory must be private.",
    )
    .options(&["--out"])
    .arguments(1),
    spec(
        "new",
        &["new <kind> <name> [options]", "    [--dry-run] [--json]"],
        "In a change's worktree, a feature, collector, Slack integration, agent tool or kind of slot that works as \
         written, listed where it is read, then what is generated from it, if anything is: \
         dispatchdev new lists the kinds, new <kind> a kind's options. --dry-run prints what it \
         would write, from any checkout.",
    )
    .passed(),
    spec(
        "add",
        &["add <piece> --to <feature>", "    [--dry-run] [--json]"],
        "A piece new feature writes, added to a feature that exists: page, sidebar <pages>, \
         tab-of <feature>, settings, api, people <source> or contract <name>. Its slot \
         entries, fields and permissions join the feature's; what it can't merge it names. \
         dispatchdev add lists the pieces.",
    )
    .passed(),
    spec(
        "generate",
        &["generate [--check] [--json]"],
        "Write everything generated from what the owners declare: the app's and the MCP's \
         lists, the API types and the snapshots the app's tests hold, from one incremental \
         build in the worktree's own target folder. It names each file it changed; after a \
         failure, fix it and run it again. --check writes nothing, and fails naming what would \
         change.",
    )
    .passed(),
    spec(
        "list",
        &["list <what> [--json]"],
        "What the product has, as the build lists it: features, collectors, slots, exposes, \
         events, integrations, tools, sources, permissions or pages.",
    )
    .passed(),
    spec(
        "explain",
        &["explain <owner> [--json]"],
        "What a feature or collector is made of: what it uses and who uses it, its permissions, \
         pages, tables, routes, the slots it fills and the tools needing it.",
    )
    .passed(),
    spec(
        "doctor",
        &["doctor [--json]"],
        "Whether the checkout is ready to make and change things: its packages, the wiring, \
         Cargo.lock, what a generator left to write, how far behind origin/main it is, and free \
         disk, each with what fixes it.",
    )
    .passed(),
    spec(
        "todo",
        &["todo [<owner>] [--json]"],
        "What a generator left to write, by owner: each scaffold marker's file, line and what \
         it asks for, or one owner's: a name, or feature:, collector:, integration: or core: \
         with it, app or mcp.",
    )
    .passed(),
    spec(
        "build",
        &["build [--release | --fixture]", "      [--cache-key]"],
        "Build the backend and support worker, or the browser tests' assessment fixture, \
         reusing a build of identical inputs.",
    )
    .flags(&["--release", "--fixture", "--cache-key"]),
    spec("help", &["help [<command>]"], "Show this, or one command.").arguments(1),
];
/// The help's sections, each with its commands in order.
const SECTIONS: &[(&str, &[&str])] = &[
    (
        "A change, from start to finish:",
        &[
            "start", "preview", "api", "shot", "logs", "verify", "test", "check", "ci", "review",
            "pr", "ship", "finish", "status", "browser",
        ],
    ),
    (
        "Making things, and reading what the product has:",
        &[
            "new", "add", "generate", "list", "explain", "doctor", "todo",
        ],
    ),
    ("", &["build", "help"]),
];

fn find(name: &str) -> Option<&'static Spec> {
    COMMANDS.iter().find(|spec| spec.name == name)
}
fn help() -> String {
    let mut lines = vec!["Usage: dispatchdev <command>".to_owned()];
    for (heading, names) in SECTIONS {
        lines.push(String::new());
        if !heading.is_empty() {
            lines.push((*heading).to_owned());
        }
        for name in *names {
            lines.extend(find(name).expect("a listed command").rows());
        }
    }
    lines.push(String::new());
    lines.push(
        "verify, test, check, build, new, add, generate, list, explain, doctor and todo work on the\n\
         checkout the current directory is in, or on --root <directory>. dispatchdev <command>\n\
         --help shows one."
            .to_owned(),
    );
    lines.join("\n")
}

/// A command's line: its flags and options, and its other arguments, in order.
struct Line {
    spec: &'static Spec,
    flags: Vec<String>,
    options: BTreeMap<String, String>,
    arguments: Vec<String>,
}
/// Reads `args` as `spec` takes them, refusing what it doesn't take.
fn parse(spec: &'static Spec, args: Vec<String>) -> Result<Line> {
    let refuse = |what: &str| {
        format!(
            "{} takes no {what}. Run dispatchdev {} --help.",
            spec.name, spec.name
        )
    };
    let (mut flags, mut options, mut arguments) = (vec![], BTreeMap::new(), vec![]);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if !arg.starts_with("--") {
            arguments.push(arg);
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
            None => (arg.clone(), None),
        };
        if name == "--root" || spec.options.contains(&name.as_str()) {
            let value = match inline {
                Some(value) => value,
                None => args.next().ok_or_else(|| format!("{name} needs a value"))?,
            };
            options.insert(name, value);
        } else if spec.flags.contains(&name.as_str()) && inline.is_none() {
            flags.push(name);
        } else {
            return Err(refuse(&arg).into());
        }
    }
    if let Some(extra) = arguments.get(spec.arguments) {
        return Err(refuse(extra).into());
    }
    Ok(Line {
        spec,
        flags,
        options,
        arguments,
    })
}
impl Line {
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }
    fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
    fn usage(&self) -> Box<dyn std::error::Error + Send + Sync> {
        format!("Usage: dispatchdev {}", self.spec.line()).into()
    }
    /// The change a command names.
    fn name(&self) -> Result<&str> {
        self.arguments
            .first()
            .map(String::as_str)
            .ok_or_else(|| self.usage())
    }
    /// The checkout to work on: `--root`, or the one the current directory is in.
    fn root(&self) -> Result<PathBuf> {
        checkout(self.option("--root"))
    }
    fn workspace(&self) -> Result<Workspace> {
        Workspace::find(&std::env::current_dir()?, &Native)
    }
}
/// The checkout to work on: `root`, or the one the current directory is in, by its real path, as
/// git reports it.
fn checkout(root: Option<&str>) -> Result<PathBuf> {
    if let Some(root) = root {
        return Ok(std::fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root)));
    }
    let here = std::env::current_dir()?;
    Ok(Native
        .command(&["git", "rev-parse", "--show-toplevel"], Some(&here), 30)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|top| PathBuf::from(top.trim()))
        .filter(|top| top.is_dir())
        .unwrap_or(here))
}
/// A command the checkout's scaffolder runs: every argument but `--root` passes to it as given,
/// so its own lists of options are the only ones. It says why it failed itself.
fn passed(command: &str, args: Vec<String>) -> Result<()> {
    let (mut root, mut rest) = (None, vec![]);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--root" {
            root = Some(args.next().ok_or("--root needs a value")?);
        } else if let Some(value) = arg.strip_prefix("--root=") {
            root = Some(value.to_owned());
        } else {
            rest.push(arg);
        }
    }
    match scaffold::run(&checkout(root.as_deref())?, command, &rest)? {
        true => Ok(()),
        false => Err(Quiet.into()),
    }
}
/// A failure that has said why already.
#[derive(Debug)]
struct Quiet;
impl std::fmt::Display for Quiet {
    fn fmt(&self, _: &mut std::fmt::Formatter) -> std::fmt::Result {
        Ok(())
    }
}
impl std::error::Error for Quiet {}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(help)?;
    let rest: Vec<String> = args.collect();
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        match rest.first() {
            Some(name) => {
                let spec = find(name).ok_or_else(|| unknown(name))?;
                println!("Usage: dispatchdev {}\n\n{}", spec.line(), spec.about);
            }
            None => println!("{}", help()),
        }
        return Ok(());
    }
    let spec = find(&command).ok_or_else(|| unknown(&command))?;
    if spec.passed {
        return passed(spec.name, rest);
    }
    if rest.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: dispatchdev {}\n\n{}", spec.line(), spec.about);
        return Ok(());
    }
    let line = parse(spec, rest)?;
    match spec.name {
        "start" => start::run(
            &line.workspace()?,
            line.name()?,
            line.option("--from").unwrap_or("origin/main"),
            line.flag("--preview"),
            &Native,
        ),
        "preview" => {
            let action = match (line.flag("--restart"), line.flag("--stop")) {
                (true, true) => return Err("Choose --restart or --stop.".into()),
                (true, false) => preview::Action::Restart,
                (false, true) => preview::Action::Stop,
                (false, false) => preview::Action::Start,
            };
            let open = preview::Open {
                page: line.option("--page"),
                dsp: line.option("--dsp"),
                platform: line.flag("--platform"),
                tab: line.option("--tab"),
            };
            preview::run(&line.workspace()?, line.name()?, action, &open, &Native)
        }
        "api" => {
            let [name, method, path] = line.arguments.as_slice() else {
                return Err(line.usage());
            };
            let call = api::Call {
                method,
                path,
                dsp: line.option("--dsp"),
                who: line.option("--as"),
                data: line.option("--data"),
            };
            match api::run(&line.workspace()?, name, call, &Native)? {
                true => Ok(()),
                false => Err("The call failed.".into()),
            }
        }
        "shot" => {
            let shot = shot::Shot {
                open: preview::Open {
                    page: line.option("--page"),
                    dsp: line.option("--dsp"),
                    platform: line.flag("--platform"),
                    tab: line.option("--tab"),
                },
                who: line.option("--as"),
                size: line.option("--size").unwrap_or("1440x1000"),
                theme: line.option("--theme").unwrap_or("light"),
            };
            shot::run(&line.workspace()?, line.name()?, &shot, &Native)
        }
        "logs" => {
            let options = logs::Options {
                errors: line.flag("--errors"),
                since: line.option("--since"),
                follow: line.flag("--follow"),
            };
            logs::run(line.name()?, &options, Machine::THIS, &Native)
        }
        "test" => {
            let (only, file) = (line.option("--only"), line.option("--file"));
            let tests = match (line.flag("--all"), line.option("--owner")) {
                (true, Some(_)) => return Err("Choose --all or --owner.".into()),
                (_, Some(_)) if only.is_some() && file.is_some() => {
                    return Err("Choose --only or --file.".into());
                }
                (_, Some(owner)) => test::Tests::Owner { owner, only, file },
                (_, None) if only.is_some() || file.is_some() => {
                    return Err("--only and --file choose among an --owner's tests.".into());
                }
                (true, None) => test::Tests::All,
                (false, None) => test::Tests::Affected,
            };
            match test::run(
                &line.root()?,
                tests,
                line.flag("--keep"),
                Machine::THIS,
                &Native,
            )? {
                true => Ok(()),
                false => Err("A test failed.".into()),
            }
        }
        "verify" => {
            let feature = line.name()?;
            let passed = verify::run(
                &line.root()?,
                feature,
                line.flag("--removability"),
                Machine::THIS,
                &Native,
                &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
                &mut |text| println!("{text}"),
            )?;
            match passed {
                true => Ok(()),
                false => Err(Quiet.into()),
            }
        }
        "check" => {
            check::here(Machine::THIS, "check")?;
            let root = line.root()?;
            let plan = check::plan(&root, line.flag("--allow-concurrent"), &Native)?;
            if line.flag("--plan") {
                check::print(&plan);
                return Ok(());
            }
            let env = std::env::vars().collect();
            match check::execute(&root, &plan, &env, &Native)? {
                true => {
                    println!("{}", check::next_step(&plan));
                    Ok(())
                }
                false => Err("A check failed.".into()),
            }
        }
        "pr" => {
            let name = line.name()?;
            let ws = line.workspace()?;
            let title = line.option("--title");
            match (line.option("--body"), line.flag("--skeleton"), title) {
                (Some(_), true, _) => Err("Choose --body or --skeleton.".into()),
                (None, true, Some(title)) => pr::start_body(&ws, name, title),
                (None, true, None) => Err("--skeleton needs the --title it starts with.".into()),
                (body, false, title) => {
                    let body = match body {
                        Some(body) => PathBuf::from(body),
                        None => pr::body_file(&Change::existing(&ws, name)?),
                    };
                    pr::run(&ws, name, title, &body, &Native).map(|_| ())
                }
            }
        }
        "review" => {
            let name = line.name()?;
            let ws = line.workspace()?;
            let passes = line
                .option("--passes")
                .map(|passes| {
                    passes
                        .parse::<u32>()
                        .ok()
                        .filter(|passes| *passes > 0)
                        .ok_or("--passes is a number of passes, 1 or more.")
                })
                .transpose()?;
            match (line.flag("--done"), passes) {
                (true, Some(_)) => Err("Choose --passes or --done.".into()),
                (true, None) => review::done(&ws, name, &Native),
                (false, passes) => review::start(&ws, name, passes, &Native),
            }
        }
        "ship" => {
            let named = line.name()?;
            let pause = |seconds| std::thread::sleep(std::time::Duration::from_secs(seconds));
            let github = Retried {
                runner: &Native,
                pause: &pause,
            };
            let name = match named.parse::<u64>() {
                Ok(number) => pr::branch(number, &github)?,
                Err(_) => named.to_owned(),
            };
            ship::change(&line.workspace()?, &name, &github, &pause, &mut |text| {
                println!("{text}")
            })
        }
        "ci" => {
            let (ws, name) = (line.workspace()?, line.name()?);
            let (removability, again) =
                (line.option("--removability"), line.flag("--rerun-failed"));
            require(
                removability.is_none() || !again,
                "Choose --removability or --rerun-failed.",
            )?;
            let pause = |seconds| std::thread::sleep(std::time::Duration::from_secs(seconds));
            let verdict = ci::run(
                &ws,
                name,
                removability,
                again,
                &Retried {
                    runner: &Native,
                    pause: &pause,
                },
                &pause,
                &mut |text| println!("{text}"),
            )?;
            println!("{verdict}");
            Ok(())
        }
        "finish" => finish::run(
            &line.workspace()?,
            line.name()?,
            line.flag("--abandon"),
            &Native,
            &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
        ),
        "status" => {
            let status = status::read(&line.workspace()?, &Native)?;
            if line.flag("--json") {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                status::print(&status);
            }
            Ok(())
        }
        "browser" => {
            let target = line.name()?;
            let output = line.option("--out").ok_or_else(|| line.usage())?;
            let summary = observation::run(
                &line.workspace()?,
                target,
                std::path::Path::new(output),
                &Native,
            )?;
            println!("{}", serde_json::to_string(&summary)?);
            Ok(())
        }
        "build" => {
            let root = line.root()?;
            let env = std::env::vars().collect();
            if line.flag("--cache-key") {
                let key = if line.flag("--release") && build::eligible(&root, &env, true)? {
                    build::key(&root, "release", &env, &Native)?
                } else {
                    String::new()
                };
                println!("key={key}");
                return Ok(());
            }
            let artifact = match (line.flag("--release"), line.flag("--fixture")) {
                (true, true) => return Err("Choose --release or --fixture.".into()),
                (true, false) => &build::RELEASE,
                (false, true) => &build::FIXTURE,
                (false, false) => &build::DEBUG,
            };
            build::build(&root, artifact, &env, &Native)
        }
        other => unreachable!("{other} has a spec but no handler"),
    }
}
fn unknown(name: &str) -> Box<dyn std::error::Error + Send + Sync> {
    format!("Unknown command {name}. Run dispatchdev help.").into()
}
fn main() {
    if let Err(error) = run() {
        let message = error.to_string();
        if !message.is_empty() {
            eprintln!("{message}");
        }
        std::process::exit(
            error
                .downcast_ref::<ship::Exit>()
                .map_or(1, |exit| exit.code),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_command_has_a_section_and_every_section_a_command() {
        let listed: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .collect();
        let mut names: Vec<&str> = COMMANDS.iter().map(|spec| spec.name).collect();
        let mut sorted = listed.clone();
        sorted.sort_unstable();
        names.sort_unstable();
        assert_eq!(sorted, names);
    }
    #[test]
    fn options_take_their_value_after_them_or_after_an_equals_sign() {
        let start = find("start").unwrap();
        for args in [
            vec!["fix", "--from", "origin/x"],
            vec!["fix", "--from=origin/x"],
        ] {
            let line = parse(start, args.into_iter().map(String::from).collect()).unwrap();
            assert_eq!(line.option("--from"), Some("origin/x"));
            assert_eq!(line.name().unwrap(), "fix");
        }
        let refused = |args: &[&str]| {
            parse(start, args.iter().map(|arg| arg.to_string()).collect())
                .err()
                .unwrap()
                .to_string()
        };
        assert!(refused(&["fix", "--keep"]).starts_with("start takes no --keep"));
        assert!(refused(&["fix", "more"]).starts_with("start takes no more"));
        assert_eq!(refused(&["fix", "--from"]), "--from needs a value");
    }
    #[test]
    fn the_help_fits_its_columns() {
        for line in help().lines() {
            assert!(line.len() <= USAGE + 1 + ABOUT, "{line}");
        }
    }
}
