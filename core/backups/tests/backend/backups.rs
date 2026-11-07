//! A DSP's backups end to end, against a stand-in for R2: a backup holds the DSP's files and the
//! pictures it keeps in the object store, each sent once however many backups hold it, and its
//! description opens with the bucket and the key alone; an expired backup lets go of what no kept
//! backup holds; and one that fails runs again within the hour, letting go of what it sent.
use super::*;
use crate::{
    State,
    db::now,
    foundation::config::BackupStore,
    testing::{self, dsp_named, objects::Fake},
};
use axum::http::Method;
use serde_json::Value;
use std::sync::Arc;

const KEY: [u8; 32] = [7; 32];

struct Backups {
    _root: tempfile::TempDir,
    state: Arc<State>,
    bucket: Fake,
    pictures: Fake,
    dsp: String,
}
impl Backups {
    async fn start() -> Self {
        testing::install(&[], &[]);
        let (bucket, pictures) = (Fake::start().await, Fake::start().await);
        let (root, db) = testing::seeded();
        let dsp = dsp_named(&db, "Northline Logistics");
        let mut config = db.config.clone();
        config.objects = Some(pictures.store());
        config.backups = Some(BackupStore {
            objects: bucket.store(),
            key: KEY,
        });
        drop(db);
        Self {
            _root: root,
            state: State::new(config).unwrap(),
            bucket,
            pictures,
            dsp,
        }
    }
    async fn pass(&self) {
        keeper::pass(&self.state, self.state.backups.as_ref().unwrap())
            .await
            .unwrap();
    }
    async fn sql<T: Send + 'static>(
        &self,
        work: impl FnOnce(&crate::db::Store) -> crate::Result<T> + Send + 'static,
    ) -> T {
        self.state.run_bookkeeping(work).await.unwrap()
    }
    /// A backup taken by hand: its row.
    async fn taken(&self) -> Value {
        let dsp = self.dsp.clone();
        self.sql(move |db| history::ask_by_hand(&db.platform, &dsp, "owner"))
            .await;
        self.pass().await;
        let dsp = self.dsp.clone();
        self.sql(move |db| {
            db.platform
                .one(
                    "SELECT * FROM backups WHERE dsp_id=? ORDER BY created_at DESC LIMIT 1",
                    [dsp],
                )
                .map(Option::unwrap)
        })
        .await
    }
    fn key(&self, owner: &str, name: &str) -> String {
        format!("dsps/{}/{owner}/{name}", self.dsp)
    }
    /// The bucket's object `key`, in a file of its own.
    fn fetched(&self, key: &str, folder: &tempfile::TempDir) -> std::path::PathBuf {
        let path = folder.path().join(key.replace('/', "_"));
        std::fs::write(&path, self.bucket.object(key).unwrap()).unwrap();
        path
    }
}

