//! `dispatchdev preview <name>`: the change's preview, a user service serving its worktree on
//! this machine's Tailscale address with demo data. Each change keeps one port, so its link
//! survives restarts.
use crate::{
    Result, Runner, require,
    workspace::{self, Workspace},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

const PORTS: std::ops::RangeInclusive<u16> = 4100..=4199;
/// Previews are the first thing the kernel kills when memory runs short.
const MOST: usize = 3;
/// A fresh worktree's first build takes this long at most.
const READY: Duration = Duration::from_secs(30 * 60);

pub enum Action {
    Start,
    Restart,
    Stop,
}
pub fn unit(name: &str) -> String {
    format!("dispatch-preview@{name}")
}
fn config() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("HOME")?).join(".config/dispatch-preview"))
}
/// The `KEY=value` lines of an environment file.
pub fn read_env(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.trim().split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}
fn env_file(name: &str) -> Result<PathBuf> {
    Ok(config()?.join(format!("{name}.env")))
}
/// The port a change's preview keeps, if it has one.
pub fn port(name: &str) -> Result<Option<u16>> {
    Ok(fs::read_to_string(env_file(name)?)
        .ok()
        .and_then(|text| read_env(&text).get("DISPATCH_DEV_PORT")?.parse().ok()))
}
/// This machine's Tailscale address, which every preview binds to.
pub fn host() -> Result<String> {
    let path = config()?.join("host.env");
    let text = fs::read_to_string(&path).map_err(|_| {
        format!(
            "{} is missing: create it, mode 600, with DISPATCH_DEV_HOST=<this machine's Tailscale address>.",
            path.display()
        )
    })?;
    read_env(&text)
        .remove("DISPATCH_DEV_HOST")
        .ok_or_else(|| format!("{} names no DISPATCH_DEV_HOST.", path.display()).into())
}
/// The first port in the range no change keeps and nothing listens on.
pub fn pick(kept: &BTreeSet<u16>, free: impl Fn(u16) -> bool) -> Option<u16> {
    PORTS
        .into_iter()
        .find(|port| !kept.contains(port) && free(*port))
}
fn kept_ports() -> Result<BTreeSet<u16>> {
    let Ok(entries) = fs::read_dir(config()?) else {
        return Ok(BTreeSet::new());
    };
    Ok(entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name() != "host.env")
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| read_env(&text).get("DISPATCH_DEV_PORT")?.parse().ok())
        .collect())
}
/// The changes whose previews are running.
pub fn running(runner: &dyn Runner) -> Result<Vec<String>> {
    let units = workspace::text(
        runner,
        &[
            "systemctl",
            "--user",
            "list-units",
            "dispatch-preview@*",
            "--state=active",
            "--plain",
            "--no-legend",
        ],
        None,
    )?;
    Ok(units
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|unit| {
            unit.strip_prefix("dispatch-preview@")?
                .strip_suffix(".service")
                .map(str::to_owned)
        })
        .collect())
}
/// What the preview's current run has logged; nothing when it isn't running.
pub fn journal(name: &str, runner: &dyn Runner) -> Result<String> {
    let id = workspace::text(
        runner,
        &[
            "systemctl",
            "--user",
            "show",
            "-p",
            "InvocationID",
            "--value",
            &unit(name),
        ],
        None,
    )?;
    if id.is_empty() {
        return Ok(String::new());
    }
    workspace::text(
        runner,
        &[
            "journalctl",
            "--user",
            &format!("_SYSTEMD_INVOCATION_ID={id}"),
            "-o",
            "cat",
            "--no-pager",
        ],
        None,
    )
}
/// The link that signs a browser in as the owner, from a run's log.
pub fn link(journal: &str) -> Option<String> {
    journal
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("Development fixtures: "))
        .map(|link| link.trim().to_owned())
}
/// The demo accounts' password, from a run's log: `Manual sign-in: <origin> (<email> / <password>)`.
pub fn password(journal: &str) -> Option<String> {
    let line = journal
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("Manual sign-in: "))?;
    let inside = line.split_once('(')?.1.strip_suffix(')')?;
    Some(inside.split_once(" / ")?.1.to_owned())
}
fn state(name: &str, runner: &dyn Runner) -> Result<String> {
    // `is-active` exits non-zero for anything but active, so its answer is read either way.
    Ok(runner
        .command(&["systemctl", "--user", "is-active", &unit(name)], None, 30)
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
        .unwrap_or_else(|error| {
            let text = error.to_string();
            ["failed", "activating", "deactivating"]
                .into_iter()
                .find(|state| text.contains(state))
                .unwrap_or("inactive")
                .to_owned()
        }))
}

