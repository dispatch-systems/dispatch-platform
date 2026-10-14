//! A release that brings migrations waits for a Production backup that restores. Production's
//! databases are copied with the service stopped, about a second, then restored elsewhere and
//! checked, and the release records what it proved. Migrations are read from the backend's
//! snapshot of its databases, which its tests keep in step with every migration, SQL or code.
use super::*;

/// Where Production is reached for its backup.
const PRODUCTION_HOST: &str = "dispatch-production";
/// Every database's migrations, as the backend's tests keep them.
const SNAPSHOT: &str = "app/tests/backend/snapshots/databases.json";
/// How long a backup stands for the release: Production keeps changing after it.
const BACKUP_LASTS: f64 = 6.0 * 60.0 * 60.0;

/// What the release recorded of its backup.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Backup {
    format: u32,
    version: String,
    destination: String,
    files: u64,
    databases: u64,
    /// Seconds since the epoch.
    taken_at: f64,
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
fn remote(script: &str) -> String {
    format!(
        "bash --noprofile --norc -c '{}'",
        script.replace('\'', r"'\''")
    )
}
/// Copies Production's databases and configuration, with the service and its updates stopped
/// for the copy and started again whatever happens.
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
/// Restores the backup at `destination` into a folder of its own and checks every database
/// in it, then removes the folder.
fn restore_script(destination: &str) -> String {
    format!(
        r#"set -eu; umask 077
work=$(mktemp -d "${{XDG_RUNTIME_DIR:-/tmp}}/dispatch-restore-check-XXXXXX"); trap 'rm -rf "$work"' EXIT
"$HOME/dispatch-platform/public/live/services/rust/dispatch-backend" restore "{destination}" "$work/r"
python3 -c 'import json,pathlib,sqlite3,sys
found=sorted(pathlib.Path(sys.argv[1]).rglob("*.sqlite"))
failed=[d.name for d in found if sqlite3.connect(f"file:{{d}}?mode=ro",uri=True).execute("PRAGMA integrity_check").fetchone()[0]!="ok"]
print(json.dumps({{"databases":len(found),"failed":failed}}))' "$work/r""#
    )
}
/// The JSON line of a command's output holding `field`.
fn reported(output: &str, field: &str) -> Result<Value> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| !value[field].is_null())
        .ok_or_else(|| format!("Production's backup reported no {field}").into())
}

impl Release<'_> {
    fn backup_path(&self) -> PathBuf {
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
    /// Refuses to publish migrations until Production has a recent backup that restores.
    pub(super) fn require_backup(&self, commit: &str) -> Result<()> {
        let migrations = self.new_migrations(commit)?;
        if migrations.is_empty() {
            return Ok(());
        }
        let recorded = self
            .backup_path()
            .try_exists()?
            .then(|| io::read_json(&self.backup_path()))
            .transpose()?
            .map(serde_json::from_value::<Backup>)
            .transpose()?;
        let fresh = recorded.as_ref().is_some_and(|backup| {
            backup.version == self.version && self.system.now() - backup.taken_at <= BACKUP_LASTS
        });
        if let (true, Some(backup)) = (fresh, &recorded) {
            say(format!(
                "Production was backed up to {} and it restored",
                backup.destination
            ));
            return Ok(());
        }
        Err(format!(
            "{} brings migrations ({}). Production needs a backup that restores first{}: run \
             `release backup {}` with the same options, which stops Production for about a \
             second, then rerun.",
            self.tag,
            migrations.join(", "),
            if recorded.is_some() {
                ", and the last one is older than six hours"
            } else {
                ""
            },
            self.version
        )
        .into())
    }
    /// `release backup`: backs Production up, restores the copy elsewhere and checks every
    /// database, then records it for the release.
    pub(super) fn backup(&self) -> Result<Value> {
        let ssh = |script: &str| -> Result<String> {
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
                900,
            )
        };
        say("Backing up Production; it stops for about a second");
        let taken = reported(&ssh(&backup_script(&self.version))?, "destination")?;
        let destination = io::text(&taken, "destination");
        require(
            destination.starts_with('/')
                && destination.ends_with(&format!("-before-{}", self.tag))
                && destination
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c)),
            "Production's backup named an unexpected folder",
        )?;
        let files = taken["files"].as_u64().ok_or("Backup counted no files")?;
        say(format!("Restoring {destination} elsewhere to check it"));
        let checked = ssh(&restore_script(&destination))?;
        let restored = reported(&checked, "target")?;
        let integrity = reported(&checked, "databases")?;
        let databases = integrity["databases"].as_u64().unwrap_or(0);
        require(
            restored["files"].as_u64() == Some(files),
            "The backup didn't restore every file",
        )?;
        require(databases > 0, "The restored backup holds no database")?;
        require(
            integrity["failed"].as_array().is_some_and(Vec::is_empty),
            &format!(
                "The restored backup failed its integrity check: {}",
                integrity["failed"]
            ),
        )?;
        let backup = Backup {
            format: 1,
            version: self.version.clone(),
            destination,
            files,
            databases,
            taken_at: self.system.now(),
        };
        io::write_json(&self.backup_path(), &serde_json::to_value(&backup)?)?;
        say(format!(
            "Backed up {} with {files} files, and every one of its {databases} databases restored",
            backup.destination
        ));
        Ok(
            json!({"version":self.version,"stage":"backup","destination":backup.destination,
            "files":files,"databases":databases}),
        )
    }
}
