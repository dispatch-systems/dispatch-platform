//! Documents file commands. Reads use the DSP's in-memory Drive; changes ask Google outside
//! the database, inside the DSP's own tree only, put Google's answer in the copy, then check
//! the member's access again before recording anything.
use super::{
    access::{self, on_drive},
    cache::{PICTURE_LINK_LASTS, shown},
    description::{Described, extension, known},
    drive::{self, Download, Item},
    google::{FOLDER, folder_url},
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
fn gone<T>(dsp: &str, root: &str, id: &str, answer: Result<T>) -> Result<T> {
    if answer
        .as_ref()
        .is_err_and(|error| error.code == "documents_item_not_found")
    {
        mirror::of(dsp).remove(root, id);
    }
    answer
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
    let (connection, tree) = access::tree(state, &c.dsp.id).await?;
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
    let dsp = c.dsp.id.clone();
    let (records, names) = state.read(move |db| known(db, &dsp, &ids)).await?;
    let described = Described {
        dsp: &c.dsp.id,
        tree: &tree,
        records: &records,
        names: &names,
        account: &connection.account.email,
    };
    let folder = DocumentsFolder {
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
    Ok((connection, folder))
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
) -> Result<DocumentsItem> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = access::tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let mime = match kind {
        NewKind::Folder => FOLDER,
        NewKind::Doc => drive::DOC,
        NewKind::Sheet => drive::SHEET,
        NewKind::Slides => drive::SLIDES,
    };
    let (connection, made) = make(state, &dsp, &parent, mime, &name).await?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let at = dsp.clone();
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.record_documents_change(&at, &id, c.actor())?;
            c.audit(db, "documents.created", &title)?;
            known(db, &at, &[id])
        })
        .await?;
    // A file just made holds nothing, so an empty tree counts it right.
    let empty = Tree::new(&connection.folder_id, Vec::new());
    Ok(Described {
        dsp: &dsp,
        tree: &empty,
        records: &records,
        names: &names,
        account: &connection.account.email,
    }
    .item(&Row::from(made), false))
}

/// Uploads a file named `name`, of `mime`, into the folder `parent`, or the main one.
pub async fn upload(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    parent: Option<String>,
    name: String,
    mime: String,
    upload: Upload,
) -> Result<DocumentsItem> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = access::tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let (connection, made) = put(state, &dsp, &parent, &name, &mime, upload).await?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let at = dsp.clone();
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.record_documents_change(&at, &id, c.actor())?;
            c.audit(db, "documents.uploaded", &title)?;
            known(db, &at, &[id])
        })
        .await?;
    let empty = Tree::new(&connection.folder_id, Vec::new());
    Ok(Described {
        dsp: &dsp,
        tree: &empty,
        records: &records,
        names: &names,
        account: &connection.account.email,
    }
    .item(&Row::from(made), false))
}

/// Makes a folder, Doc, Sheet or Slides named `name`, of `mime`, in the folder `parent` of the
/// DSP's Drive, and puts it in the copy: the connection, and what Google made.
pub(crate) async fn make(
    state: &Arc<State>,
    dsp: &str,
    parent: &str,
    mime: &str,
    name: &str,
) -> Result<(Connection, Item)> {
    let (connection, made) = on_drive!(state, dsp, |google, token| google.create(
        token,
        name,
        mime,
        Some(parent)
    ))?;
    mirror::of(dsp).put(&connection.folder_id, made.clone());
    Ok((connection, made))
}
/// Streams `upload`, named `name`, of `mime`, into the folder `parent` of the DSP's Drive, and
/// puts it in the copy: the connection, and the file Google made.
pub(crate) async fn put(
    state: &Arc<State>,
    dsp: &str,
    parent: &str,
    name: &str,
    mime: &str,
    upload: Upload,
) -> Result<(Connection, Item)> {
    let length = upload.length;
    let (_, session) = on_drive!(state, dsp, |google, token| google
        .start_upload(token, name, mime, parent, length))?;
    // The bytes go once, with the token that just started the upload.
    let drive = access::open(state, dsp, false).await?;
    let made = drive
        .google
        .finish_upload(&drive.access, &session, upload)
        .await?;
    mirror::of(dsp).put(&drive.connection.folder_id, made.clone());
    Ok((drive.connection, made))
}

