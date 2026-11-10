//! Drive file metadata and mutations. Dispatch lists folder by folder because it reaches
//! everything in the account's Drive, whoever made it. Older connections reach only files
//! Dispatch made or was given. Fixture state belongs to the account, shared by its DSPs.
pub(super) mod fixture;
mod listing;
mod sharing;
mod transfer;

use super::google::{Connected, DRIVE_HTTP as HTTP, FILES, FOLDER, Google};
use dispatch_core::{Error, Result, db::iso, ensure, foundation::crypto};
use fixture::described;
use serde::Deserialize;
use serde_json::json;
use std::time::Instant;

#[cfg(test)]
pub use fixture::{SHARED_SHEET, fixture_on_sharing};
pub use fixture::{fixture_elsewhere, fixture_has, fixture_hidden, fixture_made_in_folder};
pub use listing::{Changed, Changes};
pub use sharing::{Refused, Share};
pub use transfer::{Download, PICTURE_SIZES, Picture};

pub const DOC: &str = "application/vnd.google-apps.document";
pub const SHEET: &str = "application/vnd.google-apps.spreadsheet";
pub const SLIDES: &str = "application/vnd.google-apps.presentation";
pub const SHORTCUT: &str = "application/vnd.google-apps.shortcut";
/// What Documents asks of each file.
pub(super) const FIELDS: &str = concat!(
    "id,name,mimeType,parents,modifiedTime,lastModifyingUser(displayName,emailAddress),size,",
    "webViewLink,thumbnailLink,thumbnailVersion,ownedByMe,shortcutDetails(targetId,targetMimeType),",
    "capabilities(canShare)"
);
/// Asked of every call about one file, which may sit in a shared drive: Google answers for
/// one there only when told the app understands them.
const ALL_DRIVES: (&str, &str) = ("supportsAllDrives", "true");

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
/// Whoever last changed a file, as Google names them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub display_name: Option<String>,
    pub email_address: Option<String>,
}
/// How full the account's Drive is, in bytes: none for a limit Google sets no number on, as a
/// Workspace's pooled storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Storage {
    pub used: u64,
    pub limit: Option<u64>,
}

impl Connected {
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
        if let Google::Fixture = self.google {
            let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
            return self
                .fixture(access, |drive| drive.create(id, name, mime, parent))
                .await;
        }
        let parents: Vec<&str> = parent.into_iter().collect();
        self.send(
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
        if let Google::Fixture = self.google {
            return self
                .fixture(access, |drive| {
                    let item = drive.files.get_mut(id).ok_or_else(missing)?;
                    item.name = name.to_owned();
                    item.modified_time = iso();
                    let item = described(item);
                    drive.touch(id);
                    Ok(item)
                })
                .await;
        }
        self.send(
            HTTP.patch(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS)])
                .json(&json!({"name": name})),
        )
        .await
    }
    /// The file `id`, if Dispatch can reach it: Google's picker grants Dispatch the files
    /// someone picks with the account.
    pub async fn get(&self, access: &str, id: &str) -> Result<Item> {
        if let Google::Fixture = self.google {
            return self
                .fixture(access, |drive| {
                    (!drive.hidden.contains(id))
                        .then(|| drive.files.get(id).map(described))
                        .flatten()
                        .ok_or_else(missing)
                })
                .await;
        }
        self.send(
            HTTP.get(format!("{FILES}/{id}"))
                .bearer_auth(access)
                .query(&[("fields", FIELDS), ALL_DRIVES]),
        )
        .await
    }
    /// Moves the file `item` into the folder `folder`, out of the folders it was in.
    pub async fn move_into(&self, access: &str, item: &Item, folder: &str) -> Result<Item> {
        if let Google::Fixture = self.google {
            return self
                .fixture(access, |drive| {
                    let file = drive.files.get_mut(&item.id).ok_or_else(missing)?;
                    ensure(file.owned_by_me, "documents_cannot_move", 409)?;
                    file.parents = vec![folder.to_owned()];
                    let file = described(file);
                    drive.touch(&item.id);
                    Ok(file)
                })
                .await;
        }
        self.send(
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
        if let Google::Fixture = self.google {
            let id = format!("fixture-{}", crypto::hex(&crypto::random::<8>()?));
            return self
                .fixture(access, |drive| {
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
                })
                .await;
        }
        self.send(
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
    pub async fn grant(&self, access: &str, ids: &[String]) -> Result<()> {
        if let Google::Fixture = self.google {
            self.fixture(access, |drive| {
                drive.hidden.retain(|id| !ids.contains(id));
                ids.iter().for_each(|id| drive.touch(id));
                Ok(())
            })
            .await?;
        }
        Ok(())
    }
    /// Moves a file to the account's trash, where Drive keeps it for 30 days.
    pub async fn trash(&self, access: &str, id: &str) -> Result<()> {
        if let Google::Fixture = self.google {
            return self
                .fixture(access, |drive| {
                    drive.files.remove(id).ok_or_else(missing)?;
                    drive.touch(id);
                    Ok(())
                })
                .await;
        }
        let _: Item = self
            .send(
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
