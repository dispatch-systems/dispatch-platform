//! Documents file commands. Reads use the DSP's in-memory Drive; changes ask Google outside
//! the database, inside the DSP's own tree only, put Google's answer in the copy, then check
//! the member's access again before recording anything.
use super::{
    access::{self, on_drive},
    cache::{PICTURE_LINK_LASTS, shown},
    description::{Described, extension, known},
    drive::{self, Download, Item},
    google::{FOLDER, folder_url},
    grant::{self, Grant},
    mirror,
    storage::{Connection, DocumentsStore},
    team,
    tree::{Row, Tree},
};
use crate::api::types::{
    AddedFiles, DocumentsFolder, DocumentsFolderRead, DocumentsItem, FolderStep, MovedItem, NewKind,
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    ensure,
    foundation::crypto,
    server::http::{route::Dsp, upload::Upload},
};
use std::sync::Arc;

fn not_found() -> Error {
    Error::new("documents_item_not_found", 404)
}
/// `answer`, taking the file `id` out of the DSP's copy when Google said it's gone.
async fn gone<T: Send>(
    state: &Arc<State>,
    owner: &Grant,
    id: &str,
    answer: Result<T>,
) -> Result<T> {
    let id = id.to_owned();
    let missing = answer
        .as_ref()
        .is_err_and(|error| error.code == "documents_item_not_found");
    owner
        .read(state, move |_, owner| {
            if missing {
                owner.mirror.remove(&owner.connection.folder_id, &id);
            }
            Ok(())
        })
        .await?;
    answer
}
async fn confirmed(state: &Arc<State>, owner: &Grant, item: Item) -> Result<()> {
    owner
        .read(state, move |_, owner| {
            owner.mirror.put(&owner.connection.folder_id, item);
            Ok(())
        })
        .await
}

/// The folder `id` of the DSP's Documents, or its main folder, with what it holds; with
/// `query`, everything inside it whose name holds its words.
pub async fn folder(
    state: &Arc<State>,
    c: &Context,
    id: Option<String>,
    query: Option<String>,
) -> Result<DocumentsFolder> {
    projected_folder(state, c, id, query)
        .await
        .map(|(_, folder)| folder)
}

/// The complete existing projection, including current provenance names and thumbnail
/// capability registration, even when the conditional response will omit its items.
async fn projected_folder(
    state: &Arc<State>,
    c: &Context,
    id: Option<String>,
    query: Option<String>,
) -> Result<(Connection, DocumentsFolder)> {
    let owner = grant::capture_member(state, c.clone(), Dsp("documents.use"), None).await?;
    let tree = access::tree_for(state, &owner).await?;
    owner
        .read(state, move |db, owner| {
            let connection = &owner.connection;
            let at = id.clone().unwrap_or_else(|| tree.root().to_owned());
            ensure(tree.is_folder(&at), "documents_item_not_found", 404)?;
            let path: Vec<FolderStep> = tree
                .path(&at)
                .into_iter()
                .map(|step| FolderStep {
                    id: step.id.to_string(),
                    name: step.name.to_string(),
                })
                .collect();
            let (shown, searched) = match query.as_deref().map(str::trim) {
                Some(words) if !words.is_empty() => (tree.search(&at, words), true),
                _ => (tree.children(&at), false),
            };
            let ids: Vec<String> = shown.iter().map(|row| row.id.to_string()).collect();
            let (records, names) = known(db, &owner.key.dsp, &ids)?;
            let described = Described {
                owner: &owner.key,
                tree: &tree,
                records: &records,
                names: &names,
                account: &connection.account.email,
            };
            let folder = DocumentsFolder {
                grant_generation: connection.generation.clone(),
                name: path
                    .last()
                    .map_or_else(|| connection.folder_name.clone(), |step| step.name.clone()),
                url: folder_url(&at),
                id,
                path,
                items: shown
                    .into_iter()
                    .map(|row| described.item(row, searched))
                    .collect(),
            };
            Ok((connection.clone(), folder))
        })
        .await
}

/// A versioned, unambiguously framed hash of the actual response and its DSP/root/query
/// scope. Struct field order and array order are deterministic; no runtime ages are included.
fn folder_revision(
    dsp: &str,
    root: &str,
    query: Option<&str>,
    folder: &DocumentsFolder,
) -> Result<String> {
    let query = query.map(str::trim).filter(|query| !query.is_empty());
    Ok(crypto::sha(serde_json::to_vec(&(
        "documents-folder-read-v1",
        dsp,
        root,
        query,
        folder,
    ))?))
}

