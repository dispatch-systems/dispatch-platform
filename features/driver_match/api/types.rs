//! What Driver Match's endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
use dispatch_core::{
    manifest::people::{DriverData, DriverSource},
    text_enum,
};
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// How an ID came to its person, surest last: as its first ID, by a name variant, a
    /// unique name, a phone number or an email address only they share, as another account
    /// its source says is theirs, a link saved on the meal-break page, or someone's decision.
    pub enum DriverLink {
        New => "new",
        Variant => "variant",
        Name => "name",
        Phone => "phone",
        Email => "email",
        Account => "account",
        Saved => "saved",
        Person => "person",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    pub enum DriverStrength {
        Strong => "strong",
        Possible => "possible",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// Why two people might be one.
    pub enum DriverEvidenceKind {
        SameEmail => "same_email",
        SamePhone => "same_phone",
        SameLastName => "same_last_name",
        SameFirstName => "same_first_name",
        ShortForm => "short_form",
        LastInitial => "last_initial",
        WorkedDays => "worked_days",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    pub enum DriverEventKind {
        Added => "added",
        Linked => "linked",
        Merged => "merged",
        Split => "split",
        Apart => "apart",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
    /// What a person does for the DSP: a driver or a helper, as Amazon's roster says or their
    /// routes show; office staff, outside the departments the DSP counts as drivers; or
    /// someone who has left, whom no source lists and nothing has seen lately.
    pub enum DriverRole {
        Driver => "driver",
        Helper => "helper",
        Office => "office",
        Former => "former",
    }
}

/// One ID a person is known by, as its source wrote it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverId {
    pub source: DriverSource,
    pub id: String,
    pub name: String,
    pub department: Option<String>,
    pub position: Option<String>,
    /// The email addresses and phone numbers its source has for them.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    /// What its source says they do: `Driver`, `Helper`.
    pub roles: Vec<String>,
    /// On its source's current roster.
    pub listed: bool,
    /// Whether a photo of them is held under this ID.
    pub photo: bool,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub linked_by: DriverLink,
    pub linked_at: String,
}
/// A person: their code, every ID they hold and the data they appear in.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Driver {
    pub code: String,
    pub name: String,
    pub role: DriverRole,
    /// How surely their IDs are one person's: the least sure link among them, `new` while
    /// they hold one ID.
    pub link: DriverLink,
    /// Whether they are in a pair waiting for a decision.
    pub review: bool,
    pub ids: Vec<DriverId>,
    pub appears: Vec<DriverData>,
    pub last_seen: Option<String>,
    /// Their photo's version, when a feature holds one: its address carries it.
    pub photo: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverCounts {
    /// Everyone with a code.
    pub people: usize,
    /// Pairs that might be one person, neither of whom has left.
    pub review: usize,
    /// IDs a name variant joined to someone who has not left, which nobody has confirmed.
    pub unconfirmed: usize,
    /// Drivers and helpers some source the DSP has does not know.
    pub missing: usize,
    pub drivers: usize,
    pub helpers: usize,
    pub office: usize,
    pub former: usize,
}
/// One reason two people might be one. `short` and `long` name a short form and the name it
/// stands for; `same` of `of` days count the days both worked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverEvidence {
    pub kind: DriverEvidenceKind,
    pub short: Option<String>,
    pub long: Option<String>,
    pub same: Option<usize>,
    pub of: Option<usize>,
}
/// Someone who might be someone else listed twice: `person` holds what `candidate` lacks, and
/// joining them keeps `candidate`'s code.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverPair {
    pub person: Driver,
    pub candidate: Driver,
    pub strength: DriverStrength,
    pub evidence: Vec<DriverEvidence>,
}
/// The Driver Match tab: the sources the DSP has, the pairs to decide, then everyone.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverMatch {
    /// When collected data was last checked for new IDs.
    pub checked_at: Option<String>,
    /// The sources of IDs the DSP's features have on, in their order.
    pub sources: Vec<DriverSource>,
    pub counts: DriverCounts,
    pub review: Vec<DriverPair>,
    pub drivers: Vec<Driver>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverActivity {
    pub data: DriverData,
    /// Days for timecards, routes and meal breaks; inspections for DVIC; weeks for the
    /// scorecard.
    pub count: usize,
}
/// One source's last fourteen days of a person's work: for each day, whether they worked,
/// `None` where nothing was collected.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverDays {
    pub source: DriverSource,
    pub worked: Vec<Option<bool>>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/driver_match/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriverDetails {
    pub driver: Driver,
    pub activity: Vec<DriverActivity>,
    /// The last fourteen days, oldest first.
    pub dates: Vec<String>,
    /// What each source that saw them work says of those days.
    pub days: Vec<DriverDays>,
    pub history: Vec<DriverEvent>,
    /// Codes merged into this person, which now lead here.
    pub merged_codes: Vec<String>,
}
