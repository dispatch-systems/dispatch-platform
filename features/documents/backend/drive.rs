//! Google Drive, as Documents uses it with the DSP's account: every file Dispatch can reach,
//! and making, uploading, downloading, renaming and trashing them. `drive.file` lets Dispatch reach only the files it
//! made or was given, so listing them all is listing Documents, plus anything another DSP made
//! with the same account, which the tree leaves out. Fixture mode keeps a Drive of its own in
//! memory for each account, as Google keeps an account's files for every DSP that connects it.
use super::google::{FILES, FOLDER, Google, HTTP, read, send, unreachable};
use axum::body::Body;
use axum::body::Bytes;
use dispatch_core::{Error, Result, db::iso, foundation::crypto, server::http::upload::Upload};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{LazyLock, Mutex},
    time::Duration,
};

pub const DOC: &str = "application/vnd.google-apps.document";
pub const SHEET: &str = "application/vnd.google-apps.spreadsheet";
pub const SLIDES: &str = "application/vnd.google-apps.presentation";
/// What Documents asks of each file.
const FIELDS: &str = "id,name,mimeType,parents,modifiedTime,lastModifyingUser(displayName,emailAddress),size,webViewLink,thumbnailLink,thumbnailVersion";
/// Enough pages for 50,000 files; a Drive that keeps answering past them is not listed whole.
const PAGES: usize = 50;
/// Where a file's bytes go up.
const UPLOADS: &str = "https://www.googleapis.com/upload/drive/v3/files";
/// The longest side of a card's picture, in pixels: sharp on a high-density screen.
const PICTURE_SIZE: u32 = 600;
/// The largest picture Dispatch hands on; Google's are a small part of it.
const PICTURE_LIMIT: usize = 2 * 1024 * 1024;
/// How many pictures the server fetches from Google at once, across every DSP.
static PICTURES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
/// The Office files Google's own Docs, Sheets and Slides download as, with their endings.
const DOCX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "docx",
);
const XLSX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "xlsx",
);
const PPTX: (&str, &str) = (
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "pptx",
);

/// Moves a file's bytes, which takes as long as they do: it gives up only on a Google that
/// stops answering for a minute.
static TRANSFER: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("the TLS backend is built in")
});