/// A complete folder read, or permission to reuse the supplied exact revision. Neither
/// answer bypasses projection, an awaited refresh error, or the final current access check.
pub async fn folder_read(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: Option<String>,
    query: Option<String>,
    since: Option<String>,
) -> Result<DocumentsFolderRead> {
    let (connection, folder) = projected_folder(state, &c, id, query.clone()).await?;
    let revision = folder_revision(&c.dsp.id, &connection.folder_id, query.as_deref(), &folder)?;
    // Check after all async work, for full and unchanged alike. A revision grants no access.
    state
        .read(move |db| {
            let c = access.revalidate(db, &c)?;
            let current = db
                .documents_connection(&c.dsp.id)?
                .ok_or_else(|| Error::new("documents_not_connected", 409))?;
            ensure(!current.broken, "google_connection_broken", 409)?;
            ensure(current == connection, "documents_not_connected", 409)
        })
        .await?;
    Ok(DocumentsFolderRead {
        grant_generation: folder.grant_generation.clone(),
        folder: (since.as_deref() != Some(revision.as_str())).then_some(folder),
        revision,
    })
}

/// Makes a folder, Doc, Sheet or Slides named `name` in the folder `parent`, or the main one.
pub async fn create(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    parent: Option<String>,
    kind: NewKind,
    name: String,
    expected: Option<String>,
) -> Result<DocumentsItem> {
    let dsp = c.dsp.id.clone();
    let owner = grant::capture_member(state, c, access, expected).await?;
    let tree = access::tree_for(state, &owner).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let mime = match kind {
        NewKind::Folder => FOLDER,
        NewKind::Doc => drive::DOC,
        NewKind::Sheet => drive::SHEET,
        NewKind::Slides => drive::SLIDES,
    };
    let (connection, made) = make(state, &owner, &parent, mime, &name).await?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let at = dsp.clone();
    let (records, names) = owner
        .run_member(state, move |db, c| {
            db.record_documents_change(&at, &id, c.actor())?;
            c.audit(db, "documents.created", &title)?;
            known(db, &at, &[id])
        })
        .await?;
    // A file just made holds nothing, so an empty tree counts it right.
    let empty = Tree::new(&connection.folder_id, Vec::new());
    owner
        .read(state, move |_, owner| {
            Ok(Described {
                owner: &owner.key,
                tree: &empty,
                records: &records,
                names: &names,
                account: &connection.account.email,
            }
            .item(&Row::from(made), false))
        })
        .await
}

/// A streamed file with its name and media type.
pub struct FileUpload {
    pub name: String,
    pub mime: String,
    pub body: Upload,
}

/// Uploads a file named `name`, of `mime`, into the folder `parent`, or the main one.
pub async fn upload(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    parent: Option<String>,
    file: FileUpload,
    expected: Option<String>,
) -> Result<DocumentsItem> {
    let FileUpload {
        name,
        mime,
        body: upload,
    } = file;
    let dsp = c.dsp.id.clone();
    let owner = grant::capture_member(state, c, access, expected).await?;
    let tree = access::tree_for(state, &owner).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let (connection, made) = put(state, &owner, &parent, &name, &mime, upload).await?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let at = dsp.clone();
    let (records, names) = owner
        .run_member(state, move |db, c| {
            db.record_documents_change(&at, &id, c.actor())?;
            c.audit(db, "documents.uploaded", &title)?;
            known(db, &at, &[id])
        })
        .await?;
    let empty = Tree::new(&connection.folder_id, Vec::new());
    owner
        .read(state, move |_, owner| {
            Ok(Described {
                owner: &owner.key,
                tree: &empty,
                records: &records,
                names: &names,
                account: &connection.account.email,
            }
            .item(&Row::from(made), false))
        })
        .await
}