fn files(row: &Value) -> Vec<Value> {
    serde_json::from_str(row["files"].as_str().unwrap()).unwrap()
}
fn objects(row: &Value) -> Vec<String> {
    files(row)
        .iter()
        .map(|file| file["object"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_backup_holds_the_dsps_files_each_sent_once_and_expires_without_what_others_hold() {
    let backups = &Backups::start().await;
    let picture = b"a picture of a van";
    let sha = crypto::sha(picture);
    backups
        .pictures
        .put(&format!("dsps/{}/phones/{sha}", backups.dsp), picture);

    // Its first pass keeps the schedule the DSP starts with, whose first run is still to come.
    backups.pass().await;
    let dsp = backups.dsp.clone();
    let started = backups
        .sql(move |db| Ok(schedule::schedule(&db.platform, &dsp)?.revision))
        .await;
    assert_eq!(started, 1);

    let first = backups.taken().await;
    assert_eq!(first["status"], "succeeded", "{first}");
    let held = files(&first);
    let path_of = |path: &str| {
        held.iter()
            .find(|file| file["path"] == path)
            .unwrap_or_else(|| panic!("{path} in {held:?}"))
            .clone()
    };
    assert_eq!(path_of("data/dispatch.sqlite")["item"], contents::DISPATCH);
    let kept_picture = path_of(&format!("objects/phones/{sha}"));
    assert_eq!(kept_picture["item"], contents::PICTURES);
    assert_eq!(
        first["size"].as_i64().unwrap(),
        held.iter()
            .map(|f| f["size"].as_i64().unwrap())
            .sum::<i64>()
    );

    // Its description and each file open with the bucket and the key alone.
    let vault = backups.state.backups.as_ref().unwrap();
    let id = first["id"].as_str().unwrap().to_owned();
    let folder = tempfile::tempdir().unwrap();
    let manifest_key = backups.key(MANIFESTS, &vault.manifest(&id));
    let manifest =
        seal::open_json(&KEY, &backups.dsp, &backups.fetched(&manifest_key, &folder)).unwrap();
    assert_eq!(
        (
            manifest["dsp"]["id"].as_str(),
            manifest["files"].as_array().map(Vec::len)
        ),
        (Some(backups.dsp.as_str()), Some(held.len()))
    );
    let mut opened = Vec::new();
    let sealed = backups.fetched(
        &backups.key(FILES, kept_picture["object"].as_str().unwrap()),
        &folder,
    );
    seal::open(&KEY, &backups.dsp, &sealed, &mut opened, &sha).unwrap();
    assert_eq!(opened, picture);

    // The next holds the same files and sends none of them again.
    let second = backups.taken().await;
    assert_eq!(second["status"], "succeeded", "{second}");
    for object in objects(&first) {
        assert_eq!(
            backups
                .bucket
                .asked(Method::PUT, &backups.key(FILES, &object)),
            1,
            "{object}"
        );
    }

    // Expired, the first lets go of its description, but not of what the second holds.
    let shared: Vec<String> = objects(&first)
        .into_iter()
        .filter(|object| objects(&second).contains(object))
        .collect();
    assert!(shared.contains(&kept_picture["object"].as_str().unwrap().to_owned()));
    let expire = |id: Value| async move {
        let id = id.as_str().unwrap().to_owned();
        backups
            .sql(move |db| {
                db.platform
                    .exec("UPDATE backups SET expires_at=1 WHERE id=?", [id])
            })
            .await;
        backups.pass().await;
    };
    expire(first["id"].clone()).await;
    assert!(backups.bucket.object(&manifest_key).is_none());
    for object in &shared {
        assert!(backups.bucket.object(&backups.key(FILES, object)).is_some());
    }
    expire(second["id"].clone()).await;
    let left: Vec<String> = backups
        .bucket
        .keys()
        .into_iter()
        .filter(|key| key.starts_with(&format!("dsps/{}/", backups.dsp)))
        .collect();
    assert_eq!(left, Vec::<String>::new());
    assert!(
        backups
            .pictures
            .object(&format!("dsps/{}/phones/{sha}", backups.dsp))
            .is_some()
    );
}

#[tokio::test]
async fn a_rule_whose_backup_fails_runs_again_within_the_hour() {
    let backups = Backups::start().await;
    backups.pass().await;
    let dsp = backups.dsp.clone();
    backups
        .sql(move |db| {
            db.platform
                .exec("UPDATE backup_rules SET next_run=1 WHERE dsp_id=?", [dsp])
        })
        .await;
    backups.bucket.down(true);
    backups.pass().await;
    backups.bucket.down(false);
    let dsp = backups.dsp.clone();
    let (row, next) = backups
        .sql(move |db| {
            let row = db
                .platform
                .one("SELECT * FROM backups WHERE dsp_id=?", [&dsp])?
                .unwrap();
            let kept = schedule::schedule(&db.platform, &dsp)?;
            Ok((row, kept.rules[0].next_run.unwrap()))
        })
        .await;
    assert_eq!(
        (row["status"].as_str(), row["error"].as_str()),
        (Some("failed"), Some("object_store_unavailable"))
    );
    assert!(row["rule_id"].is_string());
    assert!(next <= now() + 60 * 60 * 1000, "{next}");
}

#[tokio::test]
async fn a_rule_that_keeps_its_last_few_lets_older_ones_go_and_forever_keeps_all() {
    let backups = Backups::start().await;
    let dsp = backups.dsp.clone();
    let left = backups
        .sql(move |db| {
            let taken = |id: &str, rule: &str, at: i64, last: Option<i64>| {
                db.platform.exec(
                    "INSERT INTO backups(id,dsp_id,rule_id,status,created_at,keep_last,files) \
                     VALUES (?,?,?,'succeeded',?,?,'[]')",
                    rusqlite::params![id, dsp, rule, at, last],
                )
            };
            // Kept as its newest three, then two: the rule's newest says how many.
            taken("bak_1", "bkr_a", 1, Some(3))?;
            taken("bak_2", "bkr_a", 2, Some(3))?;
            taken("bak_3", "bkr_a", 3, Some(2))?;
            taken("bak_4", "bkr_a", 4, Some(2))?;
            // Kept forever.
            taken("bak_5", "bkr_b", 1, None)?;
            history::outnumbered(&db.platform, 10)?;
            let kept: Vec<(String, Option<i64>)> = db
                .platform
                .query_as("SELECT id,expires_at FROM backups ORDER BY id", [])?;
            Ok(kept)
        })
        .await;
    assert_eq!(
        left,
        [
            ("bak_1".to_owned(), Some(10)),
            ("bak_2".to_owned(), Some(10)),
            ("bak_3".to_owned(), None),
            ("bak_4".to_owned(), None),
            ("bak_5".to_owned(), None),
        ]
    );
}