/// A file to save: its content type, the name it's saved as, its length when Google says,
/// and its bytes as they come.
pub struct Download {
    pub kind: String,
    pub name: String,
    pub length: Option<u64>,
    pub body: Body,
}

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
    /// Google's picture of what it holds, when Google made one: a link that lasts hours and
    /// opens only for the account.
    pub thumbnail_link: Option<String>,
    /// Which picture that is, a number that grows as the file changes.
    pub thumbnail_version: Option<String>,
}
impl Item {
    pub fn folder(&self) -> bool {
        self.mime_type == FOLDER
    }
}
/// Someone a folder is shared with, as Drive lists them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    /// `owner`, `writer`, `commenter` or `reader`.
    pub role: String,
    /// `user`, `group`, `domain` or `anyone`.
    #[serde(rename = "type")]
    pub kind: String,
    pub email_address: Option<String>,
}
/// How full the account's Drive is, in bytes: none for a limit Google sets no number on, as a
/// Workspace's pooled storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Storage {
    pub used: u64,
    pub limit: Option<u64>,
}
/// Why Drive would not share with an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The address is no Google account, so the folder can't be shared with it.
    NoGoogleAccount,
    /// Drive refused for another reason, by its code: a Workspace that shares only inside
    /// itself, say.
    Other(String),
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
    /// the account's Drive. Its editors can't share it on: who the team's files are shared with
    /// follows the team.
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
                    thumbnail_link: None,
                    thumbnail_version: None,
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
                .json(&json!({
                    "name": name,
                    "mimeType": mime,
                    "parents": parents,
                    "writersCanShare": false,
                })),
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
    /// Who the folder `id` is shared with.
    pub async fn shares(&self, access: &str, id: &str) -> Result<Vec<Share>> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let mut shares = vec![Share {
                    id: "owner".into(),
                    role: "owner".into(),
                    kind: "user".into(),
                    email_address: Some(drive.account.clone()),
                }];
                shares.extend(drive.shares.get(id).cloned().unwrap_or_default());
                Ok(shares)
            });
        }
        #[derive(Deserialize)]
        struct Shares {
            permissions: Vec<Share>,
        }
        let listed: Shares = send(
            HTTP.get(format!("{FILES}/{id}/permissions"))
                .bearer_auth(access)
                .query(&[("fields", "permissions(id,role,type,emailAddress)")]),
        )
        .await?;
        Ok(listed.permissions)
    }
    /// Lets `email` edit the folder `id` and everything in it, without Google emailing them:
    /// the share's ID, or why Drive refused.
    pub async fn share(
        &self,
        access: &str,
        id: &str,
        email: &str,
    ) -> Result<std::result::Result<String, Refused>> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                // Fixture mode's Google knows no account at example.net.
                if email.ends_with("@example.net") {
                    return Ok(Err(Refused::NoGoogleAccount));
                }
                let share = Share {
                    id: format!("fixture-share-{}", crypto::hex(&crypto::random::<6>()?)),
                    role: "writer".into(),
                    kind: "user".into(),
                    email_address: Some(email.to_owned()),
                };
                let shares = drive.shares.entry(id.to_owned()).or_default();
                shares.push(share.clone());
                Ok(Ok(share.id))
            });
        }
        let response = HTTP
            .post(format!("{FILES}/{id}/permissions"))
            .bearer_auth(access)
            .query(&[("sendNotificationEmail", "false"), ("fields", "id")])
            .json(&json!({"type": "user", "role": "writer", "emailAddress": email}))
            .send()
            .await
            .map_err(super::google::unreachable)?;
        if response.status() == reqwest::StatusCode::BAD_REQUEST
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let reason = body
                .pointer("/error/errors/0/reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            return Ok(Err(match reason {
                "invalidSharingRequest" => Refused::NoGoogleAccount,
                other => Refused::Other(other.to_owned()),
            }));
        }
        #[derive(Deserialize)]
        struct Made {
            id: String,
        }
        let made: Made = super::google::read(response).await?;
        Ok(Ok(made.id))
    }
    /// Takes back the share `share` of the folder `id`. One already gone is taken back.
    pub async fn unshare(&self, access: &str, id: &str, share: &str) -> Result<()> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                if let Some(shares) = drive.shares.get_mut(id) {
                    shares.retain(|each| each.id != share);
                }
                Ok(())
            });
        }
        let response = HTTP
            .delete(format!("{FILES}/{id}/permissions/{share}"))
            .bearer_auth(access)
            .send()
            .await
            .map_err(super::google::unreachable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND || response.status().is_success() {
            return Ok(());
        }
        super::google::read::<serde_json::Value>(response)
            .await
            .map(drop)
    }
    /// How full the account's Drive is.
    pub async fn storage(&self, access: &str) -> Result<Storage> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                Ok(Storage {
                    used: drive.files.len() as u64 * 1_000_000,
                    limit: Some(15_000_000_000),
                })
            });
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct About {
            storage_quota: Quota,
        }
        #[derive(Deserialize)]
        struct Quota {
            limit: Option<String>,
            usage: Option<String>,
        }
        let about: About = send(
            HTTP.get("https://www.googleapis.com/drive/v3/about")
                .bearer_auth(access)
                .query(&[("fields", "storageQuota(limit,usage)")]),
        )
        .await?;
        let number = |value: Option<String>| value.and_then(|v| v.parse::<u64>().ok());
        Ok(Storage {
            used: number(about.storage_quota.usage).unwrap_or(0),
            limit: number(about.storage_quota.limit),
        })
    }
    /// Starts uploading `length` bytes of `mime` named `name` into the folder `parent`, and
    /// answers where the bytes go. As with what Dispatch makes, its editors can't share it on.
    pub async fn start_upload(
        &self,
        access: &str,
        name: &str,
        mime: &str,
        parent: &str,
        length: u64,
    ) -> Result<String> {
        if let Self::Fixture = self {
            let session = format!("fixture-upload-{}", crypto::hex(&crypto::random::<8>()?));
            return fixture(access, |drive| {
                let started = (name.to_owned(), mime.to_owned(), parent.to_owned());
                drive.uploads.insert(session.clone(), started);
                Ok(session)
            });
        }
        let response = upload_start(access, name, mime, parent, length)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(read::<serde_json::Value>(response)
                .await
                .err()
                .unwrap_or_else(|| Error::new("google_unreachable", 502)));
        }
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| Error::new("google_unreachable", 502))
    }
    /// Sends the upload's bytes to where `session` said, and answers the file Drive made, as
    /// the upload's start asked for it.
    pub async fn finish_upload(&self, access: &str, session: &str, upload: Upload) -> Result<Item> {
        let length = upload.length;
        if let Self::Fixture = self {
            let bytes: Vec<u8> = upload
                .stream()
                .map_ok(|chunk| chunk.to_vec())
                .try_concat()
                .await
                .map_err(|_| Error::new("upload_incomplete", 400))?;
            return fixture(access, |drive| {
                let (name, mime, parent) = drive.uploads.remove(session).ok_or_else(missing)?;
                let account = drive.account.clone();
                let id = session.replace("fixture-upload-", "fixture-");
                // Google pictures an image as itself; fixture mode, only images.
                let pictured = mime.starts_with("image/");
                let item = Item {
                    web_view_link: link(&mime, &id),
                    thumbnail_link: pictured.then(|| format!("fixture:{id}")),
                    thumbnail_version: pictured.then(|| "1".to_owned()),
                    id: id.clone(),
                    name,
                    mime_type: mime,
                    parents: vec![parent],
                    modified_time: iso(),
                    last_modifying_user: Some(Person {
                        display_name: Some(account.clone()),
                        email_address: Some(account),
                    }),
                    size: Some(length.to_string()),
                };
                drive.files.insert(id.clone(), item.clone());
                drive.contents.insert(id, bytes);
                Ok(item)
            });
        }
        let response = TRANSFER
            .put(session)
            .bearer_auth(access)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::wrap_stream(upload.stream()))
            .send()
            .await
            .map_err(|error| {
                // The member's bytes stopped coming, rather than Google answering.
                if error.is_body() || error.is_request() {
                    Error::new("upload_incomplete", 400)
                } else {
                    unreachable(error)
                }
            })?;
        read(response).await
    }
    /// Google's picture of what a file holds, by the `link` a listing gave, sized for a card.
    /// Google refusing it only leaves the card its drawing, so it never counts against the
    /// connection.
    pub async fn picture(&self, access: &str, link: &str) -> Result<Picture> {
        if let Self::Fixture = self {
            let id = link.strip_prefix("fixture:").ok_or_else(missing)?;
            return fixture(access, |drive| {
                let item = drive.files.get(id).ok_or_else(missing)?;
                let bytes = drive.contents.get(id).ok_or_else(missing)?;
                Ok(Picture {
                    kind: item.mime_type.clone(),
                    bytes: Bytes::from(bytes.clone()),
                })
            });
        }
        let link = sized(link).ok_or_else(missing)?;
        let _turn = PICTURES.acquire().await.map_err(|_| missing())?;
        let response = HTTP
            .get(link)
            .bearer_auth(access)
            .send()
            .await
            .map_err(unreachable)?;
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|kind| kind.to_str().ok())
            .map(|kind| {
                kind.split(';')
                    .next()
                    .unwrap_or(kind)
                    .trim()
                    .to_ascii_lowercase()
            })
            .filter(|kind| {
                ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&kind.as_str())
            });
        let (true, Some(kind)) = (response.status().is_success(), kind) else {
            return Err(missing());
        };
        if response
            .content_length()
            .is_some_and(|length| length > PICTURE_LIMIT as u64)
        {
            return Err(missing());
        }
        let bytes = response.bytes().await.map_err(unreachable)?;
        if bytes.len() > PICTURE_LIMIT {
            return Err(missing());
        }
        Ok(Picture { kind, bytes })
    }
    /// The file `item`, to save: Google's own Docs, Sheets and Slides as the Word, Excel and
    /// PowerPoint files they export as, and an uploaded file as it was.
    pub async fn download(&self, access: &str, item: &Item) -> Result<Download> {
        let exported = match item.mime_type.as_str() {
            DOC => Some(DOCX),
            SHEET => Some(XLSX),
            SLIDES => Some(PPTX),
            google if google.starts_with("application/vnd.google-apps.") => {
                return Err(Error::new("documents_not_downloadable", 409));
            }
            _ => None,
        };
        let (kind, name) = match exported {
            Some((kind, ending)) => (kind.to_owned(), format!("{}.{ending}", item.name)),
            None => (item.mime_type.clone(), item.name.clone()),
        };
        if let Self::Fixture = self {
            let bytes = fixture(access, |drive| {
                drive.files.get(&item.id).ok_or_else(missing)?;
                Ok(drive.contents.get(&item.id).cloned().unwrap_or_else(|| {
                    format!("Fixture mode's export of {}", item.name).into_bytes()
                }))
            })?;
            return Ok(Download {
                kind,
                name,
                length: Some(bytes.len() as u64),
                body: Body::from(bytes),
            });
        }
        let request = match exported {
            Some((kind, _)) => TRANSFER
                .get(format!("{FILES}/{}/export", item.id))
                .query(&[("mimeType", kind)]),
            None => TRANSFER
                .get(format!("{FILES}/{}", item.id))
                .query(&[("alt", "media")]),
        };
        let response = request
            .bearer_auth(access)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(read::<serde_json::Value>(response)
                .await
                .err()
                .unwrap_or_else(|| Error::new("google_unreachable", 502)));
        }
        Ok(Download {
            kind,
            name,
            length: response.content_length(),
            body: Body::from_stream(response.bytes_stream()),
        })
    }
    /// The file `id`, if Dispatch can reach it: Google's picker grants Dispatch the files
    /// someone picks with the account.
    pub async fn get(&self, access: &str, id: &str) -> Result<Item> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                (!drive.hidden.contains(id))
                    .then(|| drive.files.get(id).cloned())
                    .flatten()
                    .ok_or_else(missing)
            });
        }
        send(
            HTTP.get(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS)]),
        )
        .await
    }
    /// Moves the file `item` into the folder `folder`, out of the folders it was in.
    pub async fn move_into(&self, access: &str, item: &Item, folder: &str) -> Result<Item> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let file = drive.files.get_mut(&item.id).ok_or_else(missing)?;
                file.parents = vec![folder.to_owned()];
                Ok(file.clone())
            });
        }
        send(
            HTTP.patch(format!("{FILES}/{}", item.id))
                .bearer_auth(access)
                .query(&[
                    ("addParents", folder),
                    ("removeParents", &item.parents.join(",")),
                    ("fields", FIELDS),
                ])
                .json(&json!({})),
        )
        .await
    }
    /// In fixture mode, grants Dispatch the files `ids`, as picking them in Google's picker
    /// does. Google's own grant happens in the browser, so live there is nothing to do.
    pub fn grant(&self, access: &str, ids: &[String]) -> Result<()> {
        if let Self::Fixture = self {
            fixture(access, |drive| {
                drive.hidden.retain(|id| !ids.contains(id));
                Ok(())
            })?;
        }
        Ok(())
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

/// The request that starts an upload. Google describes the file it makes with the fields
/// this request asks for, and ignores those the request carrying the bytes asks for.
fn upload_start(
    access: &str,
    name: &str,
    mime: &str,
    parent: &str,
    length: u64,
) -> reqwest::RequestBuilder {
    HTTP.post(UPLOADS)
        .bearer_auth(access)
        .query(&[("uploadType", "resumable"), ("fields", FIELDS)])
        .header("X-Upload-Content-Type", mime)
        .header("X-Upload-Content-Length", length)
        .json(&json!({
            "name": name,
            "mimeType": mime,
            "parents": [parent],
            "writersCanShare": false,
        }))
}

/// A picture of what a file holds, as Google sent it.
pub struct Picture {
    pub kind: String,
    pub bytes: Bytes,
}
/// Google's link to a picture, asking for it at a card's size: only a link to Google's own
/// pictures, over HTTPS, since the account's token goes with it.
fn sized(link: &str) -> Option<String> {
    let url = url::Url::parse(link).ok()?;
    let host = url.host_str()?;
    let google = host.ends_with(".googleusercontent.com")
        || ["docs.google.com", "drive.google.com"].contains(&host);
    if url.scheme() != "https" || !google {
        return None;
    }
    // Google names the size at the link's end, `=s220`.
    Some(match link.rsplit_once("=s") {
        Some((start, size)) if !size.is_empty() && size.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{start}=s{PICTURE_SIZE}")
        }
        _ => link.to_owned(),
    })
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
    /// Who each folder is shared with, beside the account.
    shares: BTreeMap<String, Vec<Share>>,
    /// Uploads started, by where their bytes go: the name, type and folder each was given.
    uploads: BTreeMap<String, (String, String, String)>,
    /// What was uploaded, by file.
    contents: BTreeMap<String, Vec<u8>>,
    /// The files someone made directly in Drive, which Dispatch can't reach until someone
    /// picks them.
    hidden: BTreeSet<String>,
}
impl Drive {
    /// The files Dispatch can reach.
    fn values(&self) -> impl Iterator<Item = &Item> {
        self.files
            .values()
            .filter(|item| !self.hidden.contains(&item.id))
    }
}
static DRIVES: LazyLock<Mutex<BTreeMap<String, Drive>>> = LazyLock::new(Default::default);
/// Runs `work` on the Drive of the account fixture mode's access token names.
fn fixture<T>(access: &str, work: impl FnOnce(&mut Drive) -> Result<T>) -> Result<T> {
    let account = access
        .strip_prefix("fixture:")
        .ok_or_else(|| Error::new("google_connection_broken", 409))?;
    let mut drives = DRIVES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let drive = drives.entry(account.to_owned()).or_insert_with(|| Drive {
        account: account.to_owned(),
        files: BTreeMap::new(),
        shares: BTreeMap::new(),
        uploads: BTreeMap::new(),
        contents: BTreeMap::new(),
        hidden: BTreeSet::new(),
    });
    work(drive)
}
/// Puts files in fixture mode's Drive as someone making them directly in Drive would: two in
/// `folder`, one elsewhere in the account's Drive, all out of Dispatch's reach.
pub fn fixture_made_in_drive(access: &str, folder: &str) -> Result<()> {
    fixture(access, |drive| {
        for (name, mime, size, inside) in [
            ("Weekly safety huddle", DOC, None, true),
            ("Fuel receipts.pdf", "application/pdf", Some("48213"), true),
            ("Route map 2025.png", "image/png", Some("231004"), false),
        ] {
            let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
            let item = Item {
                web_view_link: link(mime, &id),
                id: id.clone(),
                name: name.to_owned(),
                mime_type: mime.to_owned(),
                parents: inside.then(|| folder.to_owned()).into_iter().collect(),
                modified_time: iso(),
                last_modifying_user: Some(Person {
                    display_name: Some("Keisha Brown".to_owned()),
                    email_address: Some("keisha.brown@example.com".to_owned()),
                }),
                size: size.map(str::to_owned),
                thumbnail_link: None,
                thumbnail_version: None,
            };
            drive.files.insert(id.clone(), item);
            drive.hidden.insert(id);
        }
        Ok(())
    })
}
/// The files fixture mode's Drive holds that Dispatch can't reach yet.
pub fn fixture_hidden(access: &str) -> Result<Vec<Item>> {
    fixture(access, |drive| {
        Ok(drive
            .hidden
            .iter()
            .filter_map(|id| drive.files.get(id).cloned())
            .collect())
    })
}
/// Whether fixture mode's Drive has the file, out of the trash.
pub fn fixture_has(access: &str, id: &str) -> Result<bool> {
    fixture(access, |drive| Ok(drive.files.contains_key(id)))
}

#[cfg(test)]
#[path = "../tests/backend/drive.rs"]
mod tests;
