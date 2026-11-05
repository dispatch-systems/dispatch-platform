//! Production is checked over ssh before a release publishes, and its log after it installs.
//! Both read and never change anything. A problem before publishing refuses it, naming what to
//! fix, and a rerun picks up there; what the log shows afterwards is reported, as the release
//! is already published. A Slack app that changed with the release is saved to paste into Slack.
use super::*;

/// The least free space Production's disk keeps for an install. An install downloads the
/// runtime's archive and the build it's verified from, about 60 MB each in 0.0.35, unpacks the
/// build beside the running runtime and keeps the one before it, and a migration rewrites
/// tables beside the originals. 2 GB covers that several times over; a disk below it is
/// filling up whatever this release needs.
const FREE_SPACE: u64 = 2 * 1024 * 1024 * 1024;
/// What the release found on Production before publishing, kept beside the release.
const CHECKED: &str = "production-check.json";
/// How long a release waits for collections running on Production to finish before refusing.
/// Most finish within minutes; one waiting on someone's verification may not.
const COLLECTIONS_WAIT: Duration = Duration::from_secs(10 * 60);
/// How often Production is checked again while collections run there.
const COLLECTIONS_POLL: Duration = Duration::from_secs(30);
/// The event a migration that failed logs.
const MIGRATION_FAILED: &str = "storage.migration_failed";

/// Sets `$manifest` to the Slack app manifest Production's backend makes, configured as its
/// service is, or to `null` when it can't.
const SLACK_MANIFEST: &str = r#"public=$HOME/dispatch-platform/public
manifest=$(set -a; . "$public/config/platform.env"; set +a; cd "$public/live"
 ./services/rust/dispatch-backend slack-manifest 2>/dev/null) || manifest=null"#;

/// Reads what decides whether Production can take a release now: its updater's last failure,
/// the gh it verifies releases with, its service and update timer, its own health, its disk's
/// free space and the collections a restart would interrupt, with its Slack app manifest.
fn check_script() -> String {
    format!(
        r#"set -eu
{SLACK_MANIFEST}
python3 -c 'import json,os,pathlib,shutil,sqlite3,subprocess,sys,urllib.request
public=pathlib.Path(sys.argv[1])
failure=public/"data/platform/production-update-failure.json"
jobs=public/"data/production/jobs.sqlite"
def state(unit):
    return subprocess.run(["systemctl","--user","is-active",unit],capture_output=True,text=True).stdout.strip()
try:
    health=json.load(urllib.request.urlopen("http://127.0.0.1:5180/api/health",timeout=10)).get("status")
except Exception as error:
    health=str(error)
running=sqlite3.connect(f"file:{{jobs}}?mode=ro",uri=True).execute("SELECT count(*) FROM jobs WHERE status IN (?,?)",("running","waiting_verification")).fetchone()[0] if jobs.exists() else 0
print(json.dumps({{"failure":json.loads(failure.read_text()) if failure.exists() else None,
"gh":os.access("/usr/local/bin/gh",os.X_OK),"service":state("dispatch-production.service"),
"timer":state("dispatch-production-update.timer"),"health":health,
"free":shutil.disk_usage(public).free,"running":running,"slackManifest":json.loads(sys.argv[2])}}))' "$public" "$manifest""#
    )
}
/// Reads the warnings and errors Production's service logged since `since`, seconds since the
/// epoch, with its Slack app manifest.
fn log_script(since: i64) -> String {
    format!(
        r#"set -eu -o pipefail
{SLACK_MANIFEST}
journalctl --user -u dispatch-production.service --since @{since} -o cat -q --no-pager | python3 -c 'import json,sys
found=[]
for line in sys.stdin:
    try:
        event=json.loads(line)
    except ValueError:
        continue
    if isinstance(event,dict) and event.get("level") in ("warn","error"):
        found.append(event)
print(json.dumps({{"warn":sum(e["level"]=="warn" for e in found),"error":sum(e["level"]=="error" for e in found),
"events":found[:100],"slackManifest":json.loads(sys.argv[1])}}))' "$manifest""#
    )
}
pub(super) fn count(n: u64, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}
/// A failed migration as a person reads it: its database, its owner when a feature numbers its
/// own, its number and its name.
fn migration(fields: &Value) -> String {
    let owner = fields["owner"]
        .as_str()
        .map(|owner| format!(" {owner}"))
        .unwrap_or_default();
    format!(
        "{}{owner} migration {} {}",
        io::text(fields, "kind"),
        fields["id"],
        io::text(fields, "name")
    )
}

