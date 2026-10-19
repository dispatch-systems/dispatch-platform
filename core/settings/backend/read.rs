//! How a feature reads its own sections' values: typed, one keyed lookup each.
use super::store;
use crate::{
    Result,
    db::Store,
    ensure,
    manifest::settings::{Scope, Section},
};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The DSP's values of `section`, one of its own, as its feature reads them: each field by
/// its id as a serde name, defaults filled. `pub const` the section in the feature, so its
/// code and its manifest share it.
pub fn read<T: DeserializeOwned>(db: &Store, dsp: &str, section: &Section) -> Result<T> {
    ensure(section.scope == Scope::Dsp, "settings_scope", 500)?;
    typed(db, dsp, section, None)
}
/// `member`'s own values of `section`, one each member has, as `read` reads a DSP's.
pub fn read_member<T: DeserializeOwned>(
    db: &Store,
    dsp: &str,
    member: &str,
    section: &Section,
) -> Result<T> {
    ensure(section.scope == Scope::Member, "settings_scope", 500)?;
    typed(db, dsp, section, Some(member))
}
fn typed<T: DeserializeOwned>(
    db: &Store,
    dsp: &str,
    section: &Section,
    member: Option<&str>,
) -> Result<T> {
    let record = store::record(db, dsp, section, member)?;
    let current = store::current(db, dsp, section, &record)?;
    Ok(serde_json::from_value(Value::Object(current.values))?)
}
