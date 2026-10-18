use dispatchdev_cli::{
    Native, Result, Runner, api, build, check, ci, finish, logs, observation, pr, preview,
    scaffold, ship, start, status, test, workspace::Workspace,
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
        &["start <name> [--from <ref>]"],
        "A worktree and branch for the change from origin/main, its packages installed and its \
         scratch folder made.",
    )
    .options(&["--from"])
    .arguments(1),
    spec(
        "preview",
        &[
            "preview <name> [--page <page>]",
            "        [--restart | --stop]",
        ],
        "Start its preview, or with --restart after a backend edit, and print the link that \
         signs in by itself.",
    )
    .flags(&["--restart", "--stop"])
    .options(&["--page"])
    .arguments(1),
    spec(
        "api",
        &[
            "api <name> <method> <path>",
            "    [--dsp <dsp>] [--as <email>]",
            "    [--data <json>]",
        ],
        "One signed-in call to its preview: as the owner, or --as a demo account; --dsp opens \
         a DSP by its name or id.",
    )
    .options(&["--dsp", "--as", "--data"])
    .arguments(3),
    spec(
        "logs",
        &[
            "logs <name | dev> [--errors]",
            "     [--since <time>] [--follow]",
        ],
        "What its preview, or Dev, logged in its current run, an event a line and without the \
         build's output; --since 10m reads across runs, --errors keeps warnings and failures.",
    )
    .flags(&["--errors", "--follow"])
    .options(&["--since"])
    .arguments(1),
    spec(
        "test",
        &["test [--all] [--keep]"],
        "The tests the diff touches, or --all of them, one line each, stopping at the first \
         failure: for fixing a test, as the merge queue runs them. Each run builds in a folder \
         of its own, deleted when it ends unless --keep.",
    )
    .flags(&["--all", "--keep"]),
    spec(
        "check",
        &["check [--plan] [--keep]", "      [--allow-concurrent]"],
        "Before a push: what stops the branch, the rule checks and clippy, which passes again \
         without running on Rust inputs it passed on; ci and the merge queue run the tests. \
         --plan only names them.",
    )
    .flags(&["--plan", "--keep", "--allow-concurrent"]),
    spec(
        "ci",
        &["ci <name>"],
        "The full checks on GitHub on its committed branch, while its preview is tried: push \
         it, run them without the gate, and follow them to the first failure. ship stops on a \
         head they failed and cancels them if still running.",
    )
    .arguments(1),
    spec(
        "pr",
        &["pr <name> --title <title>", "   --body <file>"],
        "Check the title and body, push the branch, and open the PR or update it.",
    )
    .options(&["--title", "--body"])
    .arguments(1),
    spec(
        "ship",
        &["ship <PR number> [--review]"],
        "Queue the PR as soon as GitHub admits it and wait until it merges; --review first asks \
         CodeRabbit, once.",
    )
    .flags(&["--review"])
    .arguments(1),
    spec(
        "finish",
        &["finish <name> [--abandon]"],
        "After the merge: its preview, port, worktree, build, branches and scratch folder go; \
         then wait for Dev.",
    )
    .flags(&["--abandon"])
    .arguments(1),
    spec(
        "status",
        &["status"],
        "Worktrees, previews, open PRs, what Dev runs, free disk.",
    ),
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
         written, listed where it is read, then what is generated from it: dispatchdev new \
         lists the kinds, new <kind> a kind's options. --dry-run prints what it would write, \
         from any checkout.",
    )
    .passed(),
    spec(
        "add",
        &["add <piece> --to <feature>", "    [--dry-run] [--json]"],
        "A piece new feature writes, added to a feature that exists: page, tab-of <feature>, \
         settings, api, people <source> or contract <name>. Its slot entries, fields and \
         permissions join the feature's; what it can't merge it names. dispatchdev add lists \
         the pieces.",
    )
    .passed(),
    spec(
        "generate",
        &["generate [--json]"],
        "Write everything generated from what the owners declare: the app's and the MCP's \
         lists, the API types and the snapshots the app's tests hold, from one incremental \
         build in the worktree's own target folder. It names each file it changed; after a \
         failure, fix it and run it again.",
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
            "start", "preview", "api", "logs", "test", "check", "ci", "pr", "ship", "finish",
            "status", "browser",
        ],
    ),
    (
        "Making things, and reading what the product has:",
        &["new", "add", "generate", "list", "explain", "doctor"],
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
        "test, check, build, new, add, generate, list, explain and doctor work on the checkout\n\
         the current directory is in, or on --root <directory>. dispatchdev <command> --help shows one."
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
/// The checkout to work on: `root`, or the one the current directory is in.
fn checkout(root: Option<&str>) -> Result<PathBuf> {
    if let Some(root) = root {
        return Ok(PathBuf::from(root));
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
            &Native,
        ),
        "preview" => {
            let action = match (line.flag("--restart"), line.flag("--stop")) {
                (true, true) => return Err("Choose --restart or --stop.".into()),
                (true, false) => preview::Action::Restart,
                (false, true) => preview::Action::Stop,
                (false, false) => preview::Action::Start,
            };
            preview::run(
                &line.workspace()?,
                line.name()?,
                action,
                line.option("--page"),
                &Native,
            )
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
        "logs" => {
            let options = logs::Options {
                errors: line.flag("--errors"),
                since: line.option("--since"),
                follow: line.flag("--follow"),
            };
            logs::run(line.name()?, &options, &Native)
        }
        "test" => {
            let passed = test::run(
                &line.root()?,
                line.flag("--all"),
                line.flag("--keep"),
                &Native,
            )?;
            match passed {
                true => Ok(()),
                false => Err("A test failed.".into()),
            }
        }
        "check" => {
            let root = line.root()?;
            let plan = check::plan(&root, line.flag("--allow-concurrent"), &Native)?;
            if line.flag("--plan") {
                check::print(&plan);
                return Ok(());
            }
            let env = std::env::vars().collect();
            match check::execute(&root, &plan, line.flag("--keep"), &env, &Native)? {
                true => Ok(()),
                false => Err("A check failed.".into()),
            }
        }
        "pr" => {
            let name = line.name()?;
            let (Some(title), Some(body)) = (line.option("--title"), line.option("--body")) else {
                return Err(line.usage());
            };
            pr::run(
                &line.workspace()?,
                name,
                title,
                std::path::Path::new(body),
                &Native,
            )
        }
        "ship" => {
            let number = match line.arguments.as_slice() {
                [number] => number.parse::<u64>().ok(),
                _ => None,
            }
            .ok_or_else(|| line.usage())?;
            if line.flag("--review") {
                let asked = ship::request_review(number, &Native)?;
                println!(
                    "{}",
                    if asked {
                        "Asked CodeRabbit for its review; shipping waits for it."
                    } else {
                        "CodeRabbit was asked already; shipping waits for its review."
                    }
                );
            }
            let merge = ship::run(
                number,
                &Native,
                &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
                &mut |text| println!("{text}"),
            )?;
            println!("#{number} merged as {merge}");
            Ok(())
        }
        "ci" => {
            let verdict = ci::run(
                &line.workspace()?,
                line.name()?,
                &Native,
                &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
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
        ),
        "status" => status::run(&line.workspace()?, &Native),
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
        std::process::exit(1);
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
