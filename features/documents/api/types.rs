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

/// `GET /api/dsp/documents`: the DSP's Google connection, and whether it can make one.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsOverview {
    pub connection: Option<DocumentsConnection>,
    /// Whether this server can connect Google at all: it has a Google sign-in client.
    pub available: bool,
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

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
    /// What a file is: a folder, one of Google's own, or a file anyone uploaded.
    pub enum ItemKind {
        Folder => "folder", Doc => "doc", Sheet => "sheet", Slides => "slides", Pdf => "pdf",
        Image => "image", File => "file",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
    /// What Documents makes: a folder, or a Google Doc, Sheet or Slides.
    pub enum NewKind { Folder => "folder", Doc => "doc", Sheet => "sheet", Slides => "slides", }
}

/// A file or folder in the DSP's Documents.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsItem {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    /// Its bytes, for a file anyone uploaded; Google's own have none.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub size: Option<u64>,
    pub modified_at: String,
    /// Who changed it last, as Dispatch or Google names them.
    pub modified_by: Option<String>,
    /// Who added it through Dispatch.
    pub added_by: Option<String>,
    /// For a folder, how many files and folders it holds.
    pub items: Option<u32>,
    /// Where Google opens it.
    pub url: String,
    /// The folder it is in, for a search's results.
    pub location: Option<String>,
}

/// A step of the way to a folder.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct FolderStep {
    pub id: String,
    pub name: String,
}

/// `GET /api/dsp/documents/folder`: a folder of the DSP's Documents and what it holds, or what
/// a search inside it found.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsFolder {
    /// The folder; none for the main folder.
    pub id: Option<String>,
    pub name: String,
    /// Where Google Drive opens it.
    pub url: String,
    /// The folders from the main folder down to this one, this one last.
    pub path: Vec<FolderStep>,
    /// Its folders by name, then its files, the newest first.
    pub items: Vec<DocumentsItem>,
}
