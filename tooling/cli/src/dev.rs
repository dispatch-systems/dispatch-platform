//! Dev, as a workspace reaches it: through Dev's host manager where the workspace holds Dev's
//! runtime, as on dispatch-dev, or else by Dev's public health, at the address Dev's
//! configuration in the workspace names, as on a Mac. `ship` and `finish` wait here for Dev to
//! run a merge, and `status` reads what it runs.
use crate::{
    Result, Runner, preview,
    ship::{self, PAUSE},
    workspace::{self, Workspace},
};
use serde_json::Value;
use std::path::PathBuf;

/// Where a workspace without Dev's host manager, as on a Mac, finds Dev's public address, whose
/// `/api/health` names the commit Dev runs: `ORIGIN` in Dev's own configuration, this file of
/// its checkout, as `dispatchdev browser dev` reads it. The address names the owner, so it stays
/// out of the source.
const CONFIG: &str = "config/platform.env";
const ORIGIN: &str = "DISPATCH_ORIGIN";
/// How long Dev may take to install a merge before the wait gives up.
const INSTALL: u64 = 16 * 60;

/// Refuses a workspace that couldn't wait for Dev once a change merged: one without Dev's host
/// manager whose configuration names no address for Dev. `ship` asks it before pushing.
pub fn ready(ws: &Workspace) -> Result<()> {
    if host_manager(ws).is_none() {
        health(ws)?;
    }
    Ok(())
}

/// Dev's host manager, where the workspace holds Dev's runtime, as on dispatch-dev.
fn host_manager(ws: &Workspace) -> Option<PathBuf> {
    Some(ws.dev().join(".runtime/management/dispatch-host")).filter(|host| host.is_file())
}

/// The address of Dev's public health, at the origin Dev's configuration in the workspace names.
fn health(ws: &Workspace) -> Result<String> {
    let config = ws.dev().join(CONFIG);
    std::fs::read_to_string(&config)
        .ok()
        .and_then(|text| preview::read_env(&text).remove(ORIGIN))
        .map(|origin| origin.trim_matches(['"', '\'']).to_owned())
        .filter(|origin| !origin.is_empty())
        .map(|origin| format!("{origin}/api/health"))
        .ok_or_else(|| format!("{} has no {ORIGIN}.", config.display()).into())
}

/// Waits until Dev runs `commit`, or one containing it, and returns the commit it runs, short.
/// Where the workspace holds Dev, Dev's host manager waits; elsewhere, as on a Mac, Dev's public
/// health is looked at as `ship` looks at a PR, `pause` waiting between looks.
pub fn live(
    ws: &Workspace,
    commit: &str,
    runner: &dyn Runner,
    pause: &dyn Fn(u64),
) -> Result<String> {
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
        public(&health(ws)?, commit, runner, pause)?
    };
    Ok(runs.get(..8).unwrap_or(&runs).to_owned())
}

/// Looks at Dev's public health, at `health`, until Dev runs `commit`, or a later commit of
/// `main` containing it, as the host manager waits, for `INSTALL` seconds of looks; returns the
/// commit it runs. While Dev restarts on a new commit it answers nothing, and the next look asks
/// again.
fn public(health: &str, commit: &str, runner: &dyn Runner, pause: &dyn Fn(u64)) -> Result<String> {
    let mut last = None;
    for _ in 0..ship::looks(INSTALL) {
        if let Some(runs) = look(health, runner) {
            if runs == commit || contains(runner, &runs, commit) {
                return Ok(runs);
            }
            last = Some(runs);
        }
        pause(PAUSE);
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

/// The commit Dev runs, as its public health names it: none where the workspace names no
/// address for Dev, or while Dev doesn't answer. `status` reads it where the workspace holds no
/// Dev of its own.
pub fn public_commit(ws: &Workspace, runner: &dyn Runner) -> Option<String> {
    look(&health(ws).ok()?, runner)
}

/// The commit Dev's health, at `health`, names; none while Dev doesn't answer.
fn look(health: &str, runner: &dyn Runner) -> Option<String> {
    let text = workspace::text(runner, &["curl", "-sSf", health], None).ok()?;
    let health: Value = serde_json::from_str(&text).ok()?;
    Some(health["source"]["commit"].as_str()?.to_owned())
}

/// Whether `later` contains `commit`, as GitHub compares them; no answer counts as no.
fn contains(runner: &dyn Runner, later: &str, commit: &str) -> bool {
    ship::rest(runner, &format!("compare/{commit}...{later}"))
        .is_ok_and(|compared| compared["status"] == "ahead")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{REPOSITORY, machine::Machine};
    use std::{cell::RefCell, collections::VecDeque, fs, path::Path};

    const MERGE: &str = "3333333333333333333333333333333333333333";
    const LATER: &str = "4444444444444444444444444444444444444444";
    const BEFORE: &str = "2222222222222222222222222222222222222222";
    const HEALTH: &str = "https://dev.test/api/health";

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
            machine: Machine::Mac,
        };
        (temp, ws)
    }

    /// Dev's public health as a script: each look answers with the next commit Dev runs, the
    /// last one staying, or nothing while Dev restarts; GitHub knows `LATER` contains `MERGE`.
    struct Dev(RefCell<VecDeque<Option<&'static str>>>);
    impl Runner for Dev {
        fn command(&self, args: &[&str], _: Option<&Path>, _: u64) -> Result<Vec<u8>> {
            match args {
                ["curl", "-sSf", url] => {
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
        let (_temp, ws) = workspace(Some(
            "DISPATCH_MODE=dev\nDISPATCH_ORIGIN=\"https://dev.test\"\n",
        ));
        let wait = |looks: Vec<Option<&'static str>>| {
            let dev = Dev(RefCell::new(looks.into()));
            let pauses = RefCell::new(0);
            let live = live(&ws, MERGE, &dev, &|seconds| {
                assert_eq!(seconds, PAUSE);
                *pauses.borrow_mut() += 1;
            });
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
    fn without_devs_host_manager_or_address_a_change_is_refused_before_it_ships() {
        let (temp, ws) = workspace(None);
        let config = temp.path().join("dev/config/platform.env");
        assert_eq!(
            ready(&ws).unwrap_err().to_string(),
            format!("{} has no DISPATCH_ORIGIN.", config.display())
        );
        fs::write(&config, "DISPATCH_MODE=dev\n").unwrap();
        assert!(ready(&ws).is_err());
        // Dev's host manager waits for Dev without it, as on dispatch-dev.
        let host = temp.path().join("dev/.runtime/management/dispatch-host");
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(&host, "").unwrap();
        assert!(ready(&ws).is_ok());
    }
}
