use dispatchdev_cli::{
    Native, Result, Runner, api, build, check, finish, logs, pr, preview, prove, ship, start,
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
                                   each, stopping at the first failure. Each run builds in a
                                   folder of its own, deleted when it ends unless --keep, so
                                   several worktrees can test at once.
  prove [<test file>...]           That the tests fail without the change and pass with it:
        [--grep <pattern>]         they run in a throwaway checkout of where the branch
        [--repeat <n>]             started, with the branch's tests unless it changes only
        [--cpu <rate>]             tests, then in the branch. --repeat runs each n times;
        [--late-frames <ms>]       --cpu and --late-frames run browser tests as on a busy
                                   runner. Without test files, the ones the branch changes.
  check [--plan] [--keep]          Before a push: what stops the branch, the rule checks,
        [--allow-concurrent]       clippy and the tests, as test runs them; --plan only names
                                   them.
  pr <name> --title <title>        Check the title and body, push the branch, and open the
     --body <file>                 PR or update it.
  ship <PR number> [--review]      Queue the PR as soon as GitHub admits it and wait until it
                                   merges; --review first asks CodeRabbit, once.
  finish <name> [--abandon]        After the merge: its preview, port, worktree, build,
                                   branches and scratch folder go; then wait for Dev.
  status                           Worktrees, previews, open PRs, what Dev runs, free disk.

  build [--release] [--cache-key]  Build the backend, reusing a build of identical inputs.
  help                             Show this.

test, prove, check and build work on the checkout the current directory is in, or on
--root <directory>.";

/// Options that take a value.
const VALUED: &[&str] = &[
    "--root",
    "--from",
    "--page",
    "--dsp",
    "--as",
    "--data",
    "--title",
    "--body",
    "--since",
    "--grep",
    "--repeat",
    "--cpu",
    "--late-frames",
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
        if let Some(root) = self.option("--root") {
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
    fn workspace(&self) -> Result<Workspace> {
        Workspace::find(&std::env::current_dir()?, &Native)
    }
}
fn run() -> Result<()> {
    let line = parse(std::env::args().skip(1).collect())?;
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
        "prove" => {
            line.only(
                &["--grep", "--repeat", "--cpu", "--late-frames"],
                usize::MAX,
            )?;
            let repeat = match line.option("--repeat") {
                None => 1,
                Some(text) => text
                    .parse::<u32>()
                    .ok()
                    .filter(|runs| *runs > 0)
                    .ok_or("--repeat takes a number of runs, 1 or more.")?,
            };
            let cpu = line
                .option("--cpu")
                .map(|text| {
                    text.parse::<f64>()
                        .ok()
                        .filter(|rate| *rate >= 1.0)
                        .ok_or("--cpu takes how many times slower, 1 or more, such as 4.")
                })
                .transpose()?;
            let late_frames = line
                .option("--late-frames")
                .map(|text| {
                    text.parse::<u32>()
                        .ok()
                        .filter(|ms| *ms > 0)
                        .ok_or("--late-frames takes milliseconds, such as 100.")
                })
                .transpose()?;
            let options = prove::Options {
                tests: &line.arguments,
                grep: line.option("--grep"),
                repeat,
                cpu,
                late_frames,
            };
            prove::run(&line.root()?, &options, &Native)
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
            line.only(&["--release", "--cache-key"], 0)?;
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
            build::build(&root, line.flag("--release"), &env, &Native)
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
