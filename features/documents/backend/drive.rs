//! Google Drive, as Documents uses it with the DSP's account: what's inside its folder, what
//! changed since, and making, uploading, downloading, renaming and trashing files. Dispatch
//! reaches everything in the account's Drive, so the folder is read folder by folder, whoever
//! made what's in it. A connection made before Dispatch asked for that reaches only the files
//! it made or was given, and reads only those. Fixture mode keeps a Drive of its own in memory
//! for each account, as Google keeps an account's files for every DSP that connects it.
use super::google::{FILES, FOLDER, Google, HTTP, Timed, read, send, unreachable};
use axum::body::Body;
use axum::body::Bytes;
use dispatch_core::{
    Error, Result, db::iso, ensure, foundation::crypto, server::http::upload::Upload,
};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

pub const DOC: &str = "application/vnd.google-apps.document";
pub const SHEET: &str = "application/vnd.google-apps.spreadsheet";
pub const SLIDES: &str = "application/vnd.google-apps.presentation";
pub const SHORTCUT: &str = "application/vnd.google-apps.shortcut";
/// What Documents asks of each file.
const FIELDS: &str = concat!(
    "id,name,mimeType,parents,modifiedTime,lastModifyingUser(displayName,emailAddress),size,",
    "webViewLink,thumbnailLink,thumbnailVersion,ownedByMe,shortcutDetails(targetId,targetMimeType),",
    "capabilities(canShare)"
);
/// Asked of every call about one file, which may sit in a shared drive: Google answers for
/// one there only when told the app understands them.
const ALL_DRIVES: (&str, &str) = ("supportsAllDrives", "true");
/// Enough pages for 50,000 files; a Drive that keeps answering past them is not listed whole.
const PAGES: usize = 50;
/// Folders asked about in one listing, as long a query as Google takes.
const FOLDERS_AT_ONCE: usize = 40;
/// The most a folder is listed with: calls to Google, and files.
const CALLS: usize = 500;
const FILES_AT_MOST: usize = 50_000;
/// What changed in the account's Drive.
const CHANGES: &str = "https://www.googleapis.com/drive/v3/changes";
/// Where a file's bytes go up.
const UPLOADS: &str = "https://www.googleapis.com/upload/drive/v3/files";
/// The longest sides a card's picture is fetched at, in pixels: sharp on an ordinary screen,
/// and on a high-density one.
pub const PICTURE_SIZES: [u32; 2] = [300, 600];
/// The largest picture Dispatch hands on; Google's are a small part of it.
const PICTURE_LIMIT: usize = 2 * 1024 * 1024;
/// How many pictures the server fetches from Google at once, across every DSP.
static PICTURES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
/// The kinds of picture Dispatch hands on.
const PICTURE_KINDS: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/// Fetches pictures, following a redirect only to Google's own hosts.
static PICTURE_HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && google(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .expect("the TLS backend is built in")
});
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
    /// Whether the account owns it: only its owner moves a file from one folder to another.
    #[serde(default)]
    pub owned_by_me: bool,
    /// For a shortcut, the file it stands for.
    pub shortcut_details: Option<Shortcut>,
    /// What the account may do with it.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// When Google described it, which its picture's link lasts hours from.
    #[serde(skip, default = "Instant::now")]
    pub seen: Instant,
}
/// What the account may do with a file.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Whether it may share the file on: an editor whose owner allows it.
    #[serde(default)]
    pub can_share: bool,
}
/// The file a shortcut stands for.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub target_id: String,
    pub target_mime_type: String,
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

/// What changed in the account's Drive since Dispatch last asked, and where to ask from next.
pub struct Changes {
    pub changed: Vec<Changed>,
    pub next: String,
}
/// A file that changed: as it is now, or gone from Dispatch's reach, trashed or deleted.
pub enum Changed {
    Kept(Box<Item>),
    Gone(String),
}

