//! What a DSP's Settings is drawn from, and what saving a section sends and answers.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Every section a member may see, with the pages they're on.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsAnswer {
    /// The pages with one of them at least: by group, then by their place, then by id.
    pub pages: Vec<SettingsPage>,
    /// By their pages' order, then by their place on it, then by id.
    pub sections: Vec<SettingsSection>,
}
/// A page of Settings: one of the shared pages, or a feature's own.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsPage {
    /// Permanent: links name it (`?tab=`).
    pub id: String,
    pub label: String,
    pub group: SettingsGroup,
    /// Its place in its group, lowest first.
    pub order: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum SettingsGroup {
    Personal,
    Dsp,
    Features,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum SettingsScope {
    /// One value for the DSP.
    Dsp,
    /// The member's own.
    Member,
}
/// A section as the member sees it: its fields and their values now.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsSection {
    pub id: String,
    /// The page it's on, by id.
    pub page: String,
    pub label: String,
    pub order: u16,
    pub scope: SettingsScope,
    /// Drawn by its owner's component, which the frontend registers under its id.
    pub custom: bool,
    /// The collection it goes with, by its schedule name: the Collectors page draws it with
    /// that collection.
    pub collection: Option<String>,
    pub fields: Vec<SettingsField>,
    /// Every field's value by id.
    #[cfg_attr(
        feature = "ts",
        ts(type = "Record<string, boolean | number | string | Array<string> | null>")
    )]
    pub values: Map<String, Value>,
    /// What a save sends back: 0 until one is saved.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    /// The collectors whose collections its feature keeps, by their labels, for search.
    pub sources: Vec<String>,
    /// More words search finds it by.
    pub keywords: Vec<String>,
}
/// One field of a section, with what its kind offers.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsField {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: SettingsKind,
}
/// What a field holds, as its value is drawn and sent.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsKind {
    /// `true` or `false`.
    Toggle,
    /// One of its options' values.
    Choice { options: Vec<SettingsOption> },
    /// One of the options listed as it's read, or `null` when it may be none, which reads as
    /// `none`.
    Pick {
        options: Vec<SettingsOption>,
        none: Option<String>,
    },
    /// Several of its options' values, kept in their order, or in the member's when
    /// `ordered`.
    Many {
        options: Vec<SettingsOption>,
        ordered: bool,
    },
    /// A whole number from `min` to `max`, or `null` when it may be none, which reads as
    /// `none`; `presets` are offered beside any other.
    Number {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        min: i64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        max: i64,
        unit: Option<String>,
        presets: Vec<SettingsPreset>,
        none: Option<String>,
    },
    /// `min` to `max` characters, or empty while it's unset. A code is uppercase letters and
    /// digits; a locked one can't change once set.
    Text {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        min: usize,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        max: usize,
        code: bool,
        locked: bool,
    },
    /// `HH:MM`.
    Time,
    /// An IANA timezone's name.
    Timezone,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsOption {
    pub value: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
pub struct SettingsPreset {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub value: i64,
    pub label: String,
}

/// Saving a section: any of its fields' values, as read at `revision`.
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
#[serde(deny_unknown_fields)]
pub struct SettingsSave {
    #[cfg_attr(
        feature = "ts",
        ts(type = "Record<string, boolean | number | string | Array<string> | null>")
    )]
    pub values: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    /// Sent again once the member has confirmed what its warning said.
    #[cfg_attr(feature = "ts", ts(optional))]
    pub confirmed: Option<bool>,
}
/// What a save answers: the section as it is now, or, keeping nothing, the words to confirm.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "core/settings/api/generated/"))]
#[serde(untagged)]
pub enum SettingsSaved {
    Section {
        section: Box<SettingsSection>,
        /// Whether the save changed the DSP itself, as its name or timezone, which the member's
        /// view of it holds: the view is to be opened again.
        reopen: bool,
    },
    Warning {
        warning: String,
    },
}