/// What keeps Production from taking a release, as `check_script` found it, but the collections
/// running there, which the release waits out.
fn problems(found: &Value) -> Vec<String> {
    let mut problems = vec![];
    if !found["failure"].is_null() {
        let failure = &found["failure"];
        problems.push(format!(
            "Production's updater has failed since {} ({} attempts): {}. Fix that on \
             Production, then rerun once its next update succeeds, which removes \
             public/data/platform/production-update-failure.json",
            io::text(failure, "since"),
            failure["attempts"],
            io::text(failure, "error")
        ));
    }
    if found["gh"] != true {
        problems.push(
            "Production has no executable /usr/local/bin/gh, which its updater verifies \
             each release with: install it there, owned by root, then rerun"
                .into(),
        );
    }
    for (field, what, unit) in [
        ("service", "service", "dispatch-production.service"),
        ("timer", "update timer", "dispatch-production-update.timer"),
    ] {
        let state = found[field]
            .as_str()
            .filter(|state| !state.is_empty())
            .unwrap_or("unknown");
        if state != "active" {
            problems.push(format!(
                "Production's {what} {unit} is {state}: start it with `systemctl --user \
                 start {unit}`, then rerun"
            ));
        }
    }
    let health = found["health"].as_str().unwrap_or("no status");
    if health != "ready" {
        problems.push(format!(
            "Production's own health check (http://127.0.0.1:5180/api/health) answered \
             {health}, not ready: read `journalctl --user -u dispatch-production` there, fix \
             it, then rerun"
        ));
    }
    let free = found["free"].as_u64().unwrap_or(0);
    if free < FREE_SPACE {
        problems.push(format!(
            "Production's disk has {} free, under the {} the release keeps for an install: \
             clear space, such as old copies in ~/dispatch-platform/backups, then rerun",
            gigabytes(free),
            gigabytes(FREE_SPACE)
        ));
    }
    problems
}

