//! What Documents's endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
use dispatch_core::text_enum;
use serde::Serialize;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
    pub enum ConnectionStatus { Connected => "connected", Broken => "broken", }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
    /// A Google Workspace account, or anyone's own Google account, such as a Gmail address.
    pub enum AccountKind { Workspace => "workspace", Personal => "personal", }
}

/// `GET /api/dsp/documents`: the DSP's Google connection, and who can make one.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsOverview {
    pub connection: Option<DocumentsConnection>,
    /// Whether this server can connect Google at all: it has a Google sign-in client.
    pub available: bool,
    /// The DSP's owners by name, whom anyone else asks to connect or reconnect it.
    pub owners: Vec<String>,
}

/// The Google account that holds the DSP's Documents, and its main folder there.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsConnection {
    pub status: ConnectionStatus,
    pub account_email: String,
    pub account_kind: AccountKind,
    pub folder_name: String,
    pub folder_url: String,
    /// Who connected it, while their account exists.
    pub connected_by: Option<String>,
    pub connected_at: String,
}

/// `POST /api/dsp/documents/connect`: where the browser goes to sign in with Google.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct GoogleSignIn {
    pub url: String,
}