pub fn run(
    ws: &Workspace,
    name: &str,
    action: Action,
    page: Option<&str>,
    runner: &dyn Runner,
) -> Result<()> {
    workspace::valid_name(name)?;
    require(
        ws.worktree(name).is_dir(),
        &format!("worktrees/{name} doesn't exist. Start it with dispatchdev start {name}."),
    )?;
    let systemctl = |verb: &str| {
        runner
            .command(&["systemctl", "--user", verb, &unit(name)], None, 120)
            .map(|_| ())
    };
    if let Action::Stop = action {
        systemctl("stop")?;
        println!(
            "Stopped {name}'s preview; it keeps port {}.",
            port(name)?.unwrap_or(0)
        );
        return Ok(());
    }
    let host = host()?;
    let active = state(name, runner)? == "active";
    let port = match port(name)? {
        Some(port) => port,
        None => {
            let others = running(runner)?;
            require(
                others.len() < MOST,
                &format!(
                    "{MOST} previews are running already ({}); stop one with dispatchdev preview <name> --stop.",
                    others.join(", ")
                ),
            )?;
            let port = pick(&kept_ports()?, |port| {
                std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
            })
            .ok_or("No free port in 4100–4199.")?;
            workspace::write_private(&env_file(name)?, &format!("DISPATCH_DEV_PORT={port}\n"))?;
            port
        }
    };
    match (action, active) {
        (Action::Restart, _) => systemctl("restart")?,
        (Action::Start, false) => systemctl("start")?,
        _ => {}
    }
    let started = Instant::now();
    let link = loop {
        let log = journal(name, runner)?;
        if let Some(link) = link(&log) {
            break link;
        }
        let now = state(name, runner)?;
        if !matches!(now.as_str(), "active" | "activating") || started.elapsed() > READY {
            let tail: Vec<_> = log.lines().rev().take(15).collect();
            return Err(format!(
                "{name}'s preview is {now} and printed no link. Its last lines:\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            )
            .into());
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let fragment = page.map_or(String::new(), |page| {
        format!("#{}", page.trim_start_matches('#'))
    });
    println!("{name}'s preview, on {host}:{port}, signs in by itself:");
    println!("{link}{fragment}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_change_keeps_its_port_and_a_new_one_takes_the_first_free() {
        let kept: BTreeSet<u16> = [4100, 4101].into();
        assert_eq!(pick(&kept, |_| true), Some(4102));
        assert_eq!(pick(&kept, |port| port != 4102), Some(4103));
        assert_eq!(pick(&BTreeSet::new(), |_| false), None);
        assert_eq!(
            read_env("DISPATCH_DEV_PORT=4105\n# note\n")["DISPATCH_DEV_PORT"],
            "4105"
        );
    }
    #[test]
    fn the_runs_log_gives_its_link_and_the_demo_password() {
        let log = "Worktree: audit\nDevelopment fixtures: http://preview.test:4101/__preview/old\n\
            Development fixtures: http://preview.test:4101/__preview/new\n\
            Manual sign-in: http://preview.test:4101 (owner@dispatch.test / Demo-pass!)\n";
        assert_eq!(
            link(log).as_deref(),
            Some("http://preview.test:4101/__preview/new")
        );
        assert_eq!(password(log).as_deref(), Some("Demo-pass!"));
        assert_eq!(link("starting"), None);
        assert_eq!(password("Manual sign-in: http://x (no password)"), None);
    }
}