/// Makes a folder, Doc, Sheet or Slides named `name`, of `mime`, in the folder `parent` of the
/// DSP's Drive, and puts it in the copy: the connection, and what Google made.
pub(super) async fn make(
    state: &Arc<State>,
    owner: &Grant,
    parent: &str,
    mime: &str,
    name: &str,
) -> Result<(Connection, Item)> {
    let (connection, made) = on_drive!(state, owner, |google, token| google.create(
        token,
        name,
        mime,
        Some(parent)
    ))?;
    confirmed(state, owner, made.clone()).await?;
    Ok((connection, made))
}
/// Streams `upload`, named `name`, of `mime`, into the folder `parent` of the DSP's Drive, and
/// puts it in the copy: the connection, and the file Google made.
pub(super) async fn put(
    state: &Arc<State>,
    owner: &Grant,
    parent: &str,
    name: &str,
    mime: &str,
    upload: Upload,
) -> Result<(Connection, Item)> {
    let length = upload.length;
    let (drive, session) = on_drive!(
        state,
        owner,
        drive,
        drive
            .google
            .start_upload(&drive.access, name, mime, parent, length)
    )?;
    // The bytes go once, with the token that just started the upload.
    owner.admit(state).await?;
    let made = drive
        .google
        .finish_upload(&drive.access, &session, upload)
        .await?;
    confirmed(state, owner, made.clone()).await?;
    Ok((drive.connection, made))
}

/// Writes the page `html` as a Doc named `name` in the folder `parent` of the DSP's Drive, and
/// puts it in the copy: the Doc Google made.
pub(super) async fn write(
    state: &Arc<State>,
    owner: &Grant,
    parent: &str,
    name: &str,
    html: &str,
) -> Result<Item> {
    let (_, made) = on_drive!(state, owner, |google, token| google
        .write(token, name, parent, html))?;
    confirmed(state, owner, made.clone()).await?;
    Ok(made)
}
/// Writes the page `html` over the Doc `id`, named `name`, and puts it in the copy as it is now.
pub(super) async fn rewrite(
    state: &Arc<State>,
    owner: &Grant,
    id: &str,
    name: &str,
    html: &str,
) -> Result<Item> {
    let (_, written) = on_drive!(state, owner, |google, token| google
        .rewrite(token, id, name, html))?;
    confirmed(state, owner, written.clone()).await?;
    Ok(written)
}

/// Adds the files and folders `ids`, picked with Google's picker, to the folder `parent`, or the
/// main one. One already inside Documents only becomes reachable; one elsewhere in the account's
/// Drive moves in. A folder brings itself only: what it holds is picked on its own.
pub async fn add(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    ids: Vec<String>,
    parent: Option<String>,
    expected: Option<String>,
) -> Result<AddedFiles> {
    let dsp = c.dsp.id.clone();
    let owner = grant::capture_member(state, c, access, expected).await?;
    let tree = access::tree_for(state, &owner).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let drive = access::open_for(state, &owner, false).await?;
    drive.google.grant(&drive.access, &ids).await?;
    let mut added = Vec::new();
    // Whether it made a shortcut to a file someone else owns, to share with the team, and
    // those the account may not share on.
    let mut shortcuts = false;
    let mut unshared = Vec::new();
    // Every pick goes in the copy of the connection captured above, and the sharing after
    // them asks as it: a disconnect meanwhile never fills its replacement's copy.
    for id in &ids {
        // One the account that holds Documents doesn't reach: the member couldn't bring it.
        let item = match drive.google.get(&drive.access, id).await {
            Err(error) if error.code == "documents_item_not_found" => {
                return Err(Error::new("documents_picked_elsewhere", 409));
            }
            found => found?,
        };
        // The main folder is Documents itself.
        if item.id == tree.root() {
            continue;
        }
        let inside = item.parents.iter().any(|folder| tree.is_folder(folder));
        if inside {
            confirmed(state, &owner, item.clone()).await?;
            added.push(item);
            continue;
        }
        // One shared with the account already has its shortcut in Documents.
        if tree.stands_for(&item.id) {
            continue;
        }
        // Only its owner moves a file. Documents gets a shortcut to one shared with the
        // account, as Drive's own "Add shortcut" does, and to one Google won't move.
        let moved = match item.owned_by_me {
            true => drive.google.move_into(&drive.access, &item, &parent).await,
            false => Err(Error::new("documents_cannot_move", 409)),
        };
        let (item, file) = match moved {
            Err(error) if error.code == "documents_cannot_move" => {
                shortcuts = true;
                if !item.capabilities.can_share {
                    unshared.push(item.name.clone());
                }
                owner.admit(state).await?;
                let made = drive.google.shortcut(&drive.access, &item, &parent).await?;
                (made, Some(item))
            }
            moved => (moved?, None),
        };
        // The shortcut first, then the file it stands for, which stays beside it.
        let confirmation = item.clone();
        owner
            .read(state, move |_, owner| {
                match file {
                    Some(file) => owner
                        .mirror
                        .put_many(&owner.connection.folder_id, [confirmation, file]),
                    None => owner.mirror.put(&owner.connection.folder_id, confirmation),
                }
                Ok(())
            })
            .await?;
        added.push(item);
    }
    let named: Vec<(String, String)> = added
        .iter()
        .map(|item| (item.id.clone(), item.name.clone()))
        .collect();
    let at = dsp.clone();
    owner
        .run_member(state, move |db, c| {
            for (id, name) in &named {
                db.record_documents_change(&at, id, c.actor())?;
                c.audit(db, "documents.added", name)?;
            }
            Ok(())
        })
        .await?;
    if shortcuts {
        team::share_soon(state, &owner);
    }
    let connection = owner.connection.clone();
    let tree = access::tree_for(state, &owner).await?;
    let ids: Vec<String> = added.iter().map(|item| item.id.clone()).collect();
    let at = dsp.clone();
    owner
        .read(state, move |db, owner| {
            let (records, names) = known(db, &at, &ids)?;
            let described = Described {
                owner: &owner.key,
                tree: &tree,
                records: &records,
                names: &names,
                account: &connection.account.email,
            };
            let items = added
                .into_iter()
                .map(|item| match tree.get(&item.id) {
                    Some(row) => described.item(row, false),
                    None => described.item(&Row::from(item), false),
                })
                .collect();
            Ok(AddedFiles { items, unshared })
        })
        .await
}

