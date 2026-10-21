use crate::{
    Result,
    collection::api::jobs::JobStatus,
    db::{FromRow, Row},
    manifest::registry,
    text_enum,
};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub provider: String,
    pub enabled: bool,
    pub status: ConnectionStatus,
    pub error: Option<String>,
    pub updated_at: String,
    pub last_verified_at: Option<String>,
    pub account_label: Option<String>,
    /// The browser session a member can take over, while one waits for them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub verification_session_id: Option<String>,
}
impl FromRow for Connection {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            provider: row.get("provider")?,
            enabled: row.get("enabled")?,
            status: row.get("status")?,
            error: row.get("error")?,
            updated_at: row.get("updated_at")?,
            last_verified_at: row.get("verified_at")?,
            account_label: row.get("account_label")?,
            verification_session_id: None,
        })
    }
}
/// One rule of a collection's schedule: on `days`, Monday 0 to Sunday 6, at each of `times`,
/// or every `every` minutes `from` `to`, `to` included; a `to` before `from` runs on past
/// midnight, and the same as `from` all day. `collects` is what it reads by: its collection's
/// own name, or one of its choice's. A new rule has no `id`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRule {
    pub id: Option<String>,
    pub collects: ScheduleCollection,
    pub kind: RuleKind,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub days: Vec<i64>,
    pub times: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub every: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// When it runs next, while its collection is on.
    pub next_run: Option<String>,
    /// Why its run waits, if it does.
    pub last_error: Option<String>,
}
/// What each rule of a collection's schedule chooses to read, under `label`, as Routes'
/// "Day".
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleChoice {
    pub label: String,
    pub options: Vec<ScheduleOption>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleOption {
    pub collection: ScheduleCollection,
    pub label: String,
}
/// How a collection's latest finished run ended, and when.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct LastRun {
    pub status: JobStatus,
    pub at: String,
    pub error: Option<String>,
}
/// One collection on Settings' Collectors page: what reads it, its schedule, and its runs.
/// `revision` is what a save or switch of its schedule is made against.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScheduledCollection {
    pub collection: ScheduleCollection,
    pub label: String,
    /// The feature that keeps it.
    pub feature: String,
    pub collector: String,
    pub collector_label: String,
    /// Whether its collector is connected.
    pub connected: bool,
    pub choice: Option<ScheduleChoice>,
    /// Whether its schedule is on: none of its rules runs while it's off.
    pub enabled: bool,
    pub rules: Vec<ScheduleRule>,
    pub revision: String,
    pub next_run: Option<String>,
    pub last_run: Option<LastRun>,
    /// Whether a run of it is queued or under way.
    pub running: bool,
    /// Whether the member may collect it now.
    pub can_collect: bool,
}
/// The collections whose schedules the member sets, in the DSP's `timezone`.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Collectors {
    pub timezone: String,
    pub collections: Vec<ScheduledCollection>,
}
/// The next runs of a collection's rules, unsaved.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRuns {
    pub runs: Vec<String>,
}

/// A bounded collection change hint. Missing history falls back to `all`.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct CollectionChange {
    pub provider: String,
    pub dates: Vec<String>,
    pub employee_code: Option<String>,
    pub roster: bool,
}
impl CollectionChange {
    pub fn all() -> Self {
        Self {
            provider: "all".into(),
            dates: vec![],
            employee_code: None,
            roster: true,
        }
    }
    pub fn provider(provider: &str) -> Self {
        Self {
            provider: provider.into(),
            ..Self::all()
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
pub struct CollectionUpdates {
    pub revision: String,
    pub changes: Vec<CollectionChange>,
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
        pub enum ConnectionStatus {
        NotConnected => "not_connected",
        Ready => "ready",
        SigningIn => "signing_in",
        NeedsVerification => "needs_verification",
        Error => "error",
    }
}
/// What a schedule collects: one provider's data, several at once, or one read another way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScheduleCollection(&'static str);
impl ScheduleCollection {
    /// Every collection a schedule may name, as the registry declares them, each alias, such
    /// as Timecard's `both`, right after the last of the collections it runs. Each is one a
    /// feature keeps, as the registry makes sure.
    pub fn all() -> impl Iterator<Item = Self> {
        registry().schedule_collections().into_iter().map(Self)
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|collection| collection.0 == text)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
    /// Every collection a schedule may name, as a JSON array for
    /// `collection IN (SELECT value FROM json_each(?))`. A schedule of any other is a newer
    /// release's, left by a rollback: this release lists, retimes and runs none of them,
    /// and keeps them for the release that knows them.
    pub fn known() -> crate::Result<String> {
        let collections: Vec<_> = Self::all().map(Self::as_str).collect();
        Ok(serde_json::to_string(&collections)?)
    }
}
impl Serialize for ScheduleCollection {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}
impl rusqlite::types::FromSql for ScheduleCollection {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value
            .as_str()
            .and_then(|text| Self::parse(text).ok_or(rusqlite::types::FromSqlError::InvalidType))
    }
}
impl rusqlite::types::ToSql for ScheduleCollection {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.0.into())
    }
}
// Its TypeScript is the union of the registry's collections, in their order.
#[cfg(feature = "ts")]
impl ts_rs::TS for ScheduleCollection {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    const IS_ENUM: bool = true;
    fn docs() -> Option<String> {
        Some(
            "/**\n * What a schedule collects: one provider's data, several at once, or \
             one read another way.\n */\n"
                .to_owned(),
        )
    }
    fn name(_: &ts_rs::Config) -> String {
        "ScheduleCollection".to_owned()
    }
    fn inline(_: &ts_rs::Config) -> String {
        let collections: Vec<_> = Self::all().map(|c| format!("{:?}", c.0)).collect();
        collections.join(" | ")
    }
    fn decl(cfg: &ts_rs::Config) -> String {
        format!("type ScheduleCollection = {};", Self::inline(cfg))
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        Some("core/collection/api/generated/ScheduleCollection.ts".into())
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
        pub enum RuleKind {
        Times => "times",
        Repeat => "repeat",
    }
}
