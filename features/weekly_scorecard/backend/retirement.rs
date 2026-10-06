//! Import the retired database once; the prior file becomes an inactive recovery archive.
use super::{DATABASE, STORAGE, archive};
use dispatch_core::{
    Result,
    collection::registry::{AddedStorage, added_identity, identity_seed},
    db::{Db, Kind, Store, private_dir, s},
    ensure,
};
use serde_json::{Value, json};
static LEGACY_STORAGE: AddedStorage = AddedStorage {
    id: "scorecard",
    kind: Kind::new("scorecard", 1),
    marker: "storage.scorecard",
    source: "scorecard-v1",
    verify: |_| Ok(()),
};
pub(crate) fn upgrade(store: &Store, id: &str) -> Result<()> {
    let core = store.dsp(id)?;
    let legacy_marker = core.setting("storage.scorecard", Value::Null)?;
    let current_marker = core.setting(STORAGE.marker, Value::Null)?;
    let data = store.area(id, "data")?;
    let old_dir = data.join("scorecard");
    let old_file = old_dir.join("scorecard.sqlite");
    let archive = store
        .area(id, "state")?
        .join("weekly_scorecard_migration_backup");
    if current_marker == json!(1) {
        if !legacy_marker.is_null() || old_file.exists() {
            ensure(
                legacy_marker == json!(1),
                "weekly_scorecard_migration_unmarked",
                503,
            )?;
            let target_file = data.join("weekly_scorecard/weekly_scorecard.sqlite");
            ensure(
                target_file.is_file(),
                "weekly_scorecard_migration_conflict",
                503,
            )?;
            let target = Db::open(&target_file, DATABASE)?;
            added_identity(&target, id, dispatch_cortex::PROVIDER, &STORAGE)?;
            ensure(
                target
                    .one(
                        "SELECT version FROM weekly_scorecard_transition WHERE version=1",
                        [],
                    )?
                    .is_some(),
                "weekly_scorecard_migration_conflict",
                503,
            )?;
            (STORAGE.verify)(&target)?;
            drop(target);
            if old_file.is_file() {
                drop(prior(&old_file, id)?);
                archive::finish(&old_dir, &archive)?;
            } else {
                ensure(
                    archive.join("scorecard.sqlite").is_file(),
                    "weekly_scorecard_migration_source_missing",
                    503,
                )?;
                drop(prior(&archive.join("scorecard.sqlite"), id)?);
                if old_dir.exists() {
                    archive::finish(&old_dir, &archive)?;
                }
            }
        }
        core.exec("DELETE FROM settings WHERE key='storage.scorecard'", [])?;
        return Ok(());
    }
    ensure(current_marker.is_null(), "unsupported_storage_layout", 503)?;
    if legacy_marker.is_null() {
        ensure(
            !old_file.exists(),
            "weekly_scorecard_migration_unmarked",
            503,
        )?;
        return Ok(());
    }
    ensure(
        legacy_marker == json!(1) && old_file.is_file(),
        "weekly_scorecard_migration_source_missing",
        503,
    )?;
    let source = prior(&old_file, id)?;
    let directory = private_dir(&data.join(STORAGE.id))?;
    let target = Db::create(
        &directory.join("weekly_scorecard.sqlite"),
        DATABASE,
        &identity_seed(id, dispatch_cortex::PROVIDER, STORAGE.source),
    )?;
    added_identity(&target, id, dispatch_cortex::PROVIDER, &STORAGE)?;
    if target
        .one(
            "SELECT version FROM weekly_scorecard_transition WHERE version=1",
            [],
        )?
        .is_none()
    {
        ensure(
            target.count("SELECT count(*) FROM weekly_scorecard_publications", [])? == 0,
            "weekly_scorecard_migration_conflict",
            503,
        )?;
        target.exec(
            "ATTACH DATABASE ? AS prior",
            [old_file.to_string_lossy().as_ref()],
        )?;
        target.transaction(|| {
            for (old, new) in [
                ("scorecard_publications", "weekly_scorecard_publications"),
                ("scorecard_weeks", "weekly_scorecard_weeks"),
                ("scorecard_sources", "weekly_scorecard_sources"),
            ]
            .into_iter()
            .chain(
                dispatch_cortex::weekly_scorecard::DATASETS
                    .iter()
                    .map(|dataset| {
                        (
                            if dataset.table == "driver_weekly_scorecards" {
                                "driver_scorecards"
                            } else {
                                dataset.table
                            },
                            dataset.table,
                        )
                    }),
            ) {
                let columns = source.all(&format!("PRAGMA table_info({old})"), [])?;
                let target_columns = target.all(&format!("PRAGMA table_info({new})"), [])?;
                let projection = target_columns
                    .iter()
                    .map(|column| {
                        let name = s(column, "name");
                        if name == "scope_verified"
                            && !columns.iter().any(|column| s(column, "name") == name)
                        {
                            "0".to_owned()
                        } else {
                            name.to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                target.exec(
                    &format!("INSERT INTO {new} SELECT {projection} FROM prior.{old}"),
                    [],
                )?;
                ensure(
                    target.count(&format!("SELECT count(*) FROM {new}"), [])?
                        == source.count(&format!("SELECT count(*) FROM {old}"), [])?,
                    "weekly_scorecard_migration_count_mismatch",
                    503,
                )?;
            }
            ensure(
                target.all("PRAGMA foreign_key_check", [])?.is_empty(),
                "weekly_scorecard_migration_invalid",
                503,
            )?;
            target.exec("INSERT INTO weekly_scorecard_transition VALUES (1)", [])?;
            Ok(())
        })?;
        target.exec("DETACH DATABASE prior", [])?;
    }
    (STORAGE.verify)(&target)?;
    drop(source);
    drop(target);
    // Reject an unrelated archive before committing the marker. The import receipt then
    // permits retries after an interrupted move, copy or removal of the old directory.
    archive::check(&old_dir, &archive)?;
    core.set(STORAGE.marker, &json!(1))?;
    archive::finish(&old_dir, &archive)?;
    core.exec("DELETE FROM settings WHERE key='storage.scorecard'", [])?;
    Ok(())
}
fn prior(path: &std::path::Path, id: &str) -> Result<Db> {
    let source = Db::open(path, Kind::new("scorecard", 1))?;
    added_identity(&source, id, dispatch_cortex::PROVIDER, &LEGACY_STORAGE)?;
    ensure(
        source.all("SELECT version FROM scorecard_schema", [])? == vec![json!({"version":1})],
        "unsupported_weekly_scorecard_schema",
        503,
    )?;
    Ok(source)
}
