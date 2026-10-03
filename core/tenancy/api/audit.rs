use crate::{manifest::registry, text_enum};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    pub enum AuditArea {
        Team => "team", Roles => "roles", Collections => "collections", Schedules => "schedules",
        Connections => "connections", Access => "access", Dsps => "dsps", Settings => "settings",
    }
}
/// The kind of record an event is about, as its reference names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuditSubject(&'static str);
impl AuditSubject {
    /// Core's own kinds, before those the features declare.
    const CORE: &[&str] = &["member", "role", "schedule", "job"];
    /// Every kind: core's, then each feature's, in the registry's order.
    pub fn all() -> impl Iterator<Item = Self> {
        let features = registry().features.iter().flat_map(|f| f.audit.subjects);
        Self::CORE.iter().chain(features).map(|kind| Self(kind))
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|subject| subject.0 == text)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}
impl Serialize for AuditSubject {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}
impl<'de> Deserialize<'de> for AuditSubject {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let kind = String::deserialize(deserializer)?;
        Self::parse(&kind).ok_or_else(|| serde::de::Error::custom("unknown audit subject"))
    }
}
// Its TypeScript is the union of the registry's kinds, in their order.
#[cfg(test)]
impl ts_rs::TS for AuditSubject {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    const IS_ENUM: bool = true;
    fn name(_: &ts_rs::Config) -> String {
        "AuditSubject".to_owned()
    }
    fn inline(_: &ts_rs::Config) -> String {
        let kinds: Vec<_> = Self::all().map(|kind| format!("{:?}", kind.0)).collect();
        kinds.join(" | ")
    }
    fn decl(cfg: &ts_rs::Config) -> String {
        format!("type AuditSubject = {};", Self::inline(cfg))
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("AuditSubject.ts".into())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AuditReference {
    pub kind: AuditSubject,
    pub id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AuditChange {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    #[cfg_attr(test, ts(type = "number"))]
    pub id: i64,
    pub at: String,
    pub actor_id: Option<String>,
    pub actor_name: String,
    pub dsp_id: Option<String>,
    pub dsp_name: Option<String>,
    pub action: String,
    pub detail: String,
    pub area: AuditArea,
    pub target: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<AuditReference>,
    pub changes: Vec<AuditChange>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AuditName {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AuditPage {
    pub events: Vec<AuditEvent>,
    #[cfg_attr(test, ts(type = "number"))]
    pub total: i64,
    pub counts: BTreeMap<AuditCount, usize>,
    pub actors: Vec<AuditName>,
    pub dsps: Vec<AuditName>,
}

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    #[derive(PartialOrd, Ord)]
    pub enum AuditCount {
        Team => "team", Roles => "roles", Collections => "collections", Schedules => "schedules",
        Connections => "connections", Access => "access", Dsps => "dsps", Settings => "settings", Failures => "failures",
    }
}
