use crate::text_enum;
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// Which days a rule runs on: days of the week, or dates of the month.
    pub enum BackupOn {
        Week => "week",
        Month => "month",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// At what times of a day a rule runs: at set times, or every so often between two.
    pub enum BackupTiming {
        Times => "times",
        Repeat => "repeat",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// How long what a rule takes is kept: always, for a while, or as its newest few.
    pub enum KeepFor {
        Forever => "forever",
        For => "for",
        Last => "last",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
    /// What a rule's `keepCount` counts, when it keeps what it takes for a while.
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

/// One rule of a DSP's backup schedule: `on` days of the week, `days`, Monday 0 to Sunday 6, or
/// dates of the month, `dates`, 1 to 31, and its last day if `lastDay`; at each of `times`, or
/// every `every` minutes `from` `to`; what it takes kept `keep`: `forever`, `for` `keepCount`
/// `keepUnit`s, or its `last` `keepCount`; holding all the DSP's data but the items `skipped`.
/// A new rule has no `id`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BackupRule {
    pub id: Option<String>,
    pub on: BackupOn,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub days: Vec<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub dates: Vec<i64>,
    pub last_day: bool,
    pub kind: BackupTiming,
    pub times: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub every: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub keep: KeepFor,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub keep_count: Option<i64>,
    pub keep_unit: Option<KeepUnit>,
    pub skipped: Vec<String>,
    /// When it runs next, while the schedule is on.
    pub next_run: Option<String>,
}

/// A day of the coming weeks, where the DSP is, and the places of the rules that run on it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/backups/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct BackupDay {
    pub date: String,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub rules: Vec<usize>,
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
    /// The coming weeks, from today, and which rules run on each day.
    pub calendar: Vec<BackupDay>,
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
    pub calendar: Vec<BackupDay>,
}
