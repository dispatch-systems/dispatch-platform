pub use crate::tenancy::visibility::Visibility;
use crate::{
    Result,
    collection::api::jobs::JobStatus,
    db::{FromRow, Row},
    foundation::config::{Environment, ProviderMode},
    tenancy::{api::types::DspStatus, switches::SwitchedFeatures, visibility::FeatureVisibility},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BrowserAdmission {
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub available_bytes: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub required_bytes: u64,
    pub can_start: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct BrowserHealth {
    pub active: usize,
    pub capacity: usize,
    pub memory: BrowserAdmission,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TransportHealth {
    pub error: Option<String>,
    pub checked_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct MailHealth {
    pub enabled: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pending: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub failed: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub oldest_pending_age_ms: Option<i64>,
    pub last_success_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub transport: TransportHealth,
}
/// One feature switched for a DSP, by the platform owner or by a dependency.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct FeatureChange {
    pub feature: String,
    pub enabled: bool,
}
/// One feature of a DSP: whether it has it, who among its people sees it, and who last changed
/// either and when.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct FeatureState {
    pub feature: String,
    pub enabled: bool,
    pub visibility: Visibility,
    pub changed_at: Option<String>,
    pub changed_by: Option<String>,
}
/// A DSP's features, with its schedules switched on and its jobs under way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DspFeatureReport {
    pub features: Vec<FeatureState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub custom: Option<Vec<crate::accounts::api::types::CustomFeature>>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub schedules: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub active_jobs: i64,
}
/// What a DSP's members don't all see of its features, after one was shown, put in beta or
/// hidden: what is hidden from them, and what only its beta testers see.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct DspVisibility {
    pub hidden: Vec<String>,
    pub beta: Vec<String>,
}
/// One of a DSP's people, as its Beta testers tab lists them: their role, and since when they
/// have tested what is in beta there, if they do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BetaMember {
    /// Their account's ID.
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: String,
    pub tester_since: Option<String>,
}
/// A DSP's people, its beta testers among them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct DspBetaTesters {
    pub members: Vec<BetaMember>,
}
/// A DSP's features after a switch: what it has now, and what the switch changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct DspFeatures {
    pub features: Vec<String>,
    pub changed: Vec<FeatureChange>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PlatformHealth {
    pub environment: Environment,
    pub release: String,
    pub jobs: BTreeMap<JobStatus, u32>,
    pub browsers: BrowserHealth,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dsps: i64,
    pub email: bool,
    pub mail: MailHealth,
    pub provider_mode: ProviderMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PlatformDiagnostics {
    pub enabled: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub storage_available_bytes: u64,
    pub runtime: DiagnosticsRuntime,
    pub dsps: Vec<DiagnosticsDsp>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRuntime {
    pub name: String,
    pub status: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub memory_bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub core_memory_bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub worker_memory_bytes: u64,
    pub browsers: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/platform_owner/api/generated/"))]
pub struct DiagnosticsDsp {
    pub id: String,
    pub name: String,
    pub status: DspStatus,
}
impl FromRow for DiagnosticsDsp {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            status: row.get("status")?,
        })
    }
}

impl From<SwitchedFeatures> for DspFeatures {
    fn from(outcome: SwitchedFeatures) -> Self {
        Self {
            features: outcome.features,
            changed: outcome
                .changed
                .into_iter()
                .map(|(feature, enabled)| FeatureChange {
                    feature: feature.id.to_owned(),
                    enabled,
                })
                .collect(),
        }
    }
}
impl From<FeatureVisibility> for DspVisibility {
    fn from(visibility: FeatureVisibility) -> Self {
        Self {
            hidden: visibility.hidden,
            beta: visibility.beta,
        }
    }
}
