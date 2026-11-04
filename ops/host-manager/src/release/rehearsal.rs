//! A release that brings migrations is rehearsed before it publishes, on Production, where the
//! data stays: the release's own backend restores the copy of Production's backup it recorded
//! into a private temporary folder there, and starts it as Production would start the release,
//! under Production's configuration, without serving it (`dispatch-backend upgrade-check`).
//! Production's service is never stopped or touched; the folder is all that changes there,
//! and it's removed whatever happens. A copy that doesn't start, or starts with a carry-over
//! failed, refuses publication; any other error it logs is reported. One that starts is
//! recorded beside the release for its commit and backup, so a rerun doesn't repeat it, and
//! the release's result reports it.
use super::*;
use std::os::unix::fs::PermissionsExt;

/// What the release keeps of the rehearsal that passed.
const REHEARSED: &str = "upgrade-rehearsal.json";
/// Makes the private folder a rehearsal runs in on Production, printing where.
const FOLDER: &str =
    r#"set -eu; umask 077; mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/dispatch-upgrade-check-XXXXXX""#;

/// Restores the backup at `destination` into `work`, beside the release's backend copied
/// there, and starts the copy with it under Production's configuration, printing what it found,
/// then the end of its log when it failed.
fn rehearsal_script(work: &str, destination: &str) -> String {
    format!(
        r#"set -eu; umask 077
work={work}; public=$HOME/dispatch-platform/public
chmod 700 "$work/dispatch-backend"
"$work/dispatch-backend" restore "{destination}" "$work/root" >/dev/null
(set -a; . "$public/config/platform.env"; set +a; cd "$work"
 ./dispatch-backend upgrade-check "$work/root" 2>"$work/log") || tail -n 5 "$work/log""#
    )
}

impl Release<'_> {
    /// Before a release that brings migrations publishes, once Production has its backup:
    /// starts a copy of that backup with the release's backend, refusing publication, with what
    /// failed, unless it starts with every carry-over done.
    pub(super) fn rehearse(&self, prepared: &assets::Prepared) -> Result<()> {
        if self.new_migrations(&prepared.commit)?.is_empty() {
            return Ok(());
        }
        let destination = io::text(&io::read_json(&self.backup_path())?, "destination");
        let receipt = self.output.join(REHEARSED);
        let rehearsed = |saved: &Value| {
            saved["commit"] == prepared.commit
                && saved["runtimeDigest"] == prepared.runtime_digest
                && saved["backup"] == destination
        };
        if receipt.try_exists()? && rehearsed(&io::read_json(&receipt)?) {
            say(format!(
                "{} already started on a copy of Production's backup {destination}",
                self.tag
            ));
            return Ok(());
        }
        let build = tempfile::Builder::new()
            .prefix("dispatch-release-rehearsal-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()?;
        let backend = self.unpack_prepared(prepared, build.path())?;
        say(format!(
            "Rehearsing {} on a copy of Production's backup {destination}, on Production, which \
             keeps serving",
            self.tag
        ));
        let work = self.on_production(FOLDER, 60)?;
        require(
            work.starts_with('/')
                && work.contains("/dispatch-upgrade-check-")
                && work
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c)),
            "Production made an unexpected folder for the rehearsal",
        )?;
        let checked = self
            .command(
                &[
                    "scp",
                    "-q",
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=10",
                    backend.to_str().ok_or("Invalid backend path")?,
                    &format!("{}:{work}/dispatch-backend", backup::PRODUCTION_HOST),
                ],
                None,
                600,
            )
            .and_then(|_| self.on_production(&rehearsal_script(&work, &destination), 1800));
        let removed = self.on_production(&format!("rm -rf -- {work}"), 120);
        if let Err(error) = &removed {
            say(format!(
                "The rehearsal's folder {work} couldn't be removed from Production ({error}): \
                 remove it there"
            ));
        }
        let output =
            checked.map_err(|error| format!("The rehearsal on Production failed: {error}"))?;
        removed?;
        let found = backup::reported(&output, "migrations").map_err(|_| {
            format!(
                "Production's upgrade check reported nothing:\n{}",
                output.trim()
            )
        })?;
        // What it logged as errors, each with its fields: a failed carry-over's, its cause's,
        // and any it went on from, which Production may log at each start already.
        let errors: Vec<String> = found["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|event| format!("{} {}", io::text(event, "event"), event["fields"]))
            .collect();
        let stopped = found["stopped"]
            .as_str()
            .map(|code| format!("it stopped with {code}"));
        let carried = found["carryOverFailures"]
            .as_array()
            .is_some_and(|failed| !failed.is_empty());
        if stopped.is_some() || carried {
            let failed: Vec<String> = stopped.into_iter().chain(errors).collect();
            return Err(format!(
                "{} didn't start cleanly on a copy of Production's backup {destination}, so it \
                 isn't published:\n- {}\nProduction is unchanged. Fix it on main, then release \
                 again.",
                self.tag,
                failed.join("\n- ")
            )
            .into());
        }
        if !errors.is_empty() {
            say(format!(
                "{} logged errors on the copy that didn't stop it, as Production may at each \
                 start; look into each:\n- {}",
                self.tag,
                errors.join("\n- ")
            ));
        }
        let migrations = found["migrations"].as_object().cloned().unwrap_or_default();
        let applied = migrations.values().filter_map(Value::as_u64).sum();
        let kinds: Vec<String> = migrations
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect();
        say(format!(
            "{} started on a copy of Production's backup, with {} ({}) applied to {}",
            self.tag,
            production::count(applied, "migration"),
            kinds.join(", "),
            production::count(found["databases"].as_u64().unwrap_or(0), "database")
        ));
        io::write_json(
            &receipt,
            &json!({"commit":prepared.commit,"runtimeDigest":prepared.runtime_digest,
                "backup":destination,"databases":found["databases"],"migrations":migrations,
                "errors":found["errors"]}),
        )
    }
    /// The rehearsal that passed for this release, as kept beside it; none without migrations.
    pub(super) fn rehearsal(&self) -> Result<Option<Value>> {
        let receipt = self.output.join(REHEARSED);
        receipt
            .try_exists()?
            .then(|| io::read_json(&receipt))
            .transpose()
    }
}
