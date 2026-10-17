//! The people slot: who each feature's data names, for the one feature that tells them apart.
//! A feature reads only its own storage; that feature joins what every feature names by ID.
//! The directory answers the other way round: who an ID is, for a feature showing people.
use crate::{
    Result, State,
    db::{Db, Store},
    manifest::{Registry, Upkeep, registry},
};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock},
};

/// A source of the IDs people are known by, as the owner of those IDs declares it: Paycom's
/// employee codes, Amazon's transporter IDs. Each kind of data naming people by its IDs names
/// it as its source.
pub struct SourceData {
    /// Permanent: people's IDs are kept under it.
    pub id: &'static str,
    /// How it is named where people's IDs are listed: "Paycom".
    pub label: &'static str,
    /// Its place in the one order sources are listed in everywhere.
    pub order: u16,
}
/// Where an ID comes from: compared, hashed, stored, sent and read by its id, and in
/// TypeScript the union of the declared ids.
#[derive(Clone, Copy)]
pub struct DriverSource(&'static SourceData);
/// Every source the features' people declare, in their order.
static SOURCES: LazyLock<Vec<DriverSource>> = LazyLock::new(|| {
    let mut all: Vec<DriverSource> = registry()
        .people()
        .iter()
        .map(|people| people.source())
        .collect();
    all.sort();
    all.dedup();
    all
});
impl DriverSource {
    pub const fn new(data: &'static SourceData) -> Self {
        Self(data)
    }
    /// Every source, in the order they are listed everywhere.
    pub fn all() -> impl Iterator<Item = Self> {
        SOURCES.iter().copied()
    }
    /// The declared source `id` names.
    pub fn parse(id: &str) -> Option<Self> {
        Self::all().find(|source| source.0.id == id)
    }
    pub const fn as_str(self) -> &'static str {
        self.0.id
    }
    pub const fn label(self) -> &'static str {
        self.0.label
    }
}
impl PartialEq for DriverSource {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for DriverSource {}
impl std::hash::Hash for DriverSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id.hash(state);
    }
}
impl PartialOrd for DriverSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DriverSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0.order, self.0.id).cmp(&(other.0.order, other.0.id))
    }
}
impl std::fmt::Debug for DriverSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.id)
    }
}
impl Serialize for DriverSource {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.id)
    }
}
impl<'de> Deserialize<'de> for DriverSource {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::parse(&id).ok_or_else(|| serde::de::Error::custom("unknown source of IDs"))
    }
}
impl ToSql for DriverSource {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.0.id.into())
    }
}
/// A source no feature in this build declares any more reads as an error: read it as text
/// where one may be stored.
impl FromSql for DriverSource {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()
            .and_then(|id| Self::parse(id).ok_or(FromSqlError::InvalidType))
    }
}
#[cfg(feature = "ts")]
impl ts_rs::TS for DriverSource {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    const IS_ENUM: bool = true;
    fn docs() -> Option<String> {
        Some("/**\n * Where an ID comes from.\n */\n".to_owned())
    }
    fn name(_: &ts_rs::Config) -> String {
        "DriverSource".to_owned()
    }
    // With nothing declared, as in a build without features, nothing is one.
    fn inline(_: &ts_rs::Config) -> String {
        let ids: Vec<_> = SOURCES
            .iter()
            .map(|source| format!("{:?}", source.0.id))
            .collect();
        if ids.is_empty() {
            "never".to_owned()
        } else {
            ids.join(" | ")
        }
    }
    fn decl(cfg: &ts_rs::Config) -> String {
        format!("type DriverSource = {};", Self::inline(cfg))
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("core/manifest/api/generated/DriverSource.ts".into())
    }
}

