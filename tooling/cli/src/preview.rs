//! `dispatchdev preview <name>`: the change's preview, a user service serving its worktree on
//! this machine's Tailscale address with demo data. Each change keeps one port, so its link
//! survives restarts. Previews, and the logs they and Dev write, are dispatch-dev's alone.
use crate::{
    Result, Runner,
    change::Change,
    machine::Machine,
    require, slot,
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
/// Refuses on a machine that runs no previews: `preview`, `shot`, `api`, `logs` and
/// `start --preview` ask it before anything else.
pub fn here(machine: Machine) -> Result<()> {
    require(
        machine.previews(),
        "Previews and logs run on dispatch-dev alone.",
    )
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
/// Where a change's preview keeps its port.
pub fn env_file(name: &str) -> Result<PathBuf> {
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
/// The changes whose previews are running: none on a machine that runs no previews.
pub fn running(machine: Machine, runner: &dyn Runner) -> Result<Vec<String>> {
    if !machine.previews() {
        return Ok(vec![]);
    }
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

/// Where the link opens: a page, in a DSP or on the platform owner's own pages, on a tab of the
/// page.
pub struct Open<'a> {
    pub page: Option<&'a str>,
    /// The DSP, by its id or a unique part of its name: the fixture's own unless named.
    pub dsp: Option<&'a str>,
    /// The platform owner's own pages, outside any DSP, in place of a DSP's.
    pub platform: bool,
    pub tab: Option<&'a str>,
}
impl Open<'_> {
    /// Refuses what can't be opened together.
    pub fn check(&self) -> Result<()> {
        require(
            self.tab.is_none() || self.page.is_some(),
            "--tab is a tab of a page: name the page with --page too.",
        )?;
        require(
            self.dsp.is_none() || !self.platform,
            "Choose --dsp or --platform.",
        )
    }
    /// The id of the DSP it opens, of those a session lists: the one it names, else the first,
    /// which is the fixture's own, as the owner's list puts it first.
    pub fn dsp_id(&self, dsps: &[serde_json::Value]) -> Result<String> {
        let dsp = match self.dsp {
            Some(wanted) => crate::api::dsp(dsps, wanted)?,
            None => dsps.first().ok_or("The preview has no DSP to open.")?,
        };
        Ok(dsp["id"].as_str().ok_or("A DSP without an id")?.to_owned())
    }
}
/// The link's fragment, as the app reads its address: `#<page>`, or a DSP's page
/// `#dsp/<id>/<page>`, with `?tab=<tab>` for a tab of it.
pub fn fragment(page: Option<&str>, dsp: Option<&str>, tab: Option<&str>) -> String {
    let page = page.map(|page| page.trim_start_matches('#'));
    let mut fragment = match (dsp, page) {
        (Some(id), page) => format!("#dsp/{}/{}", encoded(id), page.unwrap_or("")),
        (None, Some(page)) => format!("#{page}"),
        (None, None) => String::new(),
    };
    if let Some(tab) = tab {
        fragment.push_str(&format!("?tab={}", encoded(tab)));
    }
    fragment
}
/// A value as an address holds it, each byte but the unreserved ones escaped.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Starts the change's preview, or with `restart` starts it afresh, on the port it keeps or the
/// first free one, without waiting for its build; returns the port.
pub fn begin(ws: &Workspace, name: &str, restart: bool, runner: &dyn Runner) -> Result<u16> {
    let active = state(name, runner)? == "active";
    let port = match port(name)? {
        Some(port) => port,
        None => {
            let others = running(ws.machine, runner)?;
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
    let verb = match (restart, active) {
        (true, _) => "restart",
        (false, false) => "start",
        (false, true) => return Ok(port),
    };
    runner.command(&["systemctl", "--user", verb, &unit(name)], None, 120)?;
    Ok(port)
}

pub fn run(
    ws: &Workspace,
    name: &str,
    action: Action,
    open: &Open<'_>,
    runner: &dyn Runner,
) -> Result<()> {
    here(ws.machine)?;
    Change::existing(ws, name)?;
    open.check()?;
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
    let port = begin(ws, name, matches!(action, Action::Restart), runner)?;
    let started = Instant::now();
    let mut waiting = false;
    let link = loop {
        let log = journal(name, runner)?;
        if let Some(link) = link(&log) {
            break link;
        }
        // Its build waits its turn for a build slot as any other does, and says so once.
        if !waiting && let Some(line) = log.lines().find(|line| line.starts_with(slot::WAITING)) {
            println!("{line}");
            waiting = true;
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
    // The DSP by its id, from the DSPs the owner the link signs in sees.
    let dsp = if open.platform {
        None
    } else {
        Some(open.dsp_id(&crate::api::dsps(ws, name, runner)?)?)
    };
    let fragment = fragment(open.page, dsp.as_deref(), open.tab);
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
    #[test]
    fn a_link_opens_a_page_in_a_dsp_on_its_tab_as_the_app_reads_its_address() {
        assert_eq!(fragment(Some("dvic"), None, None), "#dvic");
        assert_eq!(
            fragment(Some("settings"), Some("dsp_2"), Some("uniform data")),
            "#dsp/dsp_2/settings?tab=uniform%20data"
        );
        assert_eq!(fragment(None, Some("dsp_2"), None), "#dsp/dsp_2/");
        assert_eq!(fragment(None, None, None), "");
    }
    #[test]
    fn previews_and_logs_refuse_at_once_on_a_mac() {
        /// Any command fails the test: nothing runs before the refusal.
        struct Nothing;
        impl Runner for Nothing {
            fn command(
                &self,
                args: &[&str],
                _: Option<&std::path::Path>,
                _: u64,
            ) -> Result<Vec<u8>> {
                panic!("ran {args:?}")
            }
        }
        let ws = Workspace {
            root: PathBuf::from("/workspace"),
            machine: Machine::Mac,
        };
        let open = || Open {
            page: None,
            dsp: None,
            platform: false,
            tab: None,
        };
        let shot = crate::shot::Shot {
            open: open(),
            who: None,
            size: "1440x1000",
            theme: "light",
        };
        let call = crate::api::Call {
            method: "GET",
            path: "/api/session",
            dsp: None,
            who: None,
            data: None,
        };
        let logs = crate::logs::Options {
            errors: false,
            since: None,
            follow: false,
        };
        for refused in [
            run(&ws, "fix-week", Action::Start, &open(), &Nothing),
            crate::shot::run(&ws, "fix-week", &shot, &Nothing),
            crate::api::run(&ws, "fix-week", call, &Nothing).map(|_| ()),
            crate::logs::run("dev", &logs, Machine::Mac, &Nothing),
            crate::start::run(&ws, "fix-week", "origin/main", true, &Nothing),
        ] {
            assert_eq!(
                refused.unwrap_err().to_string(),
                "Previews and logs run on dispatch-dev alone."
            );
        }
    }
}
