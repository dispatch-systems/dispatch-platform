//! The people slot: who each feature's data names, for Driver Match, which tells them apart.
//! A feature reads only its own storage; Driver Match joins what every feature names by ID.
//! The directory slot answers the other way round: who an ID is, for a feature showing people.
use crate::{
    Result,
    db::{Db, Store},
    manifest::registry,
};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

/// A source of the IDs people are known by, as the owner of those IDs declares it: Paycom's
/// employee codes, Amazon's transporter IDs. Each kind of data naming people by its IDs names
/// it as its source.
pub struct SourceData {
    /// Permanent: Driver Match keeps its people's IDs under it.
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
    /// Permanent: Driver Match's answers name it.
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
/// One kind of a feature's data that names people, as Driver Match reads it.
pub trait People: Sync {
    /// The kind of data it is, as a person's activity lists it.
    fn data(&self) -> DriverData;
    /// The source whose IDs it names people by.
    fn source(&self) -> DriverSource;
    /// Where its spelling stands among every kind's that names the same ID: the lowest
    /// first. Amazon's sources each write a driver's name their own way.
    fn order(&self) -> u16;
    /// Every ID it names, once for each way it writes the name, in the order its names
    /// should be read.
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>>;
    /// The name it writes for one ID now, if any.
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>>;
    /// How much it holds of each ID it names.
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>>;
    /// The days it says each ID worked, and the days it was collected for.
    fn workdays(&self, _: &Store, _dsp: &str) -> Result<Workdays> {
        Ok(Workdays::default())
    }
    /// The departments whose people the DSP counts as drivers, as its feature's settings
    /// say: anyone it names outside them works in the office. `None` when they say nothing.
    fn driver_departments(&self, _: &Store, _dsp: &str) -> Result<Option<BTreeSet<String>>> {
        Ok(None)
    }
}

/// An ID as one kind of data names it.
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
}
impl Named {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ..Self::default()
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

/// How much one kind of data holds of an ID: on how many days or weeks, and the last.
#[derive(Clone, Debug)]
pub struct Appearances {
    pub id: String,
    pub count: usize,
    pub last: String,
}

/// When one kind of data saw people at work.
#[derive(Debug, Default)]
pub struct Workdays {
    /// Each ID with a day it worked.
    pub worked: Vec<(String, String)>,
    /// The days it was collected for, so that a day outside them says nothing either way.
    pub collected: Vec<String>,
}

/// The first name `sql` finds for `id` that is more than spaces, its spaces made single.
pub fn first_name(db: &Db, sql: &str, id: &str) -> Result<Option<String>> {
    Ok(db
        .query_as::<(Option<String>,)>(sql, [id])?
        .into_iter()
        .filter_map(|(name,)| name)
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|name| !name.is_empty()))
}

/// Who an ID is, as the one feature that tells people apart answers, for a feature that shows
/// people its data names.
pub trait Directory: Sync {
    /// Every ID of `from` the DSP's people hold, each with its person: their code, their IDs
    /// of `to`, and whether someone confirmed who they are. Empty while the feature answering
    /// is off for the DSP.
    fn links(
        &self,
        store: &Store,
        dsp: &str,
        from: DriverSource,
        to: DriverSource,
    ) -> Result<BTreeMap<String, Link>>;
}
/// One person, as the directory answers for one of their IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub code: String,
    /// Their IDs of the source asked about.
    pub ids: Vec<String>,
    /// Whether someone confirmed who they are, rather than a match.
    pub confirmed: bool,
}
