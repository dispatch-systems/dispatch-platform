//! Dev, as a workspace reaches it: through Dev's host manager where the workspace holds Dev's
//! runtime, as on dispatch-dev, or else by Dev's public health, at the address Dev's
//! configuration in the workspace names, as on a Mac. `finish` waits here for Dev to run a
//! merge.
use crate::{
    REPOSITORY, Result, Runner,
    workspace::{self, Clock, Workspace},
};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

/// Where a workspace without Dev's host manager, as on a Mac, finds Dev's public address, whose
/// `/api/health` names the commit Dev runs: `ORIGIN` in Dev's own configuration, this file of
/// its checkout, as `dispatchdev browser dev` reads it. The address names the owner, so it stays
/// out of the source.
const CONFIG: &str = "config/platform.env";
const ORIGIN: &str = "DISPATCH_ORIGIN";
/// How long Dev may take to install a merge before the wait gives up.
const INSTALL: u64 = 16 * 60;
/// Seconds between looks at Dev's health.
const PAUSE: u64 = 10;

/// Dev's host manager, where the workspace holds Dev's runtime, as on dispatch-dev.
fn host_manager(ws: &Workspace) -> Option<PathBuf> {
    Some(ws.dev().join(".runtime/management/dispatch-host")).filter(|host| host.is_file())
}

/// The address of Dev's public health, at the origin Dev's configuration in the workspace names.
fn health(ws: &Workspace) -> Result<String> {
    let config = ws.dev().join(CONFIG);
    std::fs::read_to_string(&config)
        .ok()
        .and_then(|text| workspace::read_env(&text).remove(ORIGIN))
        .map(|origin| origin.trim_matches(['"', '\'']).to_owned())
        .filter(|origin| !origin.is_empty())
        .map(|origin| format!("{origin}/api/health"))
        .ok_or_else(|| format!("{} has no {ORIGIN}.", config.display()).into())
}

/// Waits until Dev runs `commit`, or one containing it, and returns the commit it runs, short.
/// Where the workspace holds Dev, Dev's host manager waits; elsewhere, as on a Mac, Dev's public
/// health is looked at every `PAUSE` seconds, on `clock`.
pub fn live(ws: &Workspace, commit: &str, runner: &dyn Runner, clock: &Clock) -> Result<String> {
    let runs = if let Some(host) = host_manager(ws) {
        let dev = ws.dev();
        let live = runner.command(
            &[
                host.to_str().ok_or("Non-UTF8 path")?,
                "host",
                "dev",
                "--root",
                dev.to_str().ok_or("Non-UTF8 path")?,
                "--wait",
                commit,
            ],
            None,
            INSTALL,
        )?;
        let status: Value = serde_json::from_slice(&live).unwrap_or(Value::Null);
        status["commit"].as_str().unwrap_or(commit).to_owned()
    } else {
        public(&health(ws)?, commit, runner, clock)?
    };
    Ok(runs.get(..8).unwrap_or(&runs).to_owned())
}

/// Looks at Dev's public health, at `health`, until Dev runs `commit`, or a later commit of
/// `main` containing it, as the host manager waits, for `INSTALL` seconds on `clock`, each
/// request and pause bounded by the time left; returns the commit it runs. While Dev restarts on
/// a new commit it answers nothing, and the next look asks again.
fn public(health: &str, commit: &str, runner: &dyn Runner, clock: &Clock) -> Result<String> {
    let deadline = (clock.now)() + Duration::from_secs(INSTALL);
    let left = || deadline.saturating_duration_since((clock.now)()).as_secs();
    let mut last = None;
    while left() > 0 {
        if let Some(runs) = look(health, runner, left()) {
            if runs == commit || contains(runner, &runs, commit, left()) {
                return Ok(runs);
            }
            last = Some(runs);
        }
        (clock.pause)(PAUSE.min(left()));
    }
    let short = |commit: &str| commit.get(..8).unwrap_or(commit).to_owned();
    Err(match last {
        Some(runs) => format!(
            "Dev didn't run {} within {} minutes; it runs {}.",
            short(commit),
            INSTALL / 60,
            short(&runs)
        ),
        None => format!("{health} didn't answer within {} minutes.", INSTALL / 60),
    }
    .into())
}

/// The commit Dev's health, at `health`, names, asked for `seconds` at most; none while Dev
/// doesn't answer.
fn look(health: &str, runner: &dyn Runner, seconds: u64) -> Option<String> {
    let bound = seconds.to_string();
    let answer = runner
        .command(
            &["curl", "-sSf", "--max-time", &bound, health],
            None,
            seconds,
        )
        .ok()?;
    let health: Value = serde_json::from_slice(&answer).ok()?;
    Some(health["source"]["commit"].as_str()?.to_owned())
}

