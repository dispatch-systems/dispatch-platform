//! Google Drive, as Documents uses it with the DSP's account: every file Dispatch can reach,
//! and making, renaming and trashing them. `drive.file` lets Dispatch reach only the files it
//! made or was given, so listing them all is listing Documents, plus anything another DSP made
//! with the same account, which the tree leaves out. Fixture mode keeps a Drive of its own in
//! memory for each account, as Google keeps an account's files for every DSP that connects it.
use super::google::{FILES, FOLDER, Google, HTTP, send};
use dispatch_core::{
    Error, Result,
    db::iso,
    foundation::crypto,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};

pub const DOC: &str = "application/vnd.google-apps.document";
pub const SHEET: &str = "application/vnd.google-apps.spreadsheet";
pub const SLIDES: &str = "application/vnd.google-apps.presentation";
/// What Documents asks of each file.
const FIELDS: &str =
    "id,name,mimeType,parents,modifiedTime,lastModifyingUser(displayName,emailAddress),size,webViewLink";
/// Enough pages for 50,000 files; a Drive that keeps answering past them is not listed whole.
const PAGES: usize = 50;

/// A file or folder, as Drive describes it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    #[serde(default)]
    pub parents: Vec<String>,
    pub modified_time: String,
    pub last_modifying_user: Option<Person>,
    /// Its bytes, which Drive writes as text; Google's own Docs, Sheets and Slides have none.
    pub size: Option<String>,
    pub web_view_link: String,
}
impl Item {
    pub fn folder(&self) -> bool {
        self.mime_type == FOLDER
    }
}
/// Whoever last changed a file, as Google names them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub display_name: Option<String>,
    pub email_address: Option<String>,
}

impl Google {
    /// Every file and folder Dispatch can reach in the account, out of the trash.
    pub async fn list(&self, access: &str) -> Result<Vec<Item>> {
        if let Self::Fixture = self {
            return fixture(access, |drive| Ok(drive.values().cloned().collect()));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            files: Vec<Item>,
            next_page_token: Option<String>,
        }
        let mut items = Vec::new();
        let mut next: Option<String> = None;
        for _ in 0..PAGES {
            let mut query = vec![
                ("q", "trashed=false".to_owned()),
                ("fields", format!("nextPageToken,files({FIELDS})")),
                ("pageSize", "1000".to_owned()),
                ("spaces", "drive".to_owned()),
            ];
            query.extend(next.take().map(|token| ("pageToken", token)));
            let page: Page = send(HTTP.get(FILES).bearer_auth(access).query(&query)).await?;
            items.extend(page.files);
            match page.next_page_token {
                Some(token) => next = Some(token),
                None => return Ok(items),
            }
        }
        Err(Error::new("documents_too_many_files", 507))
    }
    /// Makes a file of `mime`, a folder or one of Google's own, in `parent`, or at the top of
    /// the account's Drive.
    pub async fn create(
        &self,
        access: &str,
        name: &str,
        mime: &str,
        parent: Option<&str>,
    ) -> Result<Item> {
        if let Self::Fixture = self {
            let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
            return fixture(access, |drive| {
                let account = drive.account.clone();
                let item = Item {
                    web_view_link: link(mime, &id),
                    id: id.clone(),
                    name: name.to_owned(),
                    mime_type: mime.to_owned(),
                    parents: parent.map(str::to_owned).into_iter().collect(),
                    modified_time: iso(),
                    last_modifying_user: Some(Person {
                        display_name: Some(account.clone()),
                        email_address: Some(account),
                    }),
                    size: None,
                };
                drive.files.insert(id.clone(), item.clone());
                Ok(item)
            });
        }
        let parents: Vec<&str> = parent.into_iter().collect();
        send(
            HTTP.post(FILES)
                .bearer_auth(access)
                .query(&[("fields", FIELDS)])
                .json(&json!({"name": name, "mimeType": mime, "parents": parents})),
        )
        .await
    }
    /// Gives a file another name.
    pub async fn rename(&self, access: &str, id: &str, name: &str) -> Result<Item> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let item = drive.files.get_mut(id).ok_or_else(missing)?;
                item.name = name.to_owned();
                item.modified_time = iso();
                Ok(item.clone())
            });
        }
        send(
            HTTP.patch(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS)])
                .json(&json!({"name": name})),
        )
        .await
    }
    /// Moves a file to the account's trash, where Drive keeps it for 30 days.
    pub async fn trash(&self, access: &str, id: &str) -> Result<()> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                drive.files.remove(id).map(drop).ok_or_else(missing)
            });
        }
        let _: Item = send(
            HTTP.patch(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS)])
                .json(&json!({"trashed": true})),
        )
        .await?;
        Ok(())
    }
}

fn missing() -> Error {
    Error::new("documents_item_not_found", 404)
}

/// Where Google opens a file of `mime`.
fn link(mime: &str, id: &str) -> String {
    match mime {
        FOLDER => format!("https://drive.google.com/drive/folders/{id}"),
        DOC => format!("https://docs.google.com/document/d/{id}/edit"),
        SHEET => format!("https://docs.google.com/spreadsheets/d/{id}/edit"),
        SLIDES => format!("https://docs.google.com/presentation/d/{id}/edit"),
        _ => format!("https://drive.google.com/file/d/{id}/view"),
    }
}

/// One account's Drive, in fixture mode.
struct Drive {
    account: String,
    files: BTreeMap<String, Item>,
}
impl Drive {
    fn values(&self) -> impl Iterator<Item = &Item> {
        self.files.values()
    }
}
static DRIVES: LazyLock<Mutex<BTreeMap<String, Drive>>> = LazyLock::new(Default::default);
/// Runs `work` on the Drive of the account fixture mode's access token names.
fn fixture<T>(access: &str, work: impl FnOnce(&mut Drive) -> Result<T>) -> Result<T> {
    let account = access
        .strip_prefix("fixture:")
        .ok_or_else(|| Error::new("google_connection_broken", 409))?;
    let mut drives = DRIVES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let drive = drives.entry(account.to_owned()).or_insert_with(|| Drive {
        account: account.to_owned(),
        files: BTreeMap::new(),
    });
    work(drive)
}
/// Whether fixture mode's Drive has the file, out of the trash.
pub fn fixture_has(access: &str, id: &str) -> Result<bool> {
    fixture(access, |drive| Ok(drive.files.contains_key(id)))
}