/// The file `id`, to save, as Google sends it.
pub async fn download(state: &Arc<State>, c: &Context, id: String) -> Result<Download> {
    let owner = grant::capture_member(state, c.clone(), Dsp("documents.use"), None).await?;
    let tree = access::tree_for(state, &owner).await?;
    let item = tree
        .get(&id)
        .filter(|item| !item.folder())
        .ok_or_else(not_found)?;
    let answer = on_drive!(state, &owner, |google, token| google.download(
        token,
        item.target.as_deref().unwrap_or(&item.id),
        &item.name,
        item.mime.as_str()
    ));
    let (_, download) = gone(state, &owner, &id, answer).await?;
    Ok(download)
}

/// Google's picture of what the file `id` holds, `size` pixels on its longest side, by the
/// link the DSP's Documents showed it with.
pub async fn picture(
    state: &Arc<State>,
    c: &Context,
    id: String,
    size: u32,
) -> Result<drive::Picture> {
    let owner = grant::capture_member(state, c.clone(), Dsp("documents.use"), None).await?;
    let id_for_cache = id.clone();
    let shown = owner
        .read(state, move |_, owner| {
            shown(&owner.key, &id_for_cache).ok_or_else(not_found)
        })
        .await?;
    let drive = access::open_for(state, &owner, false).await?;
    let link = if shown.given.elapsed() < PICTURE_LINK_LASTS {
        shown.link
    } else {
        let answer = drive.google.get(&drive.access, &id).await;
        let item = gone(state, &owner, &id, answer).await?;
        let link = item.thumbnail_link.clone().ok_or_else(not_found)?;
        let observed_link = link.clone();
        owner
            .read(state, move |_, owner| {
                super::cache::observed(&owner.key, &id, &observed_link, item.seen);
                owner.mirror.put(&owner.connection.folder_id, item);
                Ok(())
            })
            .await?;
        link
    };
    owner.admit(state).await?;
    let picture = drive.google.picture(&drive.access, &link, size).await?;
    access::current(state, &owner).await?;
    Ok(picture)
}

