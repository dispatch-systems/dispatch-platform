//! Owner-declared identifier retirements run at startup, before readers are opened.
use crate::{
    Result,
    db::{Db, Store, s},
    manifest::registry,
};
use serde_json::Value;
fn renamed(value: &str) -> &str {
    registry()
        .features
        .iter()
        .flat_map(|feature| feature.retired_identifiers)
        .find(|(old, _)| *old == value)
        .map_or(value, |(_, new)| *new)
}
/// Renames the permissions retired identifiers named in the roles `db` keeps: the platform's
/// from before DSPs kept their own, and each DSP's.
pub(crate) fn roles(db: &Db) -> Result<()> {
    db.transaction(|| {
        for row in db.all("SELECT id,permissions FROM roles", [])? {
            let saved: Vec<String> = serde_json::from_str(s(&row, "permissions"))?;
            let mut next = Vec::new();
            for permission in &saved {
                let permission = renamed(permission).to_owned();
                if !next.contains(&permission) {
                    next.push(permission);
                }
            }
            if next != saved {
                db.exec(
                    "UPDATE roles SET permissions=? WHERE id=?",
                    [serde_json::to_string(&next)?.as_str(), s(&row, "id")],
                )?;
            }
        }
        Ok(())
    })
}
pub(crate) fn platform(store: &Store) -> Result<()> {
    store.platform.transaction(|| {
        for (old,new) in registry().features.iter().flat_map(|feature| feature.retired_identifiers) {
            store.platform.exec("INSERT INTO dsp_features(dsp_id,feature,enabled,changed_by,changed_at) \
                SELECT dsp_id,?,enabled,changed_by,changed_at FROM dsp_features old WHERE feature=? \
                AND NOT EXISTS(SELECT 1 FROM dsp_features current WHERE current.dsp_id=old.dsp_id AND current.feature=?)",
                [new,old,new])?;
            store.platform.exec("DELETE FROM dsp_features WHERE feature=?", [old])?;
        }
        roles(&store.platform)?;
        for (old,new) in registry().features.iter().flat_map(|feature| feature.retired_identifiers)
            .filter(|(old,_)| !old.contains('.')) {
            store.platform.exec("UPDATE audit SET action=? || substr(action,length(?) + 1) WHERE substr(action,1,length(?) + 1)=? || '.'",
                [new,old,old,old])?;
        }
        Ok(())
    })?;
    let retired: Vec<&str> = registry()
        .features
        .iter()
        .flat_map(|feature| feature.retired_identifiers)
        .map(|(old, _)| *old)
        .collect();
    let retired = serde_json::to_string(&retired)?;
    store.jobs.transaction(|| {
        for row in store.jobs.all(
            "SELECT id,kind,request FROM jobs WHERE kind IN (SELECT value FROM json_each(?)) \
            OR json_extract(request,'$.collection') IN (SELECT value FROM json_each(?))",
            [&retired, &retired],
        )? {
            let mut request: Value = serde_json::from_str(s(&row, "request"))?;
            let previous = request.clone();
            if let Some(collection) = request.get_mut("collection")
                && let Some(name) = collection.as_str()
            {
                *collection = Value::String(renamed(name).into());
            }
            let kind = renamed(s(&row, "kind"));
            if kind != s(&row, "kind") || request != previous {
                store.jobs.exec(
                    "UPDATE jobs SET kind=?,request=? WHERE id=?",
                    [kind, request.to_string().as_str(), s(&row, "id")],
                )?;
            }
        }
        Ok(())
    })
}
pub(crate) fn dsp(store: &Store, id: &str) -> Result<()> {
    let data = store.dsp(id)?;
    data.transaction(|| {
        for (old, new) in registry()
            .features
            .iter()
            .flat_map(|feature| feature.retired_identifiers)
        {
            data.exec(
                "UPDATE collection_schedules SET collection=? WHERE collection=?",
                [new, old],
            )?;
        }
        Ok(())
    })?;
    for feature in registry().features {
        if let Some(upgrade) = feature.upgrade_storage {
            upgrade(store, id)?;
        }
    }
    Ok(())
}
