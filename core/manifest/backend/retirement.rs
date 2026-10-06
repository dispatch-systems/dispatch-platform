//! Owner-declared identifier retirements run at startup, before readers are opened.
use crate::{
    Result,
    db::{Store, s},
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
fn areas(value: &str) -> String {
    let mut result = Vec::new();
    for area in value.split(',').filter(|area| !area.is_empty()) {
        let next = renamed(area);
        if !result.contains(&next) {
            result.push(next);
        }
    }
    result.join(",")
}
fn read_choices(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (name, value) in fields {
                if name == "areas"
                    && let Value::Array(values) = value
                {
                    for value in values {
                        if let Some(text) = value.as_str() {
                            *value = Value::String(renamed(text).into());
                        }
                    }
                } else {
                    read_choices(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                read_choices(value);
            }
        }
        _ => {}
    }
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
        for row in store.platform.all("SELECT id,permissions FROM roles",[])? {
            let saved: Vec<String> = serde_json::from_str(s(&row,"permissions"))?;
            let mut next = Vec::new();
            for permission in &saved {
                let permission=renamed(permission).to_owned();
                if !next.contains(&permission) { next.push(permission); }
            }
            if next!=saved { store.platform.exec("UPDATE roles SET permissions=? WHERE id=?",
                [serde_json::to_string(&next)?.as_str(),s(&row,"id")])?; }
        }
        for table in ["agent_keys","agent_key_dsp_reads"] {
            for row in store.platform.all(&format!("SELECT rowid migration_row,areas FROM {table}"),[])? {
                let next = areas(s(&row,"areas"));
                if next!=s(&row,"areas") { store.platform.exec(&format!("UPDATE {table} SET areas=? WHERE rowid=?"),
                    rusqlite::params![next,row["migration_row"].as_i64()])?; }
            }
        }
        for row in store.platform.all("SELECT hash,choices FROM oauth_codes",[])? {
            let mut choices: Value = serde_json::from_str(s(&row,"choices"))?;
            let previous=choices.clone(); read_choices(&mut choices);
            if choices!=previous { store.platform.exec("UPDATE oauth_codes SET choices=? WHERE hash=?",
                [choices.to_string().as_str(),s(&row,"hash")])?; }
        }
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