/// Gives the file or folder `id` the name `name`.
pub async fn rename(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: String,
    name: String,
    expected: Option<String>,
) -> Result<DocumentsItem> {
    let dsp = c.dsp.id.clone();
    let owner = grant::capture_member(state, c, access, expected).await?;
    let tree = access::tree_for(state, &owner).await?;
    let item = tree.get(&id).ok_or_else(not_found)?;
    let before = item.name.to_string();
    // A file keeps its extension, so renaming never changes what kind of file it is.
    if let Some(kept) = extension(&before, item.mime.as_str()) {
        let same = extension(&name, item.mime.as_str()) == Some(kept);
        ensure(same, "documents_extension_kept", 400)?;
    }
    let answer = on_drive!(state, &owner, |google, token| google
        .rename(token, &id, &name));
    let (connection, renamed) = gone(state, &owner, &id, answer).await?;
    confirmed(state, &owner, renamed.clone()).await?;
    let after = renamed.name.clone();
    let at = dsp.clone();
    let (records, names) = owner
        .run_member(state, move |db, c| {
            db.record_documents_change(&at, &id, c.actor())?;
            db.audit_with(
                Some(c.actor()),
                Some(&at),
                "documents.renamed",
                &after,
                None,
                &[("name", Some(before), Some(after.clone()))],
            )?;
            known(db, &at, &[id])
        })
        .await?;
    let captured_tree = tree.clone();
    owner
        .read(state, move |_, owner| {
            Ok(Described {
                owner: &owner.key,
                tree: &captured_tree,
                records: &records,
                names: &names,
                account: &connection.account.email,
            }
            .item(&Row::from(renamed), false))
        })
        .await
}

/// Moves the file or folder `id`, and whatever it holds, into the folder `into`, or the main
/// one. A folder never goes inside itself.
pub async fn move_to(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: String,
    into: Option<String>,
    expected: Option<String>,
) -> Result<MovedItem> {
    let dsp = c.dsp.id.clone();
    let owner = grant::capture_member(state, c, access, expected).await?;
    let connection = owner.connection.clone();
    let tree = access::tree_for(state, &owner).await?;
    let root = connection.folder_id;
    let into = into.unwrap_or_else(|| root.clone());
    let row = tree.get(&id).ok_or_else(not_found)?;
    ensure(tree.is_folder(&into), "documents_item_not_found", 404)?;
    let inside = tree.path(&into).iter().any(|up| *up.id == *id);
    ensure(!inside, "documents_move_inside", 409)?;
    let name = row.shown().name.to_string();
    // Someone may have moved it in Drive since the copy last heard: Google says where it is now.
    let answer = on_drive!(state, &owner, |google, token| google.get(token, &id));
    let (_, now) = gone(state, &owner, &id, answer).await?;
    let Some(from) = now
        .parents
        .iter()
        .find(|parent| tree.is_folder(parent))
        .cloned()
    else {
        confirmed(state, &owner, now).await?;
        return Err(not_found());
    };
    let moved = if from == into {
        now
    } else {
        // Drive keeps a file in one folder, and only its owner moves it to another.
        ensure(now.owned_by_me, "documents_cannot_move", 409)?;
        let answer = on_drive!(state, &owner, |google, token| google
            .move_into(token, &now, &into));
        let (_, moved) = answer?;
        confirmed(state, &owner, moved.clone()).await?;
        let named = |folder: &str| match tree.get(folder) {
            Some(row) => row.name.to_string(),
            None => "Documents".to_owned(),
        };
        let (before, after) = (named(&from), named(&into));
        let at = dsp.clone();
        let title = name.clone();
        owner
            .run_member(state, move |db, c| {
                db.audit_with(
                    Some(c.actor()),
                    Some(&at),
                    "documents.moved",
                    &title,
                    None,
                    &[("folder", Some(before), Some(after))],
                )
            })
            .await?;
        moved
    };
    let connection = owner.connection.clone();
    let tree = access::tree_for(state, &owner).await?;
    let at = dsp.clone();
    owner
        .read(state, move |db, owner| {
            let (records, names) = known(db, &at, &[id])?;
            let described = Described {
                owner: &owner.key,
                tree: &tree,
                records: &records,
                names: &names,
                account: &connection.account.email,
            };
            let item = match tree.get(&moved.id) {
                Some(row) => described.item(row, false),
                None => described.item(&Row::from(moved), false),
            };
            Ok(MovedItem {
                item,
                from: (from != root).then_some(from),
            })
        })
        .await
}

