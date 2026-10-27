//! What Phones' endpoints for the dashboard answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports. What its phones are answered with is theirs alone,
//! in `phone/`.
use dispatch_core::text_enum;
use serde::Serialize;

/// `GET /api/dsp/phones`: the DSP's phones, those removed left out, in the order paired.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/phones/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Phones {
    pub phones: Vec<PhoneRow>,
}

/// One of the DSP's phones.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/phones/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PhoneRow {
    pub id: String,
    pub name: String,
    /// Its own number, as it said when it paired; empty when it didn't know it.
    pub number: String,
    pub model: String,
    pub app_version: String,
    /// Whether it checked in in the last two minutes.
    pub online: bool,
    /// When it last checked in, epoch milliseconds.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_seen_at: Option<i64>,
}

/// `POST /api/dsp/phones/pairing`: the code that pairs the next phone, by its ID to ask after it,
/// its picture for the phone's camera, and when it expires, epoch milliseconds.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/phones/api/generated/"))]
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
    #[cfg_attr(feature = "ts", ts(export_to = "features/phones/api/generated/"))]
    /// How a pairing code stands: waiting for a phone, paired one, replaced by a newer code before
    /// it did, or expired, as one no longer kept is.
    pub enum PairingStatus {
        Waiting => "waiting",
        Paired => "paired",
        Replaced => "replaced",
        Expired => "expired",
    }
}

/// `GET /api/dsp/phones/pairing/{id}`: how the code the card shows stands.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/phones/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PhonePairingStatus {
    pub status: PairingStatus,
}
