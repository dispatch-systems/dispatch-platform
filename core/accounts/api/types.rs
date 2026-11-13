use crate::{
    Result,
    collection::api::types::ConnectionStatus,
    db::{FromRow, Row},
    foundation::config::{Environment, ProviderMode},
    server::api::types::Presence,
    tenancy::api::types::{DspStatus, OwnerStatus},
    text_enum,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PublicUser {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub platform_owner: bool,
}
impl PublicUser {
    pub fn name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}
impl FromRow for PublicUser {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            email: row.get("email")?,
            first_name: row.get("first_name")?,
            last_name: row.get("last_name")?,
            platform_owner: row.get("platform_owner")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Dsp {
    pub id: String,
    pub name: String,
    pub environment: Environment,
    pub status: DspStatus,
    pub timezone: String,
    pub permanent: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    pub created_at: String,
    /// Its short code, which names its address, in lowercase: none until it is set up.
    pub code: Option<String>,
}
impl FromRow for Dsp {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            environment: row.get("environment")?,
            status: row.get("status")?,
            timezone: row.get("timezone")?,
            permanent: row.get("permanent")?,
            revision: row.get("revision")?,
            created_at: row.get("created_at")?,
            code: row.get("code")?,
        })
    }
}
/// A DSP as the session lists it: who owns it, the caller's role, and its collection state.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DspSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub custom_features: Option<Vec<String>>,
    #[serde(flatten)]
    pub dsp: Dsp,
    pub profile: DspProfile,
    pub owner_email: Option<String>,
    pub owner_status: OwnerStatus,
    /// Every connection's state, by provider.
    pub connections: BTreeMap<String, ConnectionStatus>,
    pub last_collection: Option<String>,
    /// The next scheduled collection, if a schedule is enabled.
    pub next_collection: Option<String>,
    pub role: Option<String>,
    /// The features the DSP has (`features`).
    pub features: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub members: i64,
}
/// One queued email as platform Diagnostics lists it. What it was for is known only for
/// mail queued since the outbox recorded it; an invitation's row lasts as long as the
/// invitation does.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct MailMessage {
    pub id: String,
    /// `invitation`, `reset`, or none for older mail.
    pub kind: Option<String>,
    /// `pending`, `sent` or `failed`.
    pub status: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub attempts: i64,
    pub queued_at: Option<String>,
    pub sent_at: Option<String>,
    pub last_attempt_at: Option<String>,
    /// When a pending message is tried next.
    pub next_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub recipient: Option<String>,
    pub role: Option<String>,
    /// The invitation is for the DSP's owner, who also sets the DSP up.
    pub owner: bool,
    pub dsp_name: Option<String>,
    /// Who invited them; none when a platform owner did.
    pub invited_by: Option<String>,
    pub accepted_at: Option<String>,
    /// Owner invitations only: the DSP's setup is finished.
    pub setup_complete: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub id: String,
    pub user_id: String,
    pub dsp_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub role_id: Option<String>,
    pub owner: bool,
    pub status: Presence,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DspInvitation {
    pub email: String,
    pub role: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
    /// The list preserves SQLite's numeric 0/1 flag on the wire.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub accepted: i64,
}
impl FromRow for DspInvitation {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            email: row.get("email")?,
            role: row.get("role")?,
            expires_at: row.get("expiresAt")?,
            accepted: row.get("accepted")?,
        })
    }
}
/// A role as the team pages list it. The counts are only known to the list.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub owner: bool,
    pub permissions: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub members: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub invitations: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct RoleSummary {
    pub id: String,
    pub name: String,
    pub owner: bool,
}
/// How a member arranged the app's sidebar in a DSP: its pages by id in their order, and those
/// hidden. A page it doesn't name is listed where the app lists it; one named that the member
/// no longer has keeps its place.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(default)]
pub struct SidebarArrangement {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}
/// What opening a DSP answers with: the signed view token and what the role may do.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct DspView {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub custom: Option<CustomFeatures>,
    pub dsp: Dsp,
    pub role: RoleSummary,
    /// What the role may do, within the DSP's features.
    pub permissions: Vec<String>,
    /// The features the DSP has; a page of another does not exist here.
    pub features: Vec<String>,
    pub token: String,
    pub profile: DspProfile,
    /// Every role of the DSP, sent only to a platform owner so they can look through one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub roles: Option<Vec<RoleSummary>>,
    /// How the member arranged the sidebar here, once they have.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub sidebar: Option<SidebarArrangement>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub user: PublicUser,
    pub csrf: String,
    pub dsps: Vec<DspSummary>,
    /// Private platform slots and labels, sent only to a verified platform owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub platform_frontend: Option<String>,
    pub development: bool,
    pub environment: Environment,
    pub release: String,
    pub provider_mode: ProviderMode,
    pub source: RuntimeSource,
    pub security: SecurityStatus,
    /// Host-issued observation and support sessions can only inspect the dashboard.
    pub read_only: bool,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SecurityStatus {
    pub enrolled: bool,
    pub required: bool,
    pub verified: bool,
    pub recent: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub passkey_count: i64,
    pub authenticator: bool,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PasskeySummary {
    pub id: String,
    pub name: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}
impl FromRow for PasskeySummary {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            created_at: row.get("created_at")?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSetup {
    pub secret: String,
    pub qr_code: String,
}

/// What the running build was made from, so the dashboard can link its source.
#[derive(Clone, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSource {
    /// Set only on Production, which runs published releases.
    pub version: Option<String>,
    pub commit: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AccountSession {
    pub id: String,
    pub current: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
    pub device: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(default, rename_all = "camelCase")]
pub struct DspProfile {
    pub abbreviation: String,
    pub station_code: String,
    pub setup_required: bool,
    pub support_visible: bool,
    /// The platform owner hid the AI Support button from this DSP's members.
    pub ai_support_hidden: bool,
}
text_enum! {
    pub enum UserStatus {
        Active => "active",
        Disabled => "disabled",
    }
}

/// One DSP's code-built features; absent from every other DSP's view.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct CustomFeatures {
    pub catalog: Vec<CustomFeature>,
    pub permissions: Vec<CustomPermission>,
    pub frontends: Vec<CustomFrontend>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum CustomKind {
    Page,
    Sub,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct CustomFeature {
    pub id: String,
    pub label: String,
    pub place: u16,
    pub kind: CustomKind,
    pub permissions: Vec<String>,
    pub requires: Vec<String>,
    pub page: Option<String>,
    pub tab: bool,
    pub connects: bool,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct CustomPermission {
    pub id: String,
    pub label: String,
    pub group: Option<String>,
    pub implied: Vec<String>,
    pub under: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/accounts/api/generated/"))]
pub struct CustomFrontend {
    pub name: String,
    pub url: String,
    pub styles: Vec<String>,
    pub place: u16,
}