/// A page of Google's change feed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePage {
    #[serde(default)]
    changes: Vec<Change>,
    next_page_token: Option<String>,
    new_start_page_token: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    file_id: Option<String>,
    #[serde(default)]
    removed: bool,
    file: Option<ChangedFile>,
}
#[derive(Deserialize)]
struct ChangedFile {
    #[serde(flatten)]
    item: Item,
    #[serde(default)]
    trashed: bool,
}
/// Where the change feed goes on after a page: its next page, or where the changes after
/// these start.
enum Then {
    Page(String),
    Done(String),
}
impl ChangePage {
    /// Adds the page's changes to `changed`, each file as it is now or gone from Dispatch's
    /// reach; answers where the feed goes on, if Google said.
    fn read(self, changed: &mut Vec<Changed>) -> Option<Then> {
        for change in self.changes {
            // A change to a shared drive itself names no file.
            let Some(id) = change.file_id else { continue };
            changed.push(match change.file {
                Some(file) if !change.removed && !file.trashed => {
                    Changed::Kept(Box::new(file.item))
                }
                _ => Changed::Gone(id),
            });
        }
        match (self.new_start_page_token, self.next_page_token) {
            (Some(next), _) => Some(Then::Done(next)),
            (None, Some(page)) => Some(Then::Page(page)),
            (None, None) => None,
        }
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
    /// Everything inside the folder `root`, out of the trash, read a few folders at a time,
    /// and the files outside it that shortcuts inside stand for.
    pub async fn inside(&self, access: &str, root: &str) -> Result<Vec<Item>> {
        if let Self::Fixture = self {
            return fixture(access, |drive| Ok(drive.values().map(described).collect()));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            files: Vec<Item>,
            next_page_token: Option<String>,
        }
        let mut items = Vec::new();
        let mut folders = vec![root.to_owned()];
        let mut calls = 0;
        while !folders.is_empty() {
            let batch: Vec<String> = folders
                .drain(..folders.len().min(FOLDERS_AT_ONCE))
                .collect();
            let parents: Vec<String> = batch
                .iter()
                .map(|folder| format!("'{folder}' in parents"))
                .collect();
            let q = format!("({}) and trashed=false", parents.join(" or "));
            let mut next: Option<String> = None;
            loop {
                calls += 1;
                ensure(
                    calls <= CALLS && items.len() <= FILES_AT_MOST,
                    "documents_too_many_files",
                    507,
                )?;
                let mut query = vec![
                    ("q", q.clone()),
                    ("fields", format!("nextPageToken,files({FIELDS})")),
                    ("pageSize", "1000".to_owned()),
                    ("spaces", "drive".to_owned()),
                    ("includeItemsFromAllDrives", "true".to_owned()),
                    ("supportsAllDrives", "true".to_owned()),
                ];
                query.extend(next.take().map(|token| ("pageToken", token)));
                let page: Page = send(HTTP.get(FILES).bearer_auth(access).query(&query)).await?;
                folders.extend(
                    page.files
                        .iter()
                        .filter(|item| item.mime_type == FOLDER)
                        .map(|item| item.id.clone()),
                );
                items.extend(page.files);
                match page.next_page_token {
                    Some(token) => next = Some(token),
                    None => break,
                }
            }
        }
        // The files the shortcuts inside stand for, wherever they are: one the account no longer
        // reaches is left out, and its shortcut reads as unavailable.
        let targets: BTreeSet<String> = items
            .iter()
            .filter_map(|item| item.shortcut_details.as_ref())
            .map(|shortcut| shortcut.target_id.clone())
            .collect();
        for target in targets {
            match self.get(access, &target).await {
                Ok(item) => items.push(item),
                Err(error) if error.code == "documents_item_not_found" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(items)
    }
    /// Where the account's changes are read from, for those after now: asked before its files
    /// are listed, so nothing changed while they were is missed.
    pub async fn start_changes(&self, access: &str) -> Result<String> {
        if let Self::Fixture = self {
            return fixture(access, |drive| Ok(drive.changes.to_string()));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Start {
            start_page_token: String,
        }
        let start: Start = send(
            HTTP.get(format!("{CHANGES}/startPageToken"))
                .bearer_auth(access),
        )
        .await?;
        Ok(start.start_page_token)
    }
    /// What changed in the files Dispatch can reach since `from`, each as it is now.
    pub async fn changes(&self, access: &str, from: &str) -> Result<Changes> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let from: u64 = from
                    .parse()
                    .map_err(|_| Error::new("documents_changes_lost", 410))?;
                let changed = drive
                    .changed
                    .iter()
                    .filter(|(_, at)| **at > from)
                    .map(|(id, _)| match drive.files.get(id) {
                        Some(item) if !drive.hidden.contains(id) => {
                            Changed::Kept(Box::new(described(item)))
                        }
                        _ => Changed::Gone(id.clone()),
                    })
                    .collect();
                Ok(Changes {
                    changed,
                    next: drive.changes.to_string(),
                })
            });
        }
        let fields = format!(
            "nextPageToken,newStartPageToken,changes(fileId,removed,file({FIELDS},trashed))"
        );
        let mut changed = Vec::new();
        let mut at = from.to_owned();
        for _ in 0..PAGES {
            let page: ChangePage = send(HTTP.get(CHANGES).bearer_auth(access).query(&[
                ("pageToken", at.as_str()),
                ("fields", fields.as_str()),
                ("pageSize", "1000"),
                ("spaces", "drive"),
                ("includeRemoved", "true"),
                ("includeItemsFromAllDrives", "true"),
                ("supportsAllDrives", "true"),
            ]))
            .await?;
            match page.read(&mut changed) {
                Some(Then::Done(next)) => return Ok(Changes { changed, next }),
                Some(Then::Page(token)) => at = token,
                None => return Err(Error::new("documents_changes_lost", 410)),
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
                    owned_by_me: true,
                    shortcut_details: None,
                    capabilities: Capabilities { can_share: true },
                    seen: Instant::now(),
                };
                drive.files.insert(id.clone(), item.clone());
                drive.touch(&id);
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
                let item = described(item);
                drive.touch(id);
                Ok(item)
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
                .query(&[
                    ("fields", "permissions(id,role,type,emailAddress)"),
                    ALL_DRIVES,
                ]),
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
                if drive
                    .files
                    .get(id)
                    .is_some_and(|file| !file.capabilities.can_share)
                {
                    return Ok(Err(Refused::Other("insufficientFilePermissions".into())));
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
            .query(&[
                ("sendNotificationEmail", "false"),
                ("fields", "id"),
                ALL_DRIVES,
            ])
            .json(&json!({"type": "user", "role": "writer", "emailAddress": email}))
            .timed()
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
            .query(&[ALL_DRIVES])
            .bearer_auth(access)
            .timed()
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
            .timed()
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
                    owned_by_me: true,
                    shortcut_details: None,
                    capabilities: Capabilities { can_share: true },
                    seen: Instant::now(),
                };
                drive.files.insert(id.clone(), item.clone());
                drive.contents.insert(id.clone(), bytes);
                drive.touch(&id);
                Ok(item)
            });
        }
        let response = TRANSFER
            .put(session)
            .bearer_auth(access)
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(reqwest::Body::wrap_stream(upload.stream()))
            .timed()
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
    pub async fn picture(&self, access: &str, link: &str, size: u32) -> Result<Picture> {
        if let Self::Fixture = self {
            let id = link.strip_prefix("fixture:").ok_or_else(missing)?;
            return fixture(access, |drive| {
                let item = drive.files.get(id).ok_or_else(missing)?;
                let bytes = drive.contents.get(id).ok_or_else(missing)?;
                ensure(
                    handed(&item.mime_type, bytes.len()),
                    "documents_item_not_found",
                    404,
                )?;
                Ok(Picture {
                    kind: item.mime_type.clone(),
                    bytes: Bytes::from(bytes.clone()),
                })
            });
        }
        let link = sized(link, size).ok_or_else(missing)?;
        let _turn = PICTURES.acquire().await.map_err(|_| missing())?;
        let mut response = PICTURE_HTTP
            .get(link)
            .bearer_auth(access)
            .timed()
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
            .unwrap_or_default();
        let length = response
            .content_length()
            .map_or(0, |length| usize::try_from(length).unwrap_or(usize::MAX));
        if !response.status().is_success() || !handed(&kind, length) {
            return Err(missing());
        }
        // Read as it comes, giving up past the limit, whatever length Google stated.
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > PICTURE_LIMIT {
                return Err(missing());
            }
        }
        Ok(Picture {
            kind,
            bytes: bytes.into(),
        })
    }
    /// The file `id`, named `name`, of `mime`, to save: Google's own Docs, Sheets and Slides
    /// as the Word, Excel and PowerPoint files they export as, and an uploaded file as it was.
    pub async fn download(
        &self,
        access: &str,
        id: &str,
        name: &str,
        mime: &str,
    ) -> Result<Download> {
        let exported = match mime {
            DOC => Some(DOCX),
            SHEET => Some(XLSX),
            SLIDES => Some(PPTX),
            google if google.starts_with("application/vnd.google-apps.") => {
                return Err(Error::new("documents_not_downloadable", 409));
            }
            _ => None,
        };
        let (kind, saved) = match exported {
            Some((kind, ending)) => (kind.to_owned(), format!("{name}.{ending}")),
            None => (mime.to_owned(), name.to_owned()),
        };
        if let Self::Fixture = self {
            let bytes = fixture(access, |drive| {
                drive.files.get(id).ok_or_else(missing)?;
                Ok(drive
                    .contents
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("Fixture mode's export of {name}").into_bytes()))
            })?;
            return Ok(Download {
                kind,
                name: saved,
                length: Some(bytes.len() as u64),
                body: Body::from(bytes),
            });
        }
        let request = match exported {
            Some((kind, _)) => TRANSFER
                .get(format!("{FILES}/{id}/export"))
                .query(&[("mimeType", kind), ALL_DRIVES]),
            None => TRANSFER
                .get(format!("{FILES}/{id}"))
                .query(&[("alt", "media"), ALL_DRIVES]),
        };
        let response = request
            .bearer_auth(access)
            .timed()
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
            name: saved,
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
                    .then(|| drive.files.get(id).map(described))
                    .flatten()
                    .ok_or_else(missing)
            });
        }
        send(
            HTTP.get(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS), ALL_DRIVES]),
        )
        .await
    }
    /// Moves the file `item` into the folder `folder`, out of the folders it was in.
    pub async fn move_into(&self, access: &str, item: &Item, folder: &str) -> Result<Item> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                let file = drive.files.get_mut(&item.id).ok_or_else(missing)?;
                ensure(file.owned_by_me, "documents_cannot_move", 409)?;
                file.parents = vec![folder.to_owned()];
                let file = described(file);
                drive.touch(&item.id);
                Ok(file)
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
    /// Makes a shortcut to the file `item` in the folder `folder`, as Drive's own "Add shortcut"
    /// does for a file only its owner can move.
    pub async fn shortcut(&self, access: &str, item: &Item, folder: &str) -> Result<Item> {
        let target = Shortcut {
            target_id: item.id.clone(),
            target_mime_type: item.mime_type.clone(),
        };
        if let Self::Fixture = self {
            let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
            return fixture(access, |drive| {
                let made = Item {
                    web_view_link: link(SHORTCUT, &id),
                    id: id.clone(),
                    mime_type: SHORTCUT.to_owned(),
                    parents: vec![folder.to_owned()],
                    modified_time: iso(),
                    size: None,
                    thumbnail_link: None,
                    thumbnail_version: None,
                    owned_by_me: true,
                    shortcut_details: Some(target),
                    capabilities: Capabilities { can_share: true },
                    seen: Instant::now(),
                    ..item.clone()
                };
                drive.files.insert(id.clone(), made.clone());
                drive.touch(&id);
                Ok(made)
            });
        }
        send(
            HTTP.post(FILES)
                .bearer_auth(access)
                .query(&[("fields", FIELDS), ALL_DRIVES])
                .json(&json!({
                    "name": item.name,
                    "mimeType": SHORTCUT,
                    "parents": [folder],
                    "shortcutDetails": {"targetId": target.target_id},
                })),
        )
        .await
    }
    /// In fixture mode, grants Dispatch the files `ids`, as picking them in Google's picker
    /// does. Google's own grant happens in the browser, so live there is nothing to do.
    pub fn grant(&self, access: &str, ids: &[String]) -> Result<()> {
        if let Self::Fixture = self {
            fixture(access, |drive| {
                drive.hidden.retain(|id| !ids.contains(id));
                ids.iter().for_each(|id| drive.touch(id));
                Ok(())
            })?;
        }
        Ok(())
    }
    /// Moves a file to the account's trash, where Drive keeps it for 30 days.
    pub async fn trash(&self, access: &str, id: &str) -> Result<()> {
        if let Self::Fixture = self {
            return fixture(access, |drive| {
                drive.files.remove(id).ok_or_else(missing)?;
                drive.touch(id);
                Ok(())
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
#[derive(Clone)]
pub struct Picture {
    pub kind: String,
    pub bytes: Bytes,
}
/// Whether a picture of `kind` and `length` bytes is one Dispatch hands on.
fn handed(kind: &str, length: usize) -> bool {
    PICTURE_KINDS.contains(&kind) && length <= PICTURE_LIMIT
}
/// Whether `url` is Google's own, over HTTPS: the only place a picture is fetched from, since
/// the account's token goes with it.
fn google(url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    url.scheme() == "https"
        && (host.ends_with(".googleusercontent.com")
            || ["docs.google.com", "drive.google.com"].contains(&host))
}
/// Google's link to a picture, asking for it `size` pixels on its longest side: only a link of
/// Google's own.
fn sized(link: &str, size: u32) -> Option<String> {
    if !google(&reqwest::Url::parse(link).ok()?) {
        return None;
    }
    // Google names the size at the link's end, `=s220`.
    Some(match link.rsplit_once("=s") {
        Some((start, given)) if !given.is_empty() && given.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{start}=s{size}")
        }
        _ => link.to_owned(),
    })
}

fn missing() -> Error {
    Error::new("documents_item_not_found", 404)
}

/// Where Google opens a file of `mime`.
/// Where Google opens the file `id` of `mime`.
pub fn link(mime: &str, id: &str) -> String {
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
    /// How many changes the Drive has had, and which of them each file changed in last, as
    /// Google's change feed counts them.
    changes: u64,
    changed: BTreeMap<String, u64>,
}
impl Drive {
    /// The files Dispatch can reach.
    fn values(&self) -> impl Iterator<Item = &Item> {
        self.files
            .values()
            .filter(|item| !self.hidden.contains(&item.id))
    }
    /// Notes a change to the file `id`, for the change feed.
    fn touch(&mut self, id: &str) {
        self.changes += 1;
        self.changed.insert(id.to_owned(), self.changes);
    }
}
/// A file as fixture mode's Google describes it now.
fn described(item: &Item) -> Item {
    Item {
        seen: Instant::now(),
        ..item.clone()
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
        changes: 0,
        changed: BTreeMap::new(),
    });
    work(drive)
}
/// Puts files in fixture mode's Drive as someone making them directly in Drive would: a folder
/// and a file in `folder`, a Doc in that folder, a picture elsewhere in the account's Drive, and
/// two Sheets someone else owns and shared with the account, one to view only, all out of
/// Dispatch's reach.
pub fn fixture_made_in_drive(access: &str, folder: &str) -> Result<()> {
    let id = || Ok::<_, Error>(format!("fixture-{}", crypto::hex(&crypto::random::<8>()?)));
    let safety = id()?;
    fixture(access, |drive| {
        for (name, mime, size, parent) in [
            ("Safety", FOLDER, None, Some(folder)),
            ("Weekly safety huddle", DOC, None, Some(safety.as_str())),
            (
                "Fuel receipts.pdf",
                "application/pdf",
                Some("48213"),
                Some(folder),
            ),
            ("Route map 2025.png", "image/png", Some("231004"), None),
            (SHARED_SHEET, SHEET, None, None),
            (VIEW_ONLY_SHEET, SHEET, None, None),
        ] {
            let id = if mime == FOLDER {
                safety.clone()
            } else {
                id()?
            };
            let item = Item {
                web_view_link: link(mime, &id),
                id: id.clone(),
                name: name.to_owned(),
                mime_type: mime.to_owned(),
                parents: parent.map(str::to_owned).into_iter().collect(),
                modified_time: iso(),
                last_modifying_user: Some(Person {
                    display_name: Some("Keisha Brown".to_owned()),
                    email_address: Some("keisha.brown@example.com".to_owned()),
                }),
                size: size.map(str::to_owned),
                thumbnail_link: None,
                thumbnail_version: None,
                // Google shows a file shared with the account in no folder of the account's.
                owned_by_me: name != SHARED_SHEET && name != VIEW_ONLY_SHEET,
                shortcut_details: None,
                capabilities: Capabilities {
                    can_share: name != VIEW_ONLY_SHEET,
                },
                seen: Instant::now(),
            };
            drive.files.insert(id.clone(), item);
            // Inside the folder, Dispatch reaches them at once; elsewhere, once they're added.
            if parent.is_none() {
                drive.hidden.insert(id);
            }
        }
        Ok(())
    })
}
/// The Sheet someone else owns in fixture mode, shared with the account.
pub const SHARED_SHEET: &str = "Route tracker";
/// One shared with the account to view only, which it can't share on.
pub const VIEW_ONLY_SHEET: &str = "Fuel prices";
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