/// Whether `later` contains `commit`, as GitHub compares them, asked for `seconds` at most; no
/// answer counts as no.
fn contains(runner: &dyn Runner, later: &str, commit: &str, seconds: u64) -> bool {
    let compare = format!("repos/{REPOSITORY}/compare/{commit}...{later}");
    seconds > 0
        && runner
            .command(&["gh", "api", &compare], None, seconds)
            .ok()
            .and_then(|answer| serde_json::from_slice::<Value>(&answer).ok())
            .is_some_and(|compared| compared["status"] == "ahead")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        fs,
        path::Path,
        time::Instant,
    };

    const MERGE: &str = "3333333333333333333333333333333333333333";
    const LATER: &str = "4444444444444444444444444444444444444444";
    const BEFORE: &str = "2222222222222222222222222222222222222222";
    const HEALTH: &str = "https://dev.test/api/health";
    const CONFIG: &str = "DISPATCH_MODE=dev\nDISPATCH_ORIGIN=\"https://dev.test\"\n";

    /// A clock the test moves: seconds past its start, which a pause adds to.
    struct Time {
        start: Instant,
        past: Cell<u64>,
    }
    impl Time {
        fn new() -> Self {
            Self {
                start: Instant::now(),
                past: Cell::new(0),
            }
        }
        fn now(&self) -> Instant {
            self.start + Duration::from_secs(self.past.get())
        }
        fn pass(&self, seconds: u64) {
            self.past.set(self.past.get() + seconds);
        }
    }

    /// A workspace without Dev's host manager, as on a Mac, whose Dev's configuration holds
    /// `config`.
    fn workspace(config: Option<&str>) -> (tempfile::TempDir, Workspace) {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("dev/config")).unwrap();
        if let Some(config) = config {
            fs::write(temp.path().join("dev/config/platform.env"), config).unwrap();
        }
        let ws = Workspace {
            root: temp.path().to_path_buf(),
        };
        (temp, ws)
    }

    /// Dev's public health as a script: each look answers with the next commit Dev runs, the
    /// last one staying, or nothing while Dev restarts; GitHub knows `LATER` contains `MERGE`.
    struct Dev(RefCell<VecDeque<Option<&'static str>>>);
    impl Runner for Dev {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            match args {
                ["curl", "-sSf", "--max-time", _, url] => {
                    assert_eq!(*url, HEALTH);
                    let mut looks = self.0.borrow_mut();
                    let runs = match looks.len() {
                        1 => looks[0],
                        _ => looks.pop_front().unwrap(),
                    };
                    let runs = runs.ok_or("curl: (22) The requested URL returned error: 502")?;
                    Ok(serde_json::to_vec(&serde_json::json!({
                        "status": "ready",
                        "source": {"version": null, "commit": runs},
                    }))?)
                }
                ["gh", "api", endpoint] => {
                    let compared = endpoint
                        .strip_prefix(&format!("repos/{REPOSITORY}/compare/"))
                        .unwrap();
                    let status = match compared.split_once("...").unwrap() {
                        (MERGE, LATER) => "ahead",
                        _ => "behind",
                    };
                    Ok(serde_json::to_vec(
                        &serde_json::json!({ "status": status }),
                    )?)
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn without_devs_host_manager_dev_is_waited_for_by_its_public_health() {
        let (_temp, ws) = workspace(Some(CONFIG));
        let wait = |looks: Vec<Option<&'static str>>| {
            let dev = Dev(RefCell::new(looks.into()));
            let (time, pauses) = (Time::new(), RefCell::new(0));
            let clock = Clock {
                now: &|| time.now(),
                pause: &|seconds| {
                    assert_eq!(seconds, PAUSE);
                    *pauses.borrow_mut() += 1;
                    time.pass(seconds);
                },
            };
            let live = live(&ws, MERGE, &dev, &clock);
            (live, pauses.into_inner())
        };
        // Dev runs the commit before it, restarts on the merge, then runs it.
        let (live, pauses) = wait(vec![Some(BEFORE), None, Some(MERGE)]);
        assert_eq!((live.unwrap().as_str(), pauses), ("33333333", 2));
        // A later commit of main that contains it is as good.
        assert_eq!(wait(vec![Some(LATER)]).0.unwrap(), "44444444");
        // Sixteen minutes of looks on another commit, or of no answer, give up.
        let (live, pauses) = wait(vec![Some(BEFORE)]);
        assert_eq!(
            live.unwrap_err().to_string(),
            "Dev didn't run 33333333 within 16 minutes; it runs 22222222."
        );
        assert_eq!(pauses as u64, INSTALL / PAUSE);
        assert_eq!(
            wait(vec![None]).0.unwrap_err().to_string(),
            format!("{HEALTH} didn't answer within 16 minutes.")
        );
    }

    #[test]
    fn a_dev_that_hangs_is_waited_for_sixteen_minutes_and_no_more() {
        /// Dev's health hanging two minutes at each request, or until the request's bound.
        struct Hung<'a>(&'a Time);
        impl Runner for Hung<'_> {
            fn command(&self, args: &[&str], _: Option<&Path>, timeout: u64) -> Result<Vec<u8>> {
                assert_eq!((args[0], args[args.len() - 1]), ("curl", HEALTH));
                // curl's own bound is the request's.
                if let Some(at) = args.iter().position(|arg| *arg == "--max-time") {
                    assert_eq!(args[at + 1], timeout.to_string());
                }
                self.0.pass(timeout.min(125));
                Err("curl: (28) Operation timed out".into())
            }
        }
        let (_temp, ws) = workspace(Some(CONFIG));
        let time = Time::new();
        let clock = Clock {
            now: &|| time.now(),
            pause: &|seconds| time.pass(seconds),
        };
        assert_eq!(
            live(&ws, MERGE, &Hung(&time), &clock)
                .unwrap_err()
                .to_string(),
            format!("{HEALTH} didn't answer within 16 minutes.")
        );
        assert_eq!(time.past.get(), INSTALL);
    }
}
