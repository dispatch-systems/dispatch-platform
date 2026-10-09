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
    /// How the folder is shared with the member asking; none for the platform owner, who
    /// isn't one of the DSP's members.
    pub me: Option<MySharing>,
    /// For those who manage the DSP's connections, while Google is connected: how to add
    /// files someone made directly in Drive, signed in as its account. None when this server
    /// has no keys for Google's file picker.
    pub picker: Option<PickerSetup>,
}

/// How full a Google account's storage is.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriveStorage {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub used: u64,
    /// None for storage Google sets no limit on, as a Workspace's pooled storage.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub limit: Option<u64>,
}

/// `GET /api/dsp/documents/account`: the DSP's Google account, as Settings' DSP Connections
/// shows it to those who manage them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DocumentsAccount {
    pub connection: Option<DocumentsConnection>,
    /// Whether this server can connect Google at all: it has a Google sign-in client.
    pub available: bool,
    /// How full the account is, while connected; none when Google didn't say.
    pub storage: Option<DriveStorage>,
}

/// What adding files made directly in Google Drive needs.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PickerSetup {
    /// The Google account to pick them as: the one that holds the DSP's Documents.
    pub account: String,
    /// Google's keys for its file picker, which a browser uses as they are. None in fixture
    /// mode, which lists the files instead.
    pub google: Option<PickerKeys>,
    /// In fixture mode, the files made directly in its Drive.
    pub made_in_drive: Vec<DriveFile>,
}
/// The keys Google's file picker opens with, none of them secret: they name the platform's
/// Google project to Google.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PickerKeys {
    pub client_id: String,
    pub api_key: String,
    pub app_id: String,
}
/// A file in the account's Drive that Documents can't reach yet.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
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

/// `POST /api/dsp/documents/connect/finish`: the account just connected.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct GoogleConnected {
    pub account: DocumentsAccount,
    /// Whether Dispatch made the DSP's main folder just now, so Documents holds nothing yet
    /// and offers folders to start with.
    pub made_folder: bool,
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
    /// Where the page fetches Google's picture of what it holds, when Google made one.
    pub thumbnail: Option<String>,
    /// The extension its name keeps when renamed, such as `.pdf`: none for a folder, Google's
    /// own Docs, Sheets and Slides, or a name without one.
    pub extension: Option<String>,
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

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
    /// How sharing the main folder with a member went: Documents hasn't asked Google yet,
    /// they edit in Google, they need a Google account first, or Google refused for another
    /// reason.
    pub enum SharingState {
        Pending => "pending", Shared => "shared", NeedsAccount => "needs_account",
        Refused => "refused",
    }
}

/// How Documents shares the main folder with the member asking.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/documents/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct MySharing {
    pub state: SharingState,
    /// The address it is shared with, or would be.
    pub email: String,
    /// Whether that is a Google account they linked, rather than their Dispatch email.
    pub linked: bool,
}
