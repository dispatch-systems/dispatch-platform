//! People's photos. A person's is the first a kind of data holds under one of their IDs: their
//! IDs in the sources' order, then as they came, and for each ID the kinds in the order their
//! data is read. The tab, a person's photo and the directory all answer by this one rule.
use super::codes::valid_code;
use super::sources::{self, Identity};
use super::{IdRow, surviving};
use dispatch_core::{Result, db::Store, ensure, manifest::people::Photo};

/// The version of a person's photo, from what their IDs' sources say of them, `described` in
/// their IDs' order.
pub(super) fn version(described: &[&Identity]) -> Option<String> {
    described.iter().find_map(|x| x.photo.clone())
}

/// The photo of `code`'s person. A merged person's code answers for whoever kept their IDs.
pub(super) fn photo_of(store: &Store, dsp: &str, code: &str) -> Result<Option<Photo>> {
    ensure(valid_code(code), "not_found", 404)?;
    let db = store.dsp(dsp)?;
    let code = surviving(&db, code)?;
    let mut held: Vec<IdRow> = db.query_as("SELECT * FROM person_ids WHERE code=?", [&code])?;
    drop(db);
    held.sort_by_key(|row| (row.source(), row.linked_at.clone()));
    let kinds = sources::kinds(store, dsp)?;
    for (source, id) in held.iter().filter_map(IdRow::key) {
        for kind in kinds.iter().filter(|kind| kind.source == source) {
            if let Some(photos) = kind.photos
                && let Some(photo) = photos(store, dsp, std::slice::from_ref(&id))?
            {
                return Ok(Some(photo));
            }
        }
    }
    Ok(None)
}
