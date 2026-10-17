//! The people slot: who each feature's data names, for the feature that tells them apart.
//! A feature reads only its own storage; that feature joins what every feature names by ID.
//! The directory answers the other way round: who an ID is, for a feature showing people.
//! The README's "People" section is the guide to connecting a feature.
mod ids;
mod rows;

pub use crate::manifest::services::directory::{Directory, Person};
pub use ids::{Kind, KindInfo, Source, SourceInfo};
pub use rows::{Named, Records, Workdays, named, records, worked};

use crate::{
    Result, State,
    db::Store,
    manifest::{Feature, Registry, registry},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// How one kind of a feature's data names people, for the feature that tells them apart: what
/// it is, whose IDs it names them by, and who it names. A feature connects by declaring one in
/// its manifest's `people`, most often with one query:
///
/// ```ignore
/// pub const PEOPLE: People = People::new(
///     KindInfo { id: "fuel", order: 80, label: "Fuel", icon: "Fuel" },
///     dispatch_cortex::SOURCE,
///     |store, dsp| {
///         people::named(
///             &*store.fuel_db(dsp)?,
///             "SELECT transporter_id id,driver_name name,min(day) first,max(day) last \
///              FROM fill_ups GROUP BY 1,2 ORDER BY last DESC",
///         )
///     },
/// )
/// .records("fill-up", |store, dsp| {
///     people::records(
///         &*store.fuel_db(dsp)?,
///         "SELECT transporter_id id,count(*) count,max(day) last FROM fill_ups GROUP BY 1",
///     )
/// });
/// ```
///
/// It is read while the feature is on, and every ID it names gets a person. The rest says what
/// the data knows besides: how much it holds of each ID (`records`), whether each record shows
/// them driving (`driving`), the days each worked (`workdays`), which departments drive
/// (`driver_departments`), their photos (`photos`), and whether its list is only of people the
/// DSP can reach (`contacts_only`).
pub struct People {
    /// What it is, as each person's sheet lists it.
    pub kind: KindInfo,
    /// The IDs it names people by: a collector's, as `dispatch_cortex::SOURCE` for Amazon's
    /// transporter IDs, or a source of its own.
    pub source: Source,
    /// Everyone it names, once for each way it writes a name, the newest first: the first
    /// spelling read is the one shown. `named` reads them from a query.
    pub read: Reader<Vec<Named>>,
    /// How much it holds of each ID, counted in what one record is, singular ("inspection"):
    /// the person's sheet lists it so. `records` reads it from a query.
    pub records: Option<(&'static str, Reader<Vec<Records>>)>,
    /// Whether each of its records shows the person driving, as a route does: someone it
    /// counts drives, unless their source's departments say otherwise.
    pub driving: bool,
    /// The days it saw each ID at work, and the days it was collected for: two people who
    /// never worked the same days are less likely one.
    pub workdays: Option<Reader<Workdays>>,
    /// The departments the DSP counts as drivers, as the feature's settings say: anyone it
    /// names outside them works in the office. `None` while they say nothing.
    pub driver_departments: Option<Reader<Option<BTreeSet<String>>>>,
    /// Serves the photos it names in `read`'s `photo` column, each by its version. A `photo`
    /// column of a kind without it says nothing.
    pub photos: Option<Photos>,
    /// Whether its list is of people the DSP can reach, as a chat workspace's members, not of
    /// whom it employs: being on it never says someone still works for the DSP, and someone
    /// only such lists name works in the office while one lists them.
    pub contacts_only: bool,
}
/// Reads what one kind of data holds for a DSP.
pub type Reader<T> = fn(&Store, &str) -> Result<T>;
/// A photo: its kind, as `image/png`, and its bytes.
pub type Photo = (&'static str, Vec<u8>);
/// Serves the photo it holds of the first of some IDs it has one of.
pub type Photos = fn(&Store, &str, &[String]) -> Result<Option<Photo>>;

impl People {
    /// A kind of data that names people by `source`, as `read` reads them, and says nothing
    /// more.
    pub const fn new(kind: KindInfo, source: Source, read: Reader<Vec<Named>>) -> Self {
        Self {
            kind,
            source,
            read,
            records: None,
            driving: false,
            workdays: None,
            driver_departments: None,
            photos: None,
            contacts_only: false,
        }
    }
    /// Counts how much it holds of each ID, in `unit`: what one record is, singular.
    pub const fn records(self, unit: &'static str, records: Reader<Vec<Records>>) -> Self {
        Self {
            records: Some((unit, records)),
            ..self
        }
    }
    /// Each of its records shows the person driving.
    pub const fn driving(self) -> Self {
        Self {
            driving: true,
            ..self
        }
    }
    /// Says when each ID worked.
    pub const fn workdays(self, workdays: Reader<Workdays>) -> Self {
        Self {
            workdays: Some(workdays),
            ..self
        }
    }
    /// Says which departments drive.
    pub const fn driver_departments(self, departments: Reader<Option<BTreeSet<String>>>) -> Self {
        Self {
            driver_departments: Some(departments),
            ..self
        }
    }
    /// Serves the photos it names.
    pub const fn photos(self, photos: Photos) -> Self {
        Self {
            photos: Some(photos),
            ..self
        }
    }
    /// Lists people the DSP can reach, not whom it employs.
    pub const fn contacts_only(self) -> Self {
        Self {
            contacts_only: true,
            ..self
        }
    }
    /// The kind of data it is, as a person's sheet lists it.
    pub const fn kind(&'static self) -> Kind {
        Kind::new(&self.kind)
    }
    /// What one of its records is, when it counts them.
    pub fn unit(&self) -> Option<&'static str> {
        self.records.map(|(unit, _)| unit)
    }
}

/// Who holds each ID of `source` the DSP's people have, as the directory answers; nobody in a
/// build without one.
pub fn who(store: &Store, dsp: &str, source: Source) -> Result<BTreeMap<String, Person>> {
    registry().directory().map_or_else(
        || Ok(BTreeMap::new()),
        |directory| directory.who(store, dsp, source),
    )
}
/// The photo of whoever holds `id` of `source`, as the directory finds it; none in a build
/// without one. A feature serves it from a route of its own, behind its own permission.
pub fn photo(store: &Store, dsp: &str, source: Source, id: &str) -> Result<Option<Photo>> {
    registry().directory().map_or(Ok(None), |directory| {
        directory.photo(store, dsp, source, id)
    })
}
/// Has the DSP's people told apart again now, as new IDs may be there outside a collection,
/// as when a workspace's members are read: a collection's are after it ends.
pub async fn changed(state: Arc<State>, dsp: String) {
    if let Some(directory) = registry().directory() {
        directory.changed(state, dsp).await;
    }
}

impl Registry {
    /// Every kind of data that names people, in the order their spellings are read: by
    /// their order, then id.
    pub fn people(&self) -> Vec<&'static People> {
        self.people_of(|_| true)
    }
    /// The kinds of data the features `of` picks name people with, in the order their
    /// spellings are read.
    pub fn people_of(&self, of: impl Fn(&Feature) -> bool) -> Vec<&'static People> {
        let mut people: Vec<&'static People> = self
            .features
            .iter()
            .filter(|feature| of(feature))
            .flat_map(|feature| feature.people.iter().copied())
            .collect();
        people.sort_by_key(|people| (people.kind.order, people.kind.id));
        people
    }
}

/// Panics unless each kind of data that names people has an id of its own, and each source of
/// IDs is declared once, so one id never names two different sources.
pub(crate) fn check(registry: &Registry) {
    let people = registry.people();
    for (index, kind) in people.iter().enumerate() {
        assert!(
            people[..index]
                .iter()
                .all(|other| other.kind.id != kind.kind.id),
            "{} names people twice",
            kind.kind.id
        );
        assert!(
            people[..index].iter().all(|other| {
                let (a, b) = (other.source, kind.source);
                a != b || (a.label() == b.label() && a.order() == b.order())
            }),
            "the source {} is declared twice",
            kind.source.as_str()
        );
    }
}

/// The sources and kinds of data that name people, which the app's export writes to TypeScript.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(cfg, Source, Kind)
}