/// A kind of collected data a person can appear in, as the feature whose data it is
/// declares it in its `people`.
pub struct PeopleData {
    /// Permanent: answers about people name it.
    pub id: &'static str,
    /// Its place in the one order the kinds are listed in everywhere.
    pub order: u16,
    /// How it is named where people are listed: "DVIC".
    pub label: &'static str,
    /// What one of its records is, singular, to count a person's: "inspection".
    pub unit: &'static str,
    /// Its icon where people are listed, by the name lucide-react exports it as:
    /// "ClipboardCheck".
    pub icon: &'static str,
}
/// The collected data a person can appear in: compared, hashed, sent and read by its id, and
/// in TypeScript the union of the declared ids.
#[derive(Clone, Copy)]
pub struct DriverData(&'static PeopleData);
/// Every kind the features' people declare, in their order.
static DATA: LazyLock<Vec<DriverData>> = LazyLock::new(|| {
    let mut all: Vec<DriverData> = registry()
        .people()
        .iter()
        .map(|people| people.data())
        .collect();
    all.sort();
    all.dedup();
    all
});
impl DriverData {
    pub const fn new(data: &'static PeopleData) -> Self {
        Self(data)
    }
    /// Every kind, in the order they are listed everywhere.
    pub fn all() -> impl Iterator<Item = Self> {
        DATA.iter().copied()
    }
    pub const fn as_str(self) -> &'static str {
        self.0.id
    }
    pub const fn label(self) -> &'static str {
        self.0.label
    }
    pub const fn unit(self) -> &'static str {
        self.0.unit
    }
    pub const fn icon(self) -> &'static str {
        self.0.icon
    }
}
impl PartialEq for DriverData {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for DriverData {}
impl std::hash::Hash for DriverData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id.hash(state);
    }
}
impl PartialOrd for DriverData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DriverData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0.order, self.0.id).cmp(&(other.0.order, other.0.id))
    }
}
impl std::fmt::Debug for DriverData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.id)
    }
}
impl Serialize for DriverData {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.id)
    }
}
impl<'de> Deserialize<'de> for DriverData {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        DATA.iter()
            .copied()
            .find(|named| named.0.id == id)
            .ok_or_else(|| serde::de::Error::custom("unknown kind of collected data"))
    }
}
#[cfg(feature = "ts")]
impl ts_rs::TS for DriverData {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    const IS_ENUM: bool = true;
    fn docs() -> Option<String> {
        Some("/**\n * The collected data a person can appear in.\n */\n".to_owned())
    }
    fn name(_: &ts_rs::Config) -> String {
        "DriverData".to_owned()
    }
    // With nothing declared, as in a build without features, nothing is one.
    fn inline(_: &ts_rs::Config) -> String {
        let ids: Vec<_> = DATA
            .iter()
            .map(|named| format!("{:?}", named.0.id))
            .collect();
        if ids.is_empty() {
            "never".to_owned()
        } else {
            ids.join(" | ")
        }
    }
    fn decl(cfg: &ts_rs::Config) -> String {
        format!("type DriverData = {};", Self::inline(cfg))
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("core/manifest/api/generated/DriverData.ts".into())
    }
}
/// How one kind of a feature's data names people, for the feature that tells them apart: what
/// it is, whose IDs it names them by, and who it names. A feature connects by declaring one in its manifest's
/// `people`, most often with one query:
///
/// ```ignore
/// pub const PEOPLE: People = People::new(
///     PeopleData { id: "dvic", order: 40, label: "DVIC", unit: "inspection", icon: "ClipboardCheck" },
///     &dispatch_cortex::PEOPLE,
///     |store, dsp| {
///         people::named(
///             &*store.dvic_db(dsp)?,
///             "SELECT transporter_id id,transporter_name name,min(start_date) first,\
///              max(start_date) last FROM dvic_inspections GROUP BY 1,2 ORDER BY last DESC",
///         )
///     },
/// );
/// ```
///
/// It is read while the feature is on, and every ID it names gets a person.
/// The rest is for what the data knows besides: how much it holds of each ID (`records`),
/// the days each worked (`workdays`), which departments drive (`driver_departments`), their
/// photos (`photos`), whether it shortens names (`initials`), and whether its list is only of
/// people the DSP can reach (`contacts_only`).
pub struct People {
    /// What it is, as each person's sheet lists it.
    pub kind: PeopleData,
    /// The IDs it names people by: a collector's, as `&dispatch_cortex::PEOPLE` for Amazon's
    /// transporter IDs, or a source of its own.
    pub ids: &'static SourceData,
    /// Everyone it names, once for each way it writes a name, the newest first. `named` reads
    /// them from a query.
    pub read: Reader<Vec<Named>>,
    /// How much it holds of each ID, in its unit: the person's sheet counts it, and data
    /// showing someone at work tells a driver from office staff. `records` reads it from a
    /// query.
    pub records: Option<Reader<Vec<Records>>>,
    /// The days it saw each ID at work, and the days it was collected for: two people who
    /// never worked the same days are less likely one.
    pub workdays: Option<Reader<Workdays>>,
    /// The departments the DSP counts as drivers, as the feature's settings say: anyone it
    /// names outside them works in the office. `None` while they say nothing.
    pub driver_departments: Option<Reader<Option<BTreeSet<String>>>>,
    /// Serves the photos it names in `read`, each with its version.
    pub photos: Option<Photos>,
    /// Whether it may write a last name as a bare initial, as "Jordan K.": its spellings are
    /// read after every other kind's.
    pub initials: bool,
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
    /// A kind of data that names people by `ids`, as `read` reads them, and says nothing more.
    pub const fn new(kind: PeopleData, ids: &'static SourceData, read: Reader<Vec<Named>>) -> Self {
        Self {
            kind,
            ids,
            read,
            records: None,
            workdays: None,
            driver_departments: None,
            photos: None,
            initials: false,
            contacts_only: false,
        }
    }
    /// Counts how much it holds of each ID.
    pub const fn records(self, records: Reader<Vec<Records>>) -> Self {
        Self {
            records: Some(records),
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
    /// Reads its spellings after every other kind's, as it may shorten a last name.
    pub const fn initials(self) -> Self {
        Self {
            initials: true,
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
    /// The kind of data it is, as a person's activity lists it.
    pub const fn data(&'static self) -> DriverData {
        DriverData::new(&self.kind)
    }
    /// The source whose IDs it names people by.
    pub const fn source(&self) -> DriverSource {
        DriverSource::new(self.ids)
    }
    /// Where its spellings are read among every kind's: by its order, after every other kind
    /// if it shortens names.
    pub const fn reading_order(&self) -> (bool, u16) {
        (self.initials, self.kind.order)
    }
}

/// An ID as one kind of data names it. `named` reads one from each row of a query by its
/// columns' names: `id` and `name`, and any of `first` and `last` (the days it saw them),
/// `listed` (on its source's current roster, 1), `email`, `phone`, `roles` (a JSON list),
/// `department`, `position` and `photo` (its version).
#[derive(Clone, Debug, Default)]
pub struct Named {
    pub id: String,
    /// As written, spaces and all.
    pub name: String,
    pub department: Option<String>,
    pub position: Option<String>,
    /// The first and last days it saw them.
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On its source's current roster.
    pub listed: bool,
    /// The email addresses and phone numbers it has for them, as written.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// What its source says they do: `Driver`, `Helper`.
    pub roles: Vec<String>,
    /// The version of the photo it holds of them, which changes when the photo does.
    pub photo: Option<String>,
    /// The source's other IDs for the same person, as their account in each of several
    /// workspaces: Driver Match keeps them all with one person.
    pub same_as: Vec<String>,
}
impl Named {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ..Self::default()
        }
    }
    /// One row of a query, by its columns' names.
    pub fn from_row(row: &Value) -> Self {
        let text = |key: &str| {
            row[key]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Self {
            id: text("id").unwrap_or_default(),
            name: text("name").unwrap_or_default(),
            department: text("department"),
            position: text("position"),
            first_seen: text("first"),
            last_seen: text("last"),
            listed: row["listed"].as_i64() == Some(1) || row["listed"] == true,
            emails: text("email").into_iter().collect(),
            phones: text("phone").into_iter().collect(),
            roles: text("roles")
                .and_then(|roles| serde_json::from_str(&roles).ok())
                .unwrap_or_default(),
            photo: text("photo"),
            same_as: vec![],
        }
    }
    /// Extends the days it saw them by a first and last day; an empty one says nothing.
    pub fn seen(&mut self, first: &str, last: &str) {
        if !first.is_empty() && self.first_seen.as_deref().is_none_or(|f| first < f) {
            self.first_seen = Some(first.to_owned());
        }
        if !last.is_empty() && self.last_seen.as_deref().is_none_or(|l| last > l) {
            self.last_seen = Some(last.to_owned());
        }
    }
}
/// Everyone a query names, a row each, as `Named` reads them.
pub fn named(db: &Db, sql: &str) -> Result<Vec<Named>> {
    Ok(db.all(sql, [])?.iter().map(Named::from_row).collect())
}

/// How much one kind of data holds of an ID, in its unit, and the last day it has.
#[derive(Clone, Debug)]
pub struct Records {
    pub id: String,
    pub count: usize,
    pub last: String,
}
/// How much a query counts of each ID, from its columns `id`, `count` and `last`.
pub fn records(db: &Db, sql: &str) -> Result<Vec<Records>> {
    Ok(db
        .all(sql, [])?
        .iter()
        .map(|row| Records {
            id: crate::db::s(row, "id").to_owned(),
            count: crate::db::n(row, "count").max(0) as usize,
            last: crate::db::s(row, "last").to_owned(),
        })
        .collect())
}

/// When one kind of data saw people at work.
#[derive(Debug, Default)]
pub struct Workdays {
    /// Each ID with a day it worked.
    pub worked: Vec<(String, String)>,
    /// The days it was collected for, so that a day outside them says nothing.
    pub collected: Vec<String>,
}
impl Workdays {
    /// Data collected for each day it holds, as a day's routes are: whoever it names on a
    /// day worked it, and every day it holds was collected.
    pub fn daily(worked: Vec<(String, String)>) -> Self {
        let collected = worked.iter().map(|(_, day)| day.clone()).collect();
        Self { worked, collected }
    }
}

/// Who an ID is, as the one feature that tells people apart answers, for a feature that shows
/// people its data names.
pub trait Directory: Sync {
    /// Everyone holding an ID of `source`, by that ID: their code, every ID they hold,
    /// whether someone confirmed who they are and the version of their photo.
    fn people(
        &self,
        store: &Store,
        dsp: &str,
        source: DriverSource,
    ) -> Result<BTreeMap<String, Person>>;
    /// The photo of whoever holds `id` of `source`, from whichever kind of data holds one: the
    /// one their `Person` names the version of.
    fn photo(
        &self,
        store: &Store,
        dsp: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<Option<Photo>>;
    /// Tells people apart again now, as a feature's people changed outside a collection.
    fn changed(&self, state: Arc<State>, dsp: String) -> Upkeep;
}
/// One person, as the directory answers for one of their IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Person {
    pub code: String,
    /// Every ID they hold, by its source.
    pub ids: BTreeMap<DriverSource, Vec<String>>,
    /// Whether someone confirmed who they are, rather than a match.
    pub confirmed: bool,
    /// The version of their photo, which changes when the photo does: `photo` serves it.
    pub photo: Option<String>,
}
impl Person {
    /// Their IDs of `source`, the first linked first.
    pub fn ids_of(&self, source: DriverSource) -> &[String] {
        self.ids.get(&source).map_or(&[], Vec::as_slice)
    }
}
/// Who holds each ID of `source` the DSP's people have, as the directory answers; nobody in a
/// build without one.
pub fn who(store: &Store, dsp: &str, source: DriverSource) -> Result<BTreeMap<String, Person>> {
    registry().directory().map_or_else(
        || Ok(BTreeMap::new()),
        |directory| directory.people(store, dsp, source),
    )
}
/// The photo of whoever holds `id` of `source`, as the directory finds it; none in a build
/// without one. A feature serves it from a route of its own, behind its own permission.
pub fn photo(store: &Store, dsp: &str, source: DriverSource, id: &str) -> Result<Option<Photo>> {
    registry().directory().map_or(Ok(None), |directory| {
        directory.photo(store, dsp, source, id)
    })
}
/// Has the DSP's people told apart again now, as a feature's people changed outside a
/// collection, as when a workspace's members are read: a collection's are after it ends.
pub async fn changed(state: Arc<State>, dsp: String) {
    if let Some(directory) = registry().directory() {
        directory.changed(state, dsp).await;
    }
}

impl Registry {
    /// Every kind of data that names people, in the order their spellings are read: by
    /// their order, those that shorten names last.
    pub fn people(&self) -> Vec<&'static People> {
        let mut people: Vec<&'static People> = self
            .features
            .iter()
            .flat_map(|feature| feature.people.iter().copied())
            .collect();
        people.sort_by_key(|people| people.reading_order());
        people
    }
    /// Who each ID is, as the one feature that tells people apart answers, if this build has
    /// it.
    pub fn directory(&self) -> Option<&'static dyn Directory> {
        self.features.iter().find_map(|feature| feature.directory)
    }
}

/// Panics unless each kind of data that names people has a place of its own, and one feature
/// at most answers who each ID is.
pub(crate) fn check(registry: &Registry) {
    let answering = registry
        .features
        .iter()
        .filter(|f| f.directory.is_some())
        .count();
    assert!(
        answering <= 1,
        "{answering} features answer who each ID is; one at most may"
    );
    let people = registry.people();
    for (index, kind) in people.iter().enumerate() {
        assert!(
            people[..index]
                .iter()
                .all(|other| other.kind.id != kind.kind.id && other.kind.order != kind.kind.order),
            "{} names people twice, or in another's place",
            kind.kind.id
        );
    }
}
