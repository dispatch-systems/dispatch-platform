//! What only texts are answered with: a driver's workdays, Today's Routes, and whom the DSP can
//! text.
use super::{Conversation, Correspondent, Today};
use serde::Serialize;

/// A day a driver texted on a work phone: its day, YYYY-MM-DD in the DSP's own time, and the
/// route and van they drove, as Routes knows them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Workday {
    pub day: String,
    pub route: Option<String>,
    pub van: Option<String>,
}

/// `GET /api/dsp/communications/routes`: Today's Routes, while live route data holds today: each
/// driver on a route today, by name, with their route and work conversation; and each work phone
/// nobody held that texted today, as itself.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct TodaysRoutes {
    /// The live day, YYYY-MM-DD; none once live route data holds no read of today.
    pub day: Option<String>,
    pub drivers: Vec<RouteDriver>,
    pub phones: Vec<Conversation>,
}

/// A driver on a route today: who, by their ID of the source naming its drivers, their route as
/// live route data last read it, and their work conversation, if they have one.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct RouteDriver {
    pub driver_id: String,
    pub person: Correspondent,
    pub today: Today,
    pub conversation: Option<Conversation>,
}

/// `GET /api/dsp/communications/texts/contacts`: whom the DSP can text, by name, or those like
/// what was searched, the closest first: everyone Driver Match has a phone number for who hasn't
/// left, and every number the DSP saved a name for; then the numbers blocked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct TextContacts {
    pub contacts: Vec<TextContact>,
    pub blocked: Vec<TextContact>,
}

/// Someone the DSP can text, by their number.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct TextContact {
    pub number: String,
    pub person: Correspondent,
    /// Their number's conversation, if it has one.
    pub conversation_id: Option<String>,
    /// Since when it waits on an answer, and when it was last written in, as `Conversation`'s.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_at: Option<i64>,
    /// The letters of their name that matched what was searched, as `Conversation`'s.
    pub matched: Vec<(u32, u32)>,
}
