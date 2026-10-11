use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportTicket {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub progress: String,
    pub reporter_name: String,
    pub dsp_id: Option<String>,
    pub dsp_name: Option<String>,
    pub proposal_version: u32,
    pub approved_version: Option<u32>,
    pub pr_url: Option<String>,
    pub head_sha: Option<String>,
    pub merge_sha: Option<String>,
    pub released_version: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportMessage {
    pub id: String,
    pub author: String,
    pub role: String,
    pub body: String,
    pub private: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportAttachment {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub size: u32,
    pub private: bool,
}

/// Immutable analysis. Approval names this exact version and its acceptance criteria.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportProposal {
    pub version: u32,
    pub source_sha: String,
    pub reproduction: String,
    pub observations: Vec<String>,
    pub uncertainties: Vec<String>,
    pub analysis: String,
    pub plan: Vec<String>,
    pub alternatives: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub verification: Vec<String>,
    /// Read-only observations after a release; mutations are proven in the isolated preview.
    pub production_verification: Vec<String>,
    pub affected_areas: Vec<String>,
    pub risks: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportEvidence {
    pub id: String,
    pub phase: String,
    pub source_sha: String,
    pub criterion: String,
    pub steps: Vec<String>,
    pub outcome: String,
    pub passed: bool,
    pub attachment_ids: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub enum SupportIntakeOutcome {
    Acknowledged,
    Answered,
    UnclearBug,
    UnclearFeature,
}

/// Intake findings are private decision context, never implementation or browser proof.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportIntake {
    pub outcome: SupportIntakeOutcome,
    pub reply: String,
    pub reason: String,
    pub findings: Vec<String>,
    pub questions: Vec<String>,
    pub investigation_attempted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportDetail {
    pub ticket: SupportTicket,
    pub description: String,
    pub release: String,
    pub source_sha: String,
    pub messages: Vec<SupportMessage>,
    pub attachments: Vec<SupportAttachment>,
    pub proposals: Vec<SupportProposal>,
    pub evidence: Vec<SupportEvidence>,
    pub intake: Option<SupportIntake>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/support/api/generated/"))]
pub struct SupportNotification {
    pub id: String,
    pub ticket_id: String,
    pub body: String,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub read_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportJob {
    pub id: String,
    pub kind: String,
    pub version: u32,
    pub lease: String,
    pub lease_until: i64,
    pub checkpoint: serde_json::Value,
    pub question_id: Option<String>,
    pub environment: String,
    pub origin: String,
    pub detail: SupportDetail,
}
