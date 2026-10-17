//! The directory: who an ID is, for a feature that shows the people its data names. The
//! feature that tells people apart offers it; `people::who`, `people::photo` and
//! `people::changed` ask it, and answer nothing in a build without one.
use crate::{
    Result, State,
    db::Store,
    manifest::{
        Upkeep,
        people::{Photo, Source},
    },
};
use std::{collections::BTreeMap, sync::Arc};

/// Who each ID is, as the feature that tells people apart answers.
pub trait Directory: Sync {
    /// Everyone holding an ID of `source`, by that ID: every ID they hold, whether someone
    /// confirmed who they are and the version of their photo.
    fn who(&self, store: &Store, dsp: &str, source: Source) -> Result<BTreeMap<String, Person>>;
    /// The photo of whoever holds `id` of `source`: the one their `Person` names the version
    /// of.
    fn photo(&self, store: &Store, dsp: &str, source: Source, id: &str) -> Result<Option<Photo>>;
    /// Tells people apart again now, as a feature's people changed outside a collection.
    fn changed(&self, state: Arc<State>, dsp: String) -> Upkeep;
}
/// One person, as the directory answers for one of their IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Person {
    /// Every ID they hold, by its source.
    pub ids: BTreeMap<Source, Vec<String>>,
    /// Whether someone confirmed who they are, rather than a match.
    pub confirmed: bool,
    /// The version of their photo, which changes when the photo does: `photo` serves it.
    pub photo: Option<String>,
}
impl Person {
    /// Their IDs of `source`, the first linked first.
    pub fn ids_of(&self, source: Source) -> &[String] {
        self.ids.get(&source).map_or(&[], Vec::as_slice)
    }
}