/// Writes the page `html` as a Doc named `name` in the folder `parent` of the DSP's Drive, and
/// puts it in the copy: the Doc Google made.
pub(crate) async fn write(
    state: &Arc<State>,
    dsp: &str,
    parent: &str,
    name: &str,
    html: &str,
) -> Result<Item> {
    let (connection, made) = on_drive!(state, dsp, |google, token| google
        .write(token, name, parent, html))?;
    mirror::of(dsp).put(&connection.folder_id, made.clone());
    Ok(made)
}
/// Writes the page `html` over the Doc `id`, named `name`, and puts it in the copy as it is now.
pub(crate) async fn rewrite(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    name: &str,
    html: &str,
) -> Result<Item> {
    let (connection, written) = on_drive!(state, dsp, |google, token| google
        .rewrite(token, id, name, html))?;
    mirror::of(dsp).put(&connection.folder_id, written.clone());
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
) -> Result<AddedFiles> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = access::tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let drive = access::open(state, &dsp, false).await?;
    drive.google.grant(&drive.access, &ids)?;
    let mut added = Vec::new();
    // Keep this connection's mirror: a disconnect must not repopulate its replacement.
    let mirror = mirror::of(&dsp);
    // Whether it made a shortcut to a file someone else owns, to share with the team, and
    // those the account may not share on.
    let mut shortcuts = false;
    let mut unshared = Vec::new();
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
            mirror.put(&drive.connection.folder_id, item.clone());
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
                let made = drive.google.shortcut(&drive.access, &item, &parent).await?;
                (made, Some(item))
            }
            moved => (moved?, None),
        };
        // The shortcut first, then the file it stands for, which stays beside it.
        match file {
            Some(file) => mirror.put_many(&drive.connection.folder_id, [item.clone(), file]),
            None => mirror.put(&drive.connection.folder_id, item.clone()),
        }
        added.push(item);
    }
    let named: Vec<(String, String)> = added
        .iter()
        .map(|item| (item.id.clone(), item.name.clone()))
        .collect();
    let at = dsp.clone();
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            for (id, name) in &named {
                db.record_documents_change(&at, id, c.actor())?;
                c.audit(db, "documents.added", name)?;
            }
            Ok(())
        })
        .await?;
    if shortcuts {
        team::share_soon(state, &dsp);
    }
    let (connection, tree) = access::tree(state, &dsp).await?;
    let ids: Vec<String> = added.iter().map(|item| item.id.clone()).collect();
    let at = dsp.clone();
    let (records, names) = state.read(move |db| known(db, &at, &ids)).await?;
    let described = Described {
        dsp: &dsp,
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
}

/// The file `id`, to save, as Google sends it.
pub async fn download(state: &Arc<State>, c: &Context, id: String) -> Result<Download> {
    let (connection, tree) = access::tree(state, &c.dsp.id).await?;
    let item = tree
        .get(&id)
        .filter(|item| !item.folder())
        .ok_or_else(not_found)?;
    let answer = on_drive!(state, &c.dsp.id, |google, token| google.download(
        token,
        item.target.as_deref().unwrap_or(&item.id),
        &item.name,
        item.mime.as_str()
    ));
    let (_, download) = gone(&c.dsp.id, &connection.folder_id, &id, answer)?;
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
    let shown = shown(&c.dsp.id, &id).ok_or_else(not_found)?;
    let drive = access::open(state, &c.dsp.id, false).await?;
    let link = if shown.given.elapsed() < PICTURE_LINK_LASTS {
        shown.link
    } else {
        // The file was shown from a copy older than its link lasts: Google gives a new one.
        let answer = drive.google.get(&drive.access, &id).await;
        let item = gone(&c.dsp.id, &drive.connection.folder_id, &id, answer)?;
        let link = item.thumbnail_link.clone().ok_or_else(not_found)?;
        mirror::of(&c.dsp.id).put(&drive.connection.folder_id, item);
        link
    };
    drive.google.picture(&drive.access, &link, size).await
}

/// Gives the file or folder `id` the name `name`.
pub async fn rename(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: String,
    name: String,
) -> Result<DocumentsItem> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = access::tree(state, &dsp).await?;
    let item = tree.get(&id).ok_or_else(not_found)?;
    let before = item.name.to_string();
    // A file keeps its extension, so renaming never changes what kind of file it is.
    if let Some(kept) = extension(&before, item.mime.as_str()) {
        let same = extension(&name, item.mime.as_str()) == Some(kept);
        ensure(same, "documents_extension_kept", 400)?;
    }
    let answer = on_drive!(state, &dsp, |google, token| google
        .rename(token, &id, &name));
    let (connection, renamed) = gone(&dsp, tree.root(), &id, answer)?;
    mirror::of(&dsp).put(&connection.folder_id, renamed.clone());
    let after = renamed.name.clone();
    let at = dsp.clone();
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
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
    Ok(Described {
        dsp: &dsp,
        tree: &tree,
        records: &records,
        names: &names,
        account: &connection.account.email,
    }
    .item(&Row::from(renamed), false))
}

