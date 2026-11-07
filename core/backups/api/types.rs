use crate::{collection::api::types::RuleKind, text_enum};
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// What a rule's `keep` counts.
    pub enum KeepUnit {
        Day => "day",
        Week => "week",
        Month => "month",
        Year => "year",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// Where a backup is: waiting its turn, being taken, kept, or not taken.
    pub enum BackupStatus {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
    }
}

/// One rule of a DSP's backup schedule: on `days`, Monday 0 to Sunday 6, at each of `times`, or
/// every `every` minutes `from` `to`, as a collection's rules run; what it takes kept `keep`
/// `keepUnit`s, and holding all the DSP's data but the items `skipped`. A new rule has no `id`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BackupRule {
    pub id: Option<String>,
    pub kind: RuleKind,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub days: Vec<i64>,
    pub times: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub every: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub keep: i64,
    pub keep_unit: KeepUnit,
    pub skipped: Vec<String>,
    /// When it runs next, while the schedule is on.
    pub next_run: Option<String>,
}

/// One thing a DSP's backup can hold, as the owner chooses it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BackupItem {
    pub id: String,
    pub label: String,
    /// Whether every backup holds it.
    pub always: bool,
}

/// One of a DSP's backups: the rule that took it, none for one taken by hand.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub id: String,
    pub rule_id: Option<String>,
    pub status: BackupStatus,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub expires_at: Option<String>,
    /// How many bytes of the DSP's files it holds.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub size: Option<i64>,
    pub error: Option<String>,
}

/// A DSP's backups as its Backups tab shows them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DspBackups {
    /// Whether the server has somewhere to keep backups: none are taken without it.
    pub configured: bool,
    /// Whether its rules run.
    pub enabled: bool,
    /// Changes with each save, which names the schedule it changes.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    /// Where the DSP is, which its rules' times are read in.
    pub timezone: String,
    pub items: Vec<BackupItem>,
    pub rules: Vec<BackupRule>,
    /// Its next runs, while it's on.
    pub upcoming: Vec<String>,
    /// Newest first.
    pub backups: Vec<Backup>,
}

/// The next runs of a schedule being edited, before it's saved.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BackupRuns {
    pub upcoming: Vec<String>,
}
