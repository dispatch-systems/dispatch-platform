//! Where a DSP's settings are kept: a record per section in the DSP's own `settings` table,
//! `section:<id>` for the DSP's and `section:<id>:<user>` for a member's own, holding
//! `{revision, values, at, by}`. Only the DSP's own database is read or written, so one DSP's
//! values never reach another's.
use super::kinds;
use crate::{
    Result,
    db::{Db, Store, iso},
    manifest::settings::Section,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// What a section holds now, for the DSP or one member: its revision, none saved yet being 0,
/// and every field's value but a schedules field's, by id, defaults filled.
pub(crate) struct Current {
    pub revision: i64,
    pub values: Map<String, Value>,
}

/// Every record of a DSP one member reads: the DSP's, and that member's own, each by its key.
pub(crate) struct Records(HashMap<String, Value>);
impl Records {
    /// In one query of the DSP's database, whatever number of sections it has.
    pub(crate) fn load(db: &Store, dsp: &str, member: &str) -> Result<Self> {
        let own = format!(":{member}");
        // The keys from `section:` up to the next character after `:`, as the table's key
        // orders them: the DSP's have no `:` past their prefix, and the member's end in theirs.
        let rows: Vec<(String, String)> = db.dsp(dsp)?.query_as(
            "SELECT key,value FROM settings WHERE key>='section:' AND key<'section;' \
             AND (instr(substr(key,9),':')=0 OR substr(key,-length(?1))=?1)",
            [&own],
        )?;
        let records = rows
            .into_iter()
            .map(|(key, value)| Ok((key, serde_json::from_str(&value)?)))
            .collect::<Result<_>>()?;
        Ok(Self(records))
    }
    /// The record of `section`, the DSP's or the member's own as its scope says.
    pub(crate) fn of(&self, section: &Section, member: Option<&str>) -> &Value {
        self.0.get(&key(section, member)).unwrap_or(&Value::Null)
    }
}

/// The record of one section, by its key alone.
pub(crate) fn record(
    db: &Store,
    dsp: &str,
    section: &Section,
    member: Option<&str>,
) -> Result<Value> {
    db.dsp(dsp)?.setting(&key(section, member), Value::Null)
}

/// What `section` holds by its `record`: the values the record keeps, or for a section kept
/// elsewhere, those its keeper loads for the DSP; a value its field doesn't hold now, or none,
/// reads as the default.
pub(crate) fn current(db: &Store, dsp: &str, section: &Section, record: &Value) -> Result<Current> {
    let loaded;
    let kept = match section.kept {
        Some(kept) => {
            loaded = kept.load(db, dsp)?;
            &loaded
        }
        None => &record["values"],
    };
    let values = section
        .fields
        .iter()
        .filter_map(|field| {
            let value = kept
                .get(field.id)
                .filter(|value| kinds::fits(field, value))
                .cloned()
                .or_else(|| kinds::default(field))?;
            Some((field.id.to_owned(), value))
        })
        .collect();
    Ok(Current {
        revision: record["revision"].as_i64().unwrap_or(0),
        values,
    })
}

/// Records a save of `section` at `revision` by `actor`, none for values a feature adopted,
/// with its values unless they're kept elsewhere.
pub(crate) fn write(
    people: &Db,
    section: &Section,
    member: Option<&str>,
    revision: i64,
    values: &Value,
    actor: Option<&str>,
) -> Result<()> {
    let mut record = json!({"revision": revision, "at": iso(), "by": actor});
    if section.kept.is_none() {
        record["values"] = values.clone();
    }
    people.set(&key(section, member), &record)
}

/// Where a section's record is kept: `section:<id>`, or a member's own `section:<id>:<user>`.
fn key(section: &Section, member: Option<&str>) -> String {
    match member {
        Some(member) => format!("section:{}:{member}", section.id),
        None => format!("section:{}", section.id),
    }
}
