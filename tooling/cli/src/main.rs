use dispatchdev_cli::{
    Native, Result, Runner, api, build, check, finish, logs, pr, preview, scaffold, ship, start,
    status, test, workspace::Workspace,
};
use std::{collections::BTreeMap, path::PathBuf};

const HELP: &str = "\
Usage: dispatchdev <command>

A change, from start to finish:
  start <name> [--from <ref>]      A worktree and branch for the change from origin/main, its
                                   packages installed and its scratch folder made.
  preview <name> [--page <page>]   Start its preview, or with --restart after a backend edit,
          [--restart | --stop]     and print the link that signs in by itself.
  api <name> <method> <path>       One signed-in call to its preview: as the owner, or
      [--dsp <dsp>] [--as <email>] --as a demo account; --dsp opens a DSP by its name or id.
      [--data <json>]
  logs <name | dev> [--errors]     What its preview, or Dev, logged in its current run, an
       [--since <time>] [--follow] event a line and without the build's output; --since 10m
                                   reads across runs, --errors keeps warnings and failures.
  test [--all] [--keep]            The tests the diff touches, or --all of them, one line
                                   each, stopping at the first failure: for fixing a test, as
                                   the merge queue runs them. Each run builds in a folder of
                                   its own, deleted when it ends unless --keep.
  check [--plan] [--keep]          Before a push: what stops the branch, the rule checks and
        [--allow-concurrent]       clippy; the merge queue runs the tests. --plan only names
                                   them.
  pr <name> --title <title>        Check the title and body, push the branch, and open the
     --body <file>                 PR or update it.
  ship <PR number> [--review]      Queue the PR as soon as GitHub admits it and wait until it
                                   merges; --review first asks CodeRabbit, once.
  finish <name> [--abandon]        After the merge: its preview, port, worktree, build,
                                   branches and scratch folder go; then wait for Dev.
  status                           Worktrees, previews, open PRs, what Dev runs, free disk.

  new feature <name> [options]     In a change's worktree, a feature or collector that works as
  new collector <site> [options]   written, listed in app/, with its catalog, API types and
                                   snapshots written. With no name it lists its options;
                                   --dry-run prints what it would write.
  new tool <name> [options]        A tool agents can use, in mcp/tools/: its file and its
                                   test, listed in its TOOLS. It can need features, be about
                                   one DSP, several or the connection, and take actions, some
                                   that change something. Who may use it is chosen per key and
                                   app on the Agents page: Off, Read, or Read and change.

  build [--release | --fixture]    Build the backend, or the browser tests' assessment
        [--cache-key]              fixture, reusing a build of identical inputs.
  help                             Show this.

test, check, build and new work on the checkout the current directory is in, or on --root
<directory>.";

/// Options that take a value.
const VALUED: &[&str] = &[
    "--root", "--from", "--page", "--dsp", "--as", "--data", "--title", "--body", "--since",
];

