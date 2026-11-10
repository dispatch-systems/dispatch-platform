//! Production captures a backup; only Dev restores and checks it, using the verified candidate.
use super::*;

/// Where Production is reached.
pub(super) const PRODUCTION_HOST: &str = "dispatch-production";
/// Every database's migrations, as the backend's tests keep them.
const SNAPSHOT: &str = "app/tests/backend/snapshots/databases.json";
/// How long a backup stands for the release: Production keeps changing after it.
const BACKUP_LASTS: f64 = 6.0 * 60.0 * 60.0;

/// Only format 2 binds genuine Dev verification, candidate identity and captured bytes.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Backup {
    format: u32,
    version: String,
    pub destination: String,
    executor: String,
    commit: String,
    runtime_digest: String,
    archive_sha256: String,
    pub proof: local::CopyProof,
}

pub(super) fn fresh(now: f64, taken: f64) -> Result<()> {
    require(
        now.is_finite() && taken.is_finite() && (0.0..=BACKUP_LASTS).contains(&(now - taken)),
        "Backup capture timestamp is future, invalid or older than six hours",
    )
}

/// The migrations in a snapshot, each as it reads: its database, its owner when a feature
/// numbers its own, its number and its name, with what it applies.
fn migrations(snapshot: &str) -> Result<BTreeSet<(String, String)>> {
    let databases: Vec<Value> = serde_json::from_str(snapshot)?;
    let mut found = BTreeSet::new();
    for database in &databases {
        let name = io::text(database, "name");
        let word = |value: &Value| value.as_str().unwrap_or_default().to_owned();
        for migration in database["migrations"].as_array().into_iter().flatten() {
            found.insert((
                format!("{name} migration {} {}", migration[0], word(&migration[1])),
                migration[2].to_string(),
            ));
        }
        for migration in database["owned"].as_array().into_iter().flatten() {
            found.insert((
                format!(
                    "{name} {} migration {} {}",
                    word(&migration[0]),
                    migration[1],
                    word(&migration[2])
                ),
                migration[3].to_string(),
            ));
        }
    }
    Ok(found)
}
/// A command Production's shell runs, by ssh, without its profile.
pub(super) fn remote(script: &str) -> String {
    format!(
        "bash --noprofile --norc -c '{}'",
        script.replace('\'', r"'\''")
    )
}
/// Copies Production's databases and configuration, with the service and its updates stopped
/// for the copy and started again whatever happens. Production's updater lets go of them after
/// 30 days (`updater::expire_release_backups`).
fn backup_script(version: &str) -> String {
    format!(
        r#"set -eu; umask 077
dest=$HOME/dispatch-platform/backups/$(date +%F-%H%M%S)-before-v{version}
public=$HOME/dispatch-platform/public
test ! -e "$dest"
systemctl --user stop dispatch-production-update.timer
trap 'systemctl --user start dispatch-production.service dispatch-production-update.timer' EXIT
systemctl --user stop dispatch-production.service
(set -a; . "$public/config/platform.env"; set +a; cd "$public/live"
 ./services/rust/dispatch-backend backup "$dest")
mkdir "$dest/config"; cp -p "$public/config/"* "$dest/config/""#
    )
}
/// The JSON line of a command's output holding `field`.
pub(super) fn reported(output: &str, field: &str) -> Result<Value> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| !value[field].is_null())
        .ok_or_else(|| format!("Production reported no {field}").into())
}

