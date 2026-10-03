use crate::{
    contracts::{DriverSource, DriverStatus},
    text_enum,
};
use serde::Serialize;

text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// How an ID came to its person.
    pub enum DriverLink {
        New => "new",
        Name => "name",
        Variant => "variant",
        Saved => "saved",
        Person => "person",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    #[derive(PartialOrd, Ord)]
    /// The collected data a person can appear in.
    pub enum DriverData {
        Timecards => "timecards",
        Routes => "routes",
        MealBreaks => "meal_breaks",
        Dvic => "dvic",
        Scorecard => "scorecard",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    pub enum DriverStrength {
        Strong => "strong",
        Possible => "possible",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    /// Why two people might be one.
    pub enum DriverEvidenceKind {
        SameLastName => "same_last_name",
        SameFirstName => "same_first_name",
        ShortForm => "short_form",
        LastInitial => "last_initial",
        WorkedDays => "worked_days",
    }
}
text_enum! {
    #[cfg_attr(test, derive(ts_rs::TS))]
    pub enum DriverEventKind {
        Added => "added",
        Linked => "linked",
        Merged => "merged",
        Split => "split",
        Apart => "apart",
    }
}

/// One ID a person is known by, as its source wrote it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverId {
    pub source: DriverSource,
    pub id: String,
    pub name: String,
    pub department: Option<String>,
    pub position: Option<String>,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub linked_by: DriverLink,
    pub linked_at: String,
}
/// A person: their code, every ID they hold and the data they appear in.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Driver {
    pub code: String,
    pub name: String,
    pub status: DriverStatus,
    pub ids: Vec<DriverId>,
    pub appears: Vec<DriverData>,
    pub last_seen: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverCounts {
    /// Everyone with a code, office staff and those who have left included.
    pub all: usize,
    /// Everyone else.
    pub drivers: usize,
    /// Drivers known to both Paycom and Amazon.
    pub matched: usize,
    /// Pairs that might be one person.
    pub review: usize,
    pub paycom_only: usize,
    pub amazon_only: usize,
    pub office: usize,
    /// People only one source knows who have left: off Paycom's roster and unseen lately.
    pub former: usize,
}
/// One reason two people might be one. `short` and `long` name a short form and the
/// name it stands for; `same` of `of` days count days both sides worked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverEvidence {
    pub kind: DriverEvidenceKind,
    pub short: Option<String>,
    pub long: Option<String>,
    pub same: Option<usize>,
    pub of: Option<usize>,
}
/// Someone known only to Paycom and someone known only to Amazon who might be one person.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverPair {
    pub paycom: Driver,
    pub amazon: Driver,
    pub strength: DriverStrength,
    pub evidence: Vec<DriverEvidence>,
}
/// The Driver Match tab: the pairs to decide, then everyone.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverMatch {
    /// When collected data was last checked for new IDs.
    pub checked_at: Option<String>,
    pub counts: DriverCounts,
    pub review: Vec<DriverPair>,
    pub drivers: Vec<Driver>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverActivity {
    pub data: DriverData,
    /// Days for timecards, routes and meal breaks; inspections for DVIC; weeks for the
    /// scorecard.
    pub count: usize,
}
/// One day of the last fourteen. `None` where nothing was collected for that side.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverDay {
    pub date: String,
    pub clocked_in: Option<bool>,
    pub drove: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverEvent {
    pub at: String,
    pub kind: DriverEventKind,
    pub source: Option<DriverSource>,
    pub link: Option<DriverLink>,
    /// The ID's name for `added` and `linked`; the other person's for the rest.
    pub name: Option<String>,
    pub code: Option<String>,
    pub actor: Option<String>,
}
/// One person in full, for the details panel.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverDetails {
    pub driver: Driver,
    pub activity: Vec<DriverActivity>,
    pub days: Vec<DriverDay>,
    pub history: Vec<DriverEvent>,
    /// Codes merged into this person, which now lead here.
    pub merged_codes: Vec<String>,
}