/// Moves the file or folder `id`, and whatever it holds, to the account's trash.
pub async fn trash(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: String,
    expected: Option<String>,
) -> Result<()> {
    let owner = grant::capture_member(state, c, access, expected).await?;
    let tree = access::tree_for(state, &owner).await?;
    let row = tree.get(&id).ok_or_else(not_found)?;
    let name = row.name.to_string();
    // A shortcut's file stops being shared with the team as the shortcut goes.
    let shortcut = row.target.is_some();
    // Someone may have moved it out of Documents in Drive since the copy last heard: Google
    // says where it is now.
    let answer = on_drive!(state, &owner, |google, token| google.get(token, &id));
    let (_, now) = gone(state, &owner, &id, answer).await?;
    if !now.parents.iter().any(|parent| tree.is_folder(parent)) {
        confirmed(state, &owner, now).await?;
        return Err(not_found());
    }
    let answer = on_drive!(state, &owner, |google, token| google.trash(token, &id));
    gone(state, &owner, &id, answer).await?;
    let removed = id.clone();
    owner
        .read(state, move |_, owner| {
            owner.mirror.remove(&owner.connection.folder_id, &removed);
            Ok(())
        })
        .await?;
    if shortcut {
        team::share_soon(state, &owner);
    }
    owner
        .run_member(state, move |db, c| c.audit(db, "documents.trashed", &name))
        .await
}

#[cfg(test)]
#[path = "../tests/backend/folder_reads.rs"]
mod tests;