/// Moves the file or folder `id`, and whatever it holds, into the folder `into`, or the main
/// one. A folder never goes inside itself.
pub async fn move_to(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    id: String,
    into: Option<String>,
) -> Result<MovedItem> {
    let dsp = c.dsp.id.clone();
    let (connection, tree) = access::tree(state, &dsp).await?;
    let root = connection.folder_id;
    let into = into.unwrap_or_else(|| root.clone());
    let row = tree.get(&id).ok_or_else(not_found)?;
    ensure(tree.is_folder(&into), "documents_item_not_found", 404)?;
    let inside = tree.path(&into).iter().any(|up| *up.id == *id);
    ensure(!inside, "documents_move_inside", 409)?;
    let name = row.shown().name.to_string();
    // Someone may have moved it in Drive since the copy last heard: Google says where it is now.
    let answer = on_drive!(state, &dsp, |google, token| google.get(token, &id));
    let (_, now) = gone(&dsp, &root, &id, answer)?;
    let Some(from) = now
        .parents
        .iter()
        .find(|parent| tree.is_folder(parent))
        .cloned()
    else {
        mirror::of(&dsp).put(&root, now);
        return Err(not_found());
    };
    let moved = if from == into {
        now
    } else {
        // Drive keeps a file in one folder, and only its owner moves it to another.
        ensure(now.owned_by_me, "documents_cannot_move", 409)?;
        let answer = on_drive!(state, &dsp, |google, token| google
            .move_into(token, &now, &into));
        let (_, moved) = answer?;
        mirror::of(&dsp).put(&root, moved.clone());
        let named = |folder: &str| match tree.get(folder) {
            Some(row) => row.name.to_string(),
            None => "Documents".to_owned(),
        };
        let (before, after) = (named(&from), named(&into));
        let at = dsp.clone();
        let title = name.clone();
        state
            .run(move |db| {
                let c = access.revalidate(db, &c)?;
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
    let (connection, tree) = access::tree(state, &dsp).await?;
    let at = dsp.clone();
    let (records, names) = state.read(move |db| known(db, &at, &[id])).await?;
    let described = Described {
        dsp: &dsp,
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
}

/// Moves the file or folder `id`, and whatever it holds, to the account's trash.
pub async fn trash(state: &Arc<State>, c: Context, access: Dsp, id: String) -> Result<()> {
    let dsp = c.dsp.id.clone();
    let (connection, tree) = access::tree(state, &dsp).await?;
    let root = connection.folder_id;
    let row = tree.get(&id).ok_or_else(not_found)?;
    let name = row.name.to_string();
    // A shortcut's file stops being shared with the team as the shortcut goes.
    let shortcut = row.target.is_some();
    // Someone may have moved it out of Documents in Drive since the copy last heard: Google
    // says where it is now.
    let answer = on_drive!(state, &dsp, |google, token| google.get(token, &id));
    let (_, now) = gone(&dsp, &root, &id, answer)?;
    if !now.parents.iter().any(|parent| tree.is_folder(parent)) {
        mirror::of(&dsp).put(&root, now);
        return Err(not_found());
    }
    let answer = on_drive!(state, &dsp, |google, token| google.trash(token, &id));
    gone(&dsp, &root, &id, answer)?;
    mirror::of(&dsp).remove(&root, &id);
    if shortcut {
        team::share_soon(state, &dsp);
    }
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            c.audit(db, "documents.trashed", &name)
        })
        .await
}

#[cfg(test)]
#[path = "../tests/backend/folder_reads.rs"]
mod tests;

/// Shared preparation for the additive, ephemeral query protocol. Legacy callers retain
/// access::tree and the complete reader. No reconnect/durable-generation guarantee is added.
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
    let owner = mirror::of(&c.dsp.id);
    let (connection, _) = access::tree(state, &c.dsp.id).await?;
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
            let c = access.revalidate(db, &c)?;
            let current = db
                .documents_connection(&c.dsp.id)?
                .ok_or_else(|| Error::new("documents_not_connected", 409))?;
            ensure(!current.broken, "google_connection_broken", 409)?;
            ensure(current == connection, "documents_not_connected", 409)?;
            let reset = |reason| match descriptions.as_ref() {
                Some(_) => QueryAnswer::Descriptions(DocumentsDescriptionRead::Reset { reason }),
                None => QueryAnswer::Page(DocumentsQueryRead::Reset { reason }),
            };
            let Some((tree, semantic)) = owner.snapshot(&connection.folder_id) else {
                return Ok(reset(DocumentsResetReason::Changed));
            };
            if !mirror::current(&c.dsp.id, &owner) || !owner.matches(tree.root(), semantic) {
                return Ok(reset(DocumentsResetReason::Changed));
            }
            let at = scope.id.as_deref().unwrap_or(tree.root());
            if !tree.is_folder(at) {
                if snapshot.is_some() {
                    return Ok(reset(DocumentsResetReason::Changed));
                }
                return Err(not_found());
            }
            // Every stored connection field participates; this is an ephemeral equality fence,
            // not proof against reconnect ABA. Process identity prevents cross-restart reuse.
            let key = query::digest(&(
                "documents-query-index-v1",
                query::process()?,
                shared.config.environment_root(),
                &c.dsp.id,
                owner.incarnation(),
                semantic,
                &connection.folder_id,
                (
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
                        &c.dsp.id,
                        &connection.account.email,
                        &tree,
                        ids,
                        scope.q.is_some(),
                    )?,
                })
            } else {
                let (page, ids) = index.page(scope.filter, page, size);
                let reading = DocumentsQueryPage {
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
                        &c.dsp.id,
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
                            reading,
                            manifest: (!same_manifest).then(|| index.manifest.clone()),
                            content_revision,
                        }
                    },
                )
            };
            // Synchronous publications may run while this worker projects. No awaits follow this
            // check, and no mixed old manifest/current tree answer is allowed out.
            if !mirror::current(&c.dsp.id, &owner) || !owner.matches(tree.root(), semantic) {
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
