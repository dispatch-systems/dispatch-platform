//! What managing the DSP's dispatch phones answers with, on Settings' DSP Connections.
use dispatch_core::text_enum;
use serde::Serialize;

/// `GET /api/dsp/communications/phones`: the DSP's phones, those removed left out, in the order
/// paired.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Phones {
    pub phones: Vec<PhoneRow>,
    /// The app they install, while the server hands one out at the DSP's own address.
    pub app: Option<PhoneApp>,
}

/// The app the DSP's phones install: its build's name and latest version, where a phone downloads
/// it at the DSP's address, and that address as a picture of a QR code, a `data:` address.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
pub struct PhoneApp {
    pub name: String,
    pub version: String,
    pub link: String,
    pub picture: String,
}

/// One of the DSP's phones.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneRow {
    pub id: String,
    pub name: String,
    /// Its own number, as it said when it paired; empty when it didn't know it.
    pub number: String,
    pub model: String,
    /// The app's version it runs, as it last said it.
    pub app_version: String,
    /// Whether that's older than the app the DSP's address hands out, so it has an update.
    pub update: bool,
    /// Whether it checked in in the last two minutes.
    pub online: bool,
    /// When it last checked in, epoch milliseconds.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_seen_at: Option<i64>,
    /// What needs attention on it, as it last said it stands; none from an app that never said.
    pub health: Option<PhoneHealth>,
}

/// What needs attention on a phone, as it last said it stands.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneHealth {
    /// Android doesn't let it send texts, or read them.
    pub cant_send: bool,
    pub cant_read: bool,
    pub no_sim: bool,
    /// Its SIM has no network to text on.
    pub no_signal: bool,
    /// Steps of its Set up are undone.
    pub setup_undone: bool,
    /// How many texts it read wait to go to Dispatch.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub waiting: i64,
    /// The picture messages it received that its texting app hasn't downloaded.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub undownloaded: i64,
}

/// `POST /api/dsp/communications/phones/pairing`: the code that pairs the next phone, by its ID to
/// ask after it, its picture for the phone's camera, and when it expires, epoch milliseconds.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct PhonePairing {
    pub id: String,
    pub code: String,
    pub picture: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// How a pairing code stands: waiting for a phone, paired one, replaced by a newer code before
    /// it did, or expired, as one no longer kept is.
    pub enum PairingStatus {
        Waiting => "waiting",
        Paired => "paired",
        Replaced => "replaced",
        Expired => "expired",
    }
}

/// `GET /api/dsp/communications/phones/pairing/{id}`: how the code the card shows stands.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct PhonePairingStatus {
    pub status: PairingStatus,
}