/// The command line: the command, its flags and options, and its other arguments, in order.
struct Line {
    command: String,
    flags: Vec<String>,
    options: BTreeMap<String, String>,
    arguments: Vec<String>,
}
fn parse(args: Vec<String>) -> Result<Line> {
    let mut args = args.into_iter();
    let command = args.next().ok_or(HELP)?;
    let (mut flags, mut options, mut arguments) = (vec![], BTreeMap::new(), vec![]);
    while let Some(arg) = args.next() {
        if VALUED.contains(&arg.as_str()) {
            let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
            options.insert(arg, value);
        } else if arg.starts_with("--") {
            flags.push(arg);
        } else {
            arguments.push(arg);
        }
    }
    Ok(Line {
        command,
        flags,
        options,
        arguments,
    })
}
impl Line {
    /// Refuses any flag or option the command doesn't take, and arguments beyond `count`.
    fn only(&self, allowed: &[&str], count: usize) -> Result<()> {
        let extra = self
            .flags
            .iter()
            .chain(self.options.keys())
            .find(|name| !allowed.contains(&name.as_str()) && name.as_str() != "--root")
            .or(self.arguments.get(count));
        match extra {
            Some(extra) => {
                Err(format!("{} takes no {extra}. Run dispatchdev help.", self.command).into())
            }
            None => Ok(()),
        }
    }
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }
    fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
    /// The change a command names.
    fn name(&self, usage: &str) -> Result<&str> {
        self.arguments
            .first()
            .map(String::as_str)
            .ok_or_else(|| format!("Usage: dispatchdev {usage}").into())
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
/// `new <kind> [options]`: every option but `--root` is the scaffolder's, which passes to it as
/// given, so its own list is the only one.
fn new(args: &[String]) -> Result<()> {
    let (kind, rest) = args.split_first().ok_or(scaffold::USAGE)?;
    let (mut root, mut passed) = (None, vec![]);
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        if arg == "--root" {
            root = Some(rest.next().ok_or("--root needs a value")?.as_str());
        } else {
            passed.push(arg.clone());
        }
    }
    let root = checkout(root)?;
    match scaffold::run(&root, kind, &passed)? {
        true => Ok(()),
        false => Err("It wasn't made, or a step after it failed.".into()),
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|command| command == "new") {
        return new(&args[1..]);
    }
    let line = parse(args)?;
    match line.command.as_str() {
        "start" => {
            line.only(&["--from"], 1)?;
            let name = line.name("start <name> [--from <ref>]")?;
            start::run(
                &line.workspace()?,
                name,
                line.option("--from").unwrap_or("origin/main"),
                &Native,
            )
        }
        "preview" => {
            line.only(&["--page", "--restart", "--stop"], 1)?;
            let name = line.name("preview <name> [--page <page>] [--restart | --stop]")?;
            let action = match (line.flag("--restart"), line.flag("--stop")) {
                (true, true) => return Err("Choose --restart or --stop.".into()),
                (true, false) => preview::Action::Restart,
                (false, true) => preview::Action::Stop,
                (false, false) => preview::Action::Start,
            };
            preview::run(
                &line.workspace()?,
                name,
                action,
                line.option("--page"),
                &Native,
            )
        }
        "api" => {
            line.only(&["--dsp", "--as", "--data"], 3)?;
            let usage = "api <name> <method> <path> [--dsp <dsp>] [--as <email>] [--data <json>]";
            let [name, method, path] = line.arguments.as_slice() else {
                return Err(format!("Usage: dispatchdev {usage}").into());
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
            line.only(&["--errors", "--since", "--follow"], 1)?;
            let name = line.name("logs <name | dev> [--errors] [--since <time>] [--follow]")?;
            let options = logs::Options {
                errors: line.flag("--errors"),
                since: line.option("--since"),
                follow: line.flag("--follow"),
            };
            logs::run(name, &options, &Native)
        }
        "test" => {
            line.only(&["--all", "--keep"], 0)?;
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
            line.only(&["--plan", "--keep", "--allow-concurrent"], 0)?;
            let root = line.root()?;
            let plan = check::plan(&root, line.flag("--allow-concurrent"), &Native)?;
            if line.flag("--plan") {
                check::print(&plan);
                return Ok(());
            }
            match check::execute(&root, &plan, line.flag("--keep"))? {
                true => Ok(()),
                false => Err("A check failed.".into()),
            }
        }
        "pr" => {
            line.only(&["--title", "--body"], 1)?;
            let usage = "pr <name> --title <title> --body <file>";
            let name = line.name(usage)?;
            let (Some(title), Some(body)) = (line.option("--title"), line.option("--body")) else {
                return Err(format!("Usage: dispatchdev {usage}").into());
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
            line.only(&["--review"], 1)?;
            let number = match line.arguments.as_slice() {
                [number] => number.parse::<u64>().ok(),
                _ => None,
            }
            .ok_or("Usage: dispatchdev ship <PR number> [--review]")?;
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
        "finish" => {
            line.only(&["--abandon"], 1)?;
            let name = line.name("finish <name> [--abandon]")?;
            finish::run(&line.workspace()?, name, line.flag("--abandon"), &Native)
        }
        "status" => {
            line.only(&[], 0)?;
            status::run(&line.workspace()?, &Native)
        }
        "build" => {
            line.only(&["--release", "--fixture", "--cache-key"], 0)?;
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
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        other => Err(format!("Unknown command {other}. Run dispatchdev help.").into()),
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
