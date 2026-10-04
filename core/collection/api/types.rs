use crate::{
    Result,
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
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct CollectionSchedule {
    pub id: String,
    pub name: String,
    pub collection: ScheduleCollection,
    pub cadence: Cadence,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub interval_minutes: Option<i64>,
    pub local_time: String,
    pub enabled: bool,
    pub next_run: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    pub last_error: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct CollectionSchedules {
    pub timezone: String,
    pub dsp_name: String,
    pub schedules: Vec<CollectionSchedule>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/collection/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SchedulePreview {
    pub next_run: String,
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
/// What a schedule collects: one provider's data, or every scheduled provider's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScheduleCollection(&'static str);
impl ScheduleCollection {
    /// Every collection a schedule may name, as the registry declares them: the
    /// collections a feature's alias runs, such as Timecard's `both`, the aliases, then the
    /// others. Each is one a feature keeps, as the registry makes sure.
    pub fn all() -> impl Iterator<Item = Self> {
        registry().schedule_collections().into_iter().map(Self)
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|collection| collection.0 == text)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
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
            "/**\n * What a schedule collects: one provider's data, or every scheduled \
             provider's.\n */\n"
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
        pub enum Cadence {
        Interval => "interval",
        Daily => "daily",
    }
}