impl Release<'_> {
    /// Runs `script` on Production over ssh, answering what it printed.
    pub(super) fn on_production(&self, script: &str, timeout: u64) -> Result<String> {
        self.command(
            &[
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                PRODUCTION_HOST,
                &remote(script),
            ],
            None,
            timeout,
        )
    }
    pub(super) fn backup_path(&self) -> PathBuf {
        self.directory.join(format!(".{}-backup.json", self.tag))
    }
    /// The migrations `commit` brings since the latest published release: none before the
    /// first, or without the snapshot to tell.
    pub(super) fn new_migrations(&self, commit: &str) -> Result<Vec<String>> {
        let Some(previous) = self.previous()? else {
            return Ok(vec![]);
        };
        let at = |rev: &str| self.git(&["show", &format!("{rev}:{SNAPSHOT}")], None, 120);
        let Ok(now) = at(commit) else {
            return Ok(vec![]);
        };
        let before = at(&previous)
            .map(|text| migrations(&text))
            .unwrap_or_else(|_| Ok(BTreeSet::new()))?;
        Ok(migrations(&now)?
            .difference(&before)
            .map(|(migration, _)| migration.clone())
            .collect())
    }
    /// Refuses to publish migrations until Production has a recent backup that restores,
    /// answering whether `--backup` takes it once Production can take the release.
    pub(super) fn require_backup(&self, commit: &str) -> Result<bool> {
        let migrations = self.new_migrations(commit)?;
        if migrations.is_empty() {
            return Ok(false);
        }
        let recorded = self.backup_path().try_exists()?;
        let problem = if recorded {
            self.checked_backup(&self.prepared(Some(commit))?)
                .err()
                .map(|e| e.to_string())
        } else {
            None
        };
        if recorded && problem.is_none() {
            return Ok(false);
        }
        if self.take_backup {
            say(format!(
                "{} brings migrations ({}), so Production is backed up once it can take the \
                 release",
                self.tag,
                migrations.join(", ")
            ));
            return Ok(true);
        }
        Err(format!(
            "{} brings migrations ({}). Production needs a backup that restores first{}: rerun \
             with `--backup`, which takes it and stops Production for about a second.",
            self.tag,
            migrations.join(", "),
            problem
                .map(|p| format!(", and its saved receipt was refused ({p})"))
                .unwrap_or_default()
        )
        .into())
    }
    pub(super) fn checked_backup(&self, prepared: &releases::Provenance) -> Result<Backup> {
        let backup: Backup = serde_json::from_value(io::read_json(&self.backup_path())?)
            .map_err(|_| "Old or invalid backup receipt; a genuine Dev check is required")?;
        require(
            backup.format == 2
                && backup.executor == "dispatch-dev"
                && backup.version == self.version
                && backup.commit == prepared.commit
                && backup.runtime_digest == prepared.runtime_digest
                && backup.archive_sha256 == prepared.archive_sha256
                && backup.proof.files > 0
                && backup.proof.databases > 0
                && artifact::hex(&backup.proof.content_sha256, 64)
                && artifact::hex(&backup.proof.config_sha256, 64),
            "Backup receipt does not prove this candidate on Dev",
        )?;
        self.backup_destination(&backup.destination)?;
        fresh(self.system.now(), backup.proof.taken_at)?;
        Ok(backup)
    }
    fn backup_destination(&self, destination: &str) -> Result<()> {
        require(
            destination.starts_with('/')
                && destination.ends_with(&format!("-before-{}", self.tag))
                && destination
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c))
                && Path::new(destination).components().all(|c| {
                    matches!(
                        c,
                        std::path::Component::RootDir | std::path::Component::Normal(_)
                    )
                }),
            "Production's backup named an unexpected folder",
        )
    }
    /// Capture only on Production, then restore/integrity verification on physical Dev.
    pub(super) fn backup(&self) -> Result<Value> {
        let prepared = self.prepared(None)?;
        self.dev_preflight()?;
        say("Capturing Production's backup; Dev alone restores and checks it");
        let taken = reported(
            &self
                .on_production(&backup_script(&self.version), 900)
                .map_err(|_| "Production capture failed; private output suppressed")?,
            "destination",
        )?;
        let destination = io::text(&taken, "destination");
        self.backup_destination(&destination)?;
        let proof = self.dev_copy(&destination, &prepared, local::Check::Restore)?;
        require(
            taken["files"].as_u64() == Some(proof.files),
            "The backup didn't restore every captured file",
        )?;
        fresh(self.system.now(), proof.taken_at)?;
        let backup = Backup {
            format: 2,
            version: self.version.clone(),
            destination,
            executor: "dispatch-dev".into(),
            commit: prepared.commit,
            runtime_digest: prepared.runtime_digest,
            archive_sha256: prepared.archive_sha256,
            proof,
        };
        io::write_json(&self.backup_path(), &serde_json::to_value(&backup)?)?;
        Ok(
            json!({"version":self.version,"stage":"backup","destination":backup.destination,
            "files":backup.proof.files,"databases":backup.proof.databases,"executor":backup.executor}),
        )
    }
}