impl Release<'_> {
    /// Refuses to publish while Production couldn't take the release, naming each problem and
    /// what to do about it. Collections running there, which installing would interrupt, are
    /// waited out for up to `COLLECTIONS_WAIT` when nothing else is wrong. What it found is kept
    /// for after the install.
    pub(super) fn check_production(&self) -> Result<()> {
        say("Checking Production before publishing");
        let deadline = self.system.monotonic() + COLLECTIONS_WAIT;
        let mut waiting = false;
        let found = loop {
            let found = backup::reported(&self.on_production(&check_script(), 120)?, "service")?;
            let mut problems = problems(&found);
            let running = found["running"].as_u64().unwrap_or(0);
            if running > 0 && problems.is_empty() && self.system.monotonic() < deadline {
                if !waiting {
                    say(format!(
                        "Waiting up to {} minutes for {} on Production to finish",
                        COLLECTIONS_WAIT.as_secs() / 60,
                        count(running, "running collection")
                    ));
                    waiting = true;
                }
                self.system.sleep(COLLECTIONS_POLL);
                continue;
            }
            if running > 0 {
                problems.push(format!(
                    "Collections are running on Production ({running}), which installing would \
                     interrupt: rerun once they finish"
                ));
            }
            if !problems.is_empty() {
                return Err(format!(
                    "Production can't take {} yet:\n- {}",
                    self.tag,
                    problems.join("\n- ")
                )
                .into());
            }
            break found;
        };
        let free = found["free"].as_u64().unwrap_or(0);
        io::write_json(&self.output.join(CHECKED), &found)?;
        say(format!(
            "Production can take the release, with {} free",
            gigabytes(free)
        ));
        Ok(())
    }
    /// After Production installs the release: the warnings and errors its service logged since
    /// the release was published, so from before the new runtime's first start, and where the
    /// Slack app it makes is saved when that changed. Neither fails the release, which is
    /// already published.
    pub(super) fn check_installed(&self, release: &Value) -> Result<(Value, Option<PathBuf>)> {
        let published = io::text(release, "published_at");
        let log = match chrono::DateTime::parse_from_rfc3339(&published)
            .map_err(|_| format!("{} names no publication time", self.tag).into())
            .and_then(|at| self.on_production(&log_script(at.timestamp()), 120))
            .and_then(|output| backup::reported(&output, "events"))
        {
            Ok(log) => log,
            Err(error) => {
                say(format!(
                    "Production's log since the release couldn't be read: {error}. Read \
                     `journalctl --user -u dispatch-production` there, and compare its \
                     `dispatch-backend slack-manifest` with the one in {}",
                    self.output.join(CHECKED).display()
                ));
                return Ok((json!({"problem": error.to_string()}), None));
            }
        };
        let events = log["events"].as_array().cloned().unwrap_or_default();
        let errors = log["error"].as_u64().unwrap_or(0);
        let warnings = log["warn"].as_u64().unwrap_or(0);
        if errors + warnings == 0 {
            say(format!(
                "Production logged no warnings or errors since {} was published",
                self.tag
            ));
        } else {
            let mut lines: Vec<_> = events
                .iter()
                .map(|event| {
                    format!(
                        "- {} {} {} {}",
                        io::text(event, "at"),
                        io::text(event, "level"),
                        io::text(event, "event"),
                        event["fields"]
                    )
                })
                .collect();
            if errors + warnings > lines.len() as u64 {
                lines.push(format!(
                    "- and {} more",
                    errors + warnings - lines.len() as u64
                ));
            }
            say(format!(
                "Production logged {} and {} since {} was published; look into each:\n{}",
                count(errors, "error"),
                count(warnings, "warning"),
                self.tag,
                lines.join("\n")
            ));
        }
        let failed: Vec<String> = events
            .iter()
            .filter(|event| event["event"] == MIGRATION_FAILED)
            .map(|event| migration(&event["fields"]))
            .collect();
        for failure in &failed {
            say(format!("A migration failed on Production: {failure}"));
        }
        let report = json!({"since":published,"errors":errors,"warnings":warnings,
            "migrationFailures":failed,"events":events});
        Ok((report, self.changed_slack_app(&log["slackManifest"])?))
    }
    /// Saves the Slack app manifest Production makes after the install when it differs from
    /// the one before publishing, answering where. A release published before Production was
    /// checked has none to compare with.
    fn changed_slack_app(&self, after: &Value) -> Result<Option<PathBuf>> {
        let checked = self.output.join(CHECKED);
        if !checked.try_exists()? {
            return Ok(None);
        }
        if after.is_null() {
            say(
                "Production printed no Slack manifest after the install, so whether its Slack \
                 app changed is unknown: run `dispatch-backend slack-manifest` there",
            );
            return Ok(None);
        }
        if io::read_json(&checked)?["slackManifest"] == *after {
            return Ok(None);
        }
        let saved = self
            .directory
            .join(format!("{}-slack-manifest.json", self.tag));
        fs::write(&saved, serde_json::to_string_pretty(after)? + "\n")?;
        say(format!(
            "The Slack app changed with this release: paste {} into the Dispatch app in Slack, \
             under App Manifest at api.slack.com/apps",
            saved.display()
        ));
        Ok(Some(saved))
    }
}
