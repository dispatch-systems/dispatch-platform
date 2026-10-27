//! The stand-in for Driver Match's `directory`, always on: who people are, as a test says.
use super::Known;
use dispatch_core::{
    Result,
    db::Store,
    manifest::{
        Feature, feature, mandatory,
        people::{Directory, Everyone, Person, Photo},
    },
};
use std::collections::BTreeMap;

pub(super) static NAMING: Feature = Feature {
    switch: mandatory("naming", "Naming"),
    exposes: &[dispatch_core::expose!(
        dyn Directory,
        &STAND_IN,
        "directory",
        "Who people are, as a test says.",
    )],
    ..feature("naming")
};
struct Naming;
static STAND_IN: Naming = Naming;
/// Who people are at each DSP.
static PEOPLE: Known<Vec<Person>> = Known::new();

/// Has it answer that the DSP's people are `people`.
pub(super) fn know(dsp: &str, people: Vec<Person>) {
    PEOPLE.of(dsp, |known| *known = people);
}

impl Directory for Naming {
    fn everyone(&self, _: &Store, dsp: &str) -> Result<Everyone> {
        let people = PEOPLE.of(dsp, |known| known.clone());
        Ok(Everyone::new(people, BTreeMap::new()))
    }
    fn photo(&self, _: &Store, _: &str, _: &str) -> Result<Option<Photo>> {
        Ok(None)
    }
}
