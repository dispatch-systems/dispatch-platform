//! The people slot: who each feature's data names, for Driver Match, which tells them apart.
//! A feature reads only its own storage; Driver Match joins what every feature names by ID.
use crate::{
    Result,
    db::{Db, Store},
    manifest::registry,
    text_enum,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::LazyLock};

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/manifest/api/generated/"))]
    #[derive(PartialOrd, Ord)]
    /// Where an ID comes from. Every Amazon source (routes, meal breaks, DVIC, the
    /// scorecard) knows a driver by the same transporter ID.
    pub enum DriverSource {
        Paycom => "paycom",
        Amazon => "amazon",
    }
}
/// A kind of collected data a person can appear in, as the feature whose data it is
/// declares it in its `people`.
pub struct PeopleData {
    /// Permanent: Driver Match's answers name it.
    pub id: &'static str,
    /// Its place in the one order the kinds are listed in everywhere.
    pub order: u16,
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
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/manifest/api/generated/"))]
    /// Where a person stands. `review` is a person who might be someone else listed twice;
    /// `former` someone only one source knows who has left.
    pub enum DriverStatus {
        Matched => "matched",
        Variant => "variant",
        Confirmed => "confirmed",
        Review => "review",
        PaycomOnly => "paycom_only",
        AmazonOnly => "amazon_only",
        Office => "office",
        Former => "former",
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