/// Shared preparation for the ephemeral query protocol, its pages and its descriptions.
/// `folder` and `folder_read` read through access::tree_for and the complete reader.
/// Captured grant ownership is independent of semantic snapshots.
pub(crate) struct QueryRequest {
    pub scope: crate::api::types::DocumentsQueryScope,
    pub page: u32,
    pub size: u32,
    pub snapshot: Option<String>,
    pub manifest_since: Option<String>,
    pub since: Option<String>,
}
async fn query_read(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    request: QueryRequest,
    descriptions: Option<Vec<String>>,
) -> Result<QueryAnswer> {
    use super::query;
    use crate::api::types::{
        DocumentsDescriptionRead, DocumentsQueryPage, DocumentsQueryRead, DocumentsResetReason,
    };
    use dispatch_core::server::cache::Scope;
    use std::sync::atomic::Ordering;
    let QueryRequest {
        scope,
        snapshot,
        page,
        size,
        manifest_since,
        since,
    } = request;
    let scope = query::normalized(scope);
    ensure((1..=100).contains(&size), "invalid_input", 400)?;
    if let Some(ids) = &descriptions {
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        ensure(
            !ids.is_empty() && ids.len() <= 100 && unique.len() == ids.len(),
            "invalid_input",
            400,
        )?;
    }
    let captured = grant::capture_member(state, c.clone(), access, None).await?;
    access::tree_for(state, &captured).await?;
    let owner = Arc::clone(&captured.mirror);
    let connection = captured.connection.clone();
    // Serialize openings before database admission; a cancelled caller's worker keeps its
    // turn until the calculation ends. Continuations have no builder and need no keyed turn.
    let turn = if snapshot.is_none() {
        Some(
            state
                .read_turns
                .enter(query::digest(&(
                    "documents-query-turn-v1",
                    &c.dsp.id,
                    owner.incarnation(),
                    &connection.folder_id,
                    &scope.id,
                    &scope.q,
                ))?)
                .await?,
        )
    } else {
        None
    };
    let shared = Arc::clone(state);
    state
        .read(move |db| {
            let _turn = turn;
            captured.check(db)?;
            let reset = |reason| match descriptions.as_ref() {
                Some(_) => QueryAnswer::Descriptions(DocumentsDescriptionRead::Reset { reason }),
                None => QueryAnswer::Page(DocumentsQueryRead::Reset { reason }),
            };
            let Some((tree, semantic)) = owner.snapshot(&connection.folder_id) else {
                return Ok(reset(DocumentsResetReason::Changed));
            };
            if !mirror::current(&captured.key, &owner) || !owner.matches(tree.root(), semantic) {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            let at = scope.id.as_deref().unwrap_or(tree.root());
            if !tree.is_folder(at) {
                if snapshot.is_some() {
                    return Ok(reset(DocumentsResetReason::Changed));
                }
                return Err(not_found());
            }
            // Persisted grant identity, mirror incarnation, and semantic sequence have separate
            // lifetimes. Process identity still prevents ephemeral continuations across restart.
            let key = query::digest(&(
                "documents-query-index-v1",
                query::process()?,
                shared.config.environment_root(),
                &c.dsp.id,
                owner.incarnation(),
                semantic,
                &connection.folder_id,
                (
                    &connection.generation,
                    connection.broken,
                    connection.full,
                    &connection.account.email,
                    connection.account.workspace,
                    &connection.folder_name,
                    &connection.connected_by,
                    &connection.connected_at,
                ),
                &scope.id,
                &scope.q,
            ))?;
            let expected = query::digest(&("documents-query-snapshot-v1", &key, scope.filter))?;
            if snapshot
                .as_ref()
                .is_some_and(|snapshot| *snapshot != expected)
            {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            // Global revision, never the mirror sequence: core prunes unequal revisions globally.
            let global = shared.data_revision.load(Ordering::Relaxed);
            let Some(index) = query::index(
                &shared.read_cache,
                Scope::tenant(query::CACHED, &c.dsp.id),
                key,
                global,
                &tree,
                (at, scope.q.as_deref()),
                snapshot.is_none(),
            )?
            else {
                return Ok(reset(DocumentsResetReason::Missing));
            };
            let answer = if let Some(ids) = &descriptions {
                ensure(
                    ids.iter().all(|id| index.contains(scope.filter, id)),
                    "documents_item_not_found",
                    404,
                )?;
                QueryAnswer::Descriptions(DocumentsDescriptionRead::Full {
                    snapshot: expected,
                    items: query::describe(
                        db,
                        &captured.key,
                        &connection.account.email,
                        &tree,
                        ids,
                        scope.q.is_some(),
                    )?,
                })
            } else {
                let (page, ids) = index.page(scope.filter, page, size);
                let reading = DocumentsQueryPage {
                    grant_generation: connection.generation.clone(),
                    snapshot: expected,
                    manifest_revision: index.manifest_revision.clone(),
                    page,
                    page_size: size,
                    total: index.total(scope.filter),
                    unfiltered_total: index.counts().all,
                    counts: index.counts(),
                    folder: query::header(&tree, at, &scope, &connection.folder_name),
                    items: query::describe(
                        db,
                        &captured.key,
                        &connection.account.email,
                        &tree,
                        &ids,
                        scope.q.is_some(),
                    )?,
                };
                let content_revision = query::digest(&("documents-query-page-v1", &reading))?;
                let same_manifest =
                    manifest_since.as_deref() == Some(index.manifest_revision.as_str());
                QueryAnswer::Page(
                    if same_manifest && since.as_deref() == Some(content_revision.as_str()) {
                        DocumentsQueryRead::Unchanged {
                            snapshot: reading.snapshot,
                            manifest_revision: reading.manifest_revision,
                            content_revision,
                        }
                    } else {
                        DocumentsQueryRead::Full {
                            reading: Box::new(reading),
                            manifest: (!same_manifest).then(|| index.manifest.clone()),
                            content_revision,
                        }
                    },
                )
            };
            // Synchronous publications may run while this worker projects. No awaits follow this
            // check, and no mixed old manifest/current tree answer is allowed out.
            if !mirror::current(&captured.key, &owner) || !owner.matches(tree.root(), semantic) {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            Ok(answer)
        })
        .await
}

enum QueryAnswer {
    Page(crate::api::types::DocumentsQueryRead),
    Descriptions(crate::api::types::DocumentsDescriptionRead),
}
pub async fn query(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    request: QueryRequest,
) -> Result<crate::api::types::DocumentsQueryRead> {
    match query_read(state, c, access, request, None).await? {
        QueryAnswer::Page(answer) => Ok(answer),
        QueryAnswer::Descriptions(_) => unreachable!("page request"),
    }
}
pub async fn describe(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    scope: crate::api::types::DocumentsQueryScope,
    snapshot: String,
    ids: Vec<String>,
) -> Result<crate::api::types::DocumentsDescriptionRead> {
    let request = QueryRequest {
        scope,
        snapshot: Some(snapshot),
        page: 0,
        size: 100,
        manifest_since: None,
        since: None,
    };
    match query_read(state, c, access, request, Some(ids)).await? {
        QueryAnswer::Descriptions(answer) => Ok(answer),
        QueryAnswer::Page(_) => unreachable!("description request"),
    }
}
