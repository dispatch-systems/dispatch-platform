//! Settings identity compatibility. Reads accept one record at either spelling; writes
//! and startup rekey that record without changing its payload. Two records are a conflict.
use crate::{
    Result,
    db::{Db, Store},
    ensure,
    manifest::{StoredKey, registry, settings::Scope},
};
use serde_json::Value;
use std::collections::BTreeSet;

/// Aliases belong to the declaring feature's DSP even while that feature is off.
pub(super) fn applicable<'a>(
    db: &Store,
    dsp: &str,
    id: &str,
    aliases: &'a [&'static str],
) -> Result<&'a [&'static str]> {
    if !aliases.is_empty() && !in_scope(id, db.feature_code(dsp)?.as_deref()) {
        return Ok(&[]);
    }
    Ok(aliases)
}

pub(super) fn in_scope(id: &str, code: Option<&str>) -> bool {
    let owner = id.split_once('.').map(|(owner, _)| owner);
    registry()
        .features
        .iter()
        .find(|f| Some(f.name) == owner)
        .is_none_or(|feature| feature.belongs_to(code))
}

pub(super) fn keys(id: &str, aliases: &[&str]) -> Vec<String> {
    std::iter::once(id)
        .chain(aliases.iter().copied())
        .map(str::to_owned)
        .collect()
}

/// Keeps presence separate from JSON null, false and empty values.
pub(super) fn record(db: &Db, keys: &[String]) -> Result<Option<(String, String)>> {
    let mut found = None;
    for key in keys {
        if let Some((value,)) =
            db.one_as::<(String,)>("SELECT value FROM settings WHERE key=?", [key])?
        {
            ensure(found.is_none(), "settings_identity_conflict", 503)?;
            found = Some((key.clone(), value));
        }
    }
    Ok(found)
}

pub(super) fn normalize(db: &Db, keys: &[String]) -> Result<()> {
    if let Some((from, _)) = record(db, keys)?
        && from != keys[0]
    {
        db.exec("UPDATE settings SET key=? WHERE key=?", [&keys[0], &from])?;
    }
    Ok(())
}

/// Reads a raw DSP setting declared by its feature, without manufacturing a saved default.
pub fn read_key(db: &Store, dsp: &str, key: &StoredKey, default: Value) -> Result<Value> {
    let aliases = applicable(db, dsp, key.id, key.aliases)?;
    let data = db.dsp(dsp)?;
    match record(&data, &keys(key.id, aliases))? {
        Some((_, value)) => Ok(serde_json::from_str(&value)?),
        None => Ok(default),
    }
}

/// Writes one authoritative raw setting. A caller's surrounding transaction may also
/// check its revision and record its audit; failure rolls the key move back with them.
pub fn write_key(db: &Store, dsp: &str, key: &StoredKey, value: &Value) -> Result<()> {
    let aliases = applicable(db, dsp, key.id, key.aliases)?;
    let keys = keys(key.id, aliases);
    let data = db.dsp(dsp)?;
    data.transaction(|| {
        normalize(&data, &keys)?;
        data.set(key.id, value)
    })
}

/// Before carry-over or owner callbacks, every member and DSP record uses this binary's
/// canonical key. The whole DSP rolls back on a conflict; JSON bytes stay unchanged.
pub(crate) fn prepare(db: &Store, dsp: &str) -> Result<()> {
    let code = db.feature_code(dsp)?;
    let data = db.dsp(dsp)?;
    data.transaction(|| {
        for feature in registry()
            .features
            .iter()
            .filter(|f| f.belongs_to(code.as_deref()))
        {
            for key in feature
                .settings_keys
                .iter()
                .filter(|key| !key.aliases.is_empty())
            {
                normalize(&data, &keys(key.id, key.aliases))?;
            }
            for section in feature
                .settings
                .iter()
                .filter(|section| !section.aliases.is_empty())
            {
                let ids = keys(section.id, section.aliases);
                if section.scope == Scope::Dsp {
                    normalize(
                        &data,
                        &ids.iter()
                            .map(|id| format!("section:{id}"))
                            .collect::<Vec<_>>(),
                    )?;
                    continue;
                }
                let mut members = BTreeSet::new();
                for id in &ids {
                    let prefix = format!("section:{id}:");
                    let end = format!("section:{id};");
                    let held: Vec<(String,)> = data.query_as(
                        "SELECT key FROM settings WHERE key>=? AND key<?",
                        [&prefix, &end],
                    )?;
                    members.extend(
                        held.into_iter()
                            .map(|(key,)| key[prefix.len()..].to_owned()),
                    );
                }
                for member in members {
                    normalize(
                        &data,
                        &ids.iter()
                            .map(|id| format!("section:{id}:{member}"))
                            .collect::<Vec<_>>(),
                    )?;
                }
            }
        }
        Ok(())
    })
}
