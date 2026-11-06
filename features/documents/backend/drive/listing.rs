//! Folder listings, account storage and change-feed pagination.
use super::{
    FIELDS, Item, Storage,
    fixture::{described, fixture},
};
use crate::backend::google::{FILES, FOLDER, Google, HTTP, send};
use dispatch_core::{Error, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;

/// Enough pages for 50,000 files; a Drive that keeps answering past them is not listed whole.
const PAGES: usize = 50;
/// Folders asked about in one listing, as long a query as Google takes.
const FOLDERS_AT_ONCE: usize = 40;
/// The most a folder is listed with: calls to Google, and files.
const CALLS: usize = 500;
const FILES_AT_MOST: usize = 50_000;
/// What changed in the account's Drive.
const CHANGES: &str = "https://www.googleapis.com/drive/v3/changes";

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
        // Every call to Google counts, and what it brings, against the listing's limits.
        let spend = |calls: &mut usize, items: &[Item]| {
            *calls += 1;
            ensure(
                *calls <= CALLS && items.len() <= FILES_AT_MOST,
                "documents_too_many_files",
                507,
            )
        };
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
                spend(&mut calls, &items)?;
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
                ensure(
                    items.len() <= FILES_AT_MOST,
                    "documents_too_many_files",
                    507,
                )?;
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
            spend(&mut calls, &items)?;
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
            return fixture(access, |drive| drive.changes(from));
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
}

#[cfg(test)]
#[path = "../../tests/backend/drive/listing.rs"]
mod tests;
