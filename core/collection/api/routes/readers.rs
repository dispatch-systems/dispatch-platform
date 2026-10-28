//! How a live reader reads for the DSP, which Settings shows with the section that sets when it
//! reads (`Section::reader`), under that section's permission, while its feature is on.
use crate::{
    Error, Result,
    collection::readers,
    db::Store,
    ensure,
    manifest::registry,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read},
    },
};
use std::sync::LazyLock;

/// Every permission a section showing a live reader is under, any of which reaches the route.
static READING: LazyLock<String> = LazyLock::new(|| {
    let mut permissions: Vec<&str> = registry()
        .sections()
        .filter(|(_, section)| section.reader.is_some())
        .map(|(_, section)| section.permission)
        .collect();
    permissions.sort();
    permissions.dedup();
    permissions.join("|")
});

/// None in a build whose features show no live reader.
pub fn routes() -> Vec<Route> {
    if READING.is_empty() {
        return vec![];
    }
    vec![read(
        "/api/dsp/live-readers/{reader}",
        Dsp(READING.as_str()),
        reading,
    )]
}

/// The reader the path names, as the section showing it is seen: its feature and part on for
/// the DSP and shown to the member, and its permission held.
fn reading(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let reader = input.param("reader");
    let (_, section) = registry()
        .sections()
        .find(|(feature, section)| {
            section.reader == Some(reader)
                && feature.is_some_and(|feature| c.has(feature.switch.id))
                && section.part.is_none_or(|part| c.has(part))
        })
        .ok_or_else(|| Error::new("not_found", 404))?;
    ensure(c.allows(section.permission), "permission_denied", 403)?;
    Reply::of(&readers::reading(db, &c.dsp, reader)?)
}
