use dispatchdev_cli::{Native, Result, Runner, build, check, ship};
use std::path::PathBuf;

const HELP: &str = "\
Usage: dispatchdev <command>

  build [--release] [--cache-key]  Build the backend, reusing a build of identical inputs.
  check [--allow-concurrent]       Check the branch is ready to push, and name the tests its
                                   diff touches.
  ship <PR number>                 Queue a PR as soon as GitHub admits it and wait until it
                                   merges.
  help                             Show this.

Each command works on the checkout the current directory is in, or on --root <directory>.";

/// The command line: the command, its flags, `--root` and any other arguments, in order.
struct Line {
    command: String,
    flags: Vec<String>,
    root: Option<PathBuf>,
    arguments: Vec<String>,
}
fn parse(args: Vec<String>) -> Result<Line> {
    let mut args = args.into_iter();
    let command = args.next().ok_or(HELP)?;
    let (mut flags, mut root, mut arguments) = (vec![], None, vec![]);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                root = Some(PathBuf::from(
                    args.next().ok_or("--root needs a directory")?,
                ))
            }
            flag if flag.starts_with("--") => flags.push(arg),
            _ => arguments.push(arg),
        }
    }
    Ok(Line {
        command,
        flags,
        root,
        arguments,
    })
}
/// Refuses any flag the command doesn't take, and any argument but those `arguments` allows.
fn only(line: &Line, allowed: &[&str], arguments: usize) -> Result<()> {
    let flag = line
        .flags
        .iter()
        .find(|flag| !allowed.contains(&flag.as_str()));
    match (flag, line.arguments.get(arguments)) {
        (Some(extra), _) | (None, Some(extra)) => {
            Err(format!("{} takes no {extra}. Run dispatchdev help.", line.command).into())
        }
        (None, None) => Ok(()),
    }
}
/// The checkout to work on: `--root`, or the one the current directory is in.
fn root(line: &Line) -> Result<PathBuf> {
    if let Some(root) = &line.root {
        return Ok(root.clone());
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
fn run() -> Result<()> {
    let line = parse(std::env::args().skip(1).collect())?;
    let flag = |name: &str| line.flags.iter().any(|flag| flag == name);
    match line.command.as_str() {
        "build" => {
            only(&line, &["--release", "--cache-key"], 0)?;
            let root = root(&line)?;
            let env = std::env::vars().collect();
            if flag("--cache-key") {
                let key = if flag("--release") && build::eligible(&root, &env, true)? {
                    build::key(&root, "release", &env, &Native)?
                } else {
                    String::new()
                };
                println!("key={key}");
                return Ok(());
            }
            build::build(&root, flag("--release"), &env, &Native)
        }
        "check" => {
            only(&line, &["--allow-concurrent"], 0)?;
            check::run(&root(&line)?, flag("--allow-concurrent"), &Native)
        }
        "ship" => {
            only(&line, &[], 1)?;
            let number = match line.arguments.as_slice() {
                [number] => number.parse::<u64>().ok(),
                _ => None,
            }
            .ok_or("Usage: dispatchdev ship <PR number>")?;
            let merge = ship::run(
                number,
                &Native,
                &|seconds| std::thread::sleep(std::time::Duration::from_secs(seconds)),
                &mut |text| println!("{text}"),
            )?;
            println!("#{number} merged as {merge}");
            Ok(())
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
