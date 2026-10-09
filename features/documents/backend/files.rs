//! Browsing and changing a DSP's Documents: what a folder holds, and making, uploading,
//! downloading, renaming and trashing files in it. Each asks Google outside the database, inside the DSP's own tree
//! only, then checks the member's access again before it records anything.
use super::{
    connection,
    drive::{self, Download, Item},
    google::{FOLDER, Google, folder_url},
    storage::{Connection, DocumentsStore, Record},
    tree::Tree,
};
use crate::api::types::{DocumentsFolder, DocumentsItem, FolderStep, ItemKind, NewKind};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::Store,
    ensure,
    server::http::{route::Dsp, upload::Upload},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

/// The access tokens Google gave, by DSP, until a minute before each lapses.
static ACCESS: LazyLock<Mutex<BTreeMap<String, (String, Instant)>>> =
    LazyLock::new(Default::default);
fn kept(dsp: &str) -> Option<String> {
    let tokens = ACCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    tokens
        .get(dsp)
        .filter(|(_, until)| Instant::now() < *until)
        .map(|(token, _)| token.clone())
}
/// Forgets the DSP's access token, as a disconnect or a new account does.
pub fn forget(dsp: &str) {
    ACCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(dsp);
}

/// What a call to the DSP's Drive works with.
pub(crate) struct Drive {
    pub google: Google,
    pub connection: Connection,
    pub access: String,
}
/// The DSP's connection, and an access token for it: the one kept unless `fresh`. A
/// connection Google stopped accepting is marked broken, as the hourly check marks it.
pub(crate) async fn open(state: &Arc<State>, dsp: &str, fresh: bool) -> Result<Drive> {
    let google = Google::of(&state.config)?;
    let at = dsp.to_owned();
    let (connection, refresh) = state
        .read(move |db| {
            Ok((
                db.documents_connection(&at)?,
                db.documents_refresh_token(&at)?,
            ))
        })
        .await?;
    let connection = connection.ok_or_else(|| Error::new("documents_not_connected", 409))?;
    let (false, Some(refresh)) = (connection.broken, refresh) else {
        return Err(Error::new("google_connection_broken", 409));
    };
    if let Some(access) = kept(dsp).filter(|_| !fresh) {
        return Ok(Drive {
            google,
            connection,
            access,
        });
    }
    match google.access(&refresh).await {
        Ok(access) => {
            let until = Instant::now() + access.lasts.saturating_sub(Duration::from_secs(60));
            ACCESS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(dsp.to_owned(), (access.token.clone(), until));
            Ok(Drive {
                google,
                connection,
                access: access.token,
            })
        }
        Err(error) => {
            if error.code == "google_connection_broken" {
                let at = dsp.to_owned();
                state.run(move |db| connection::broken(db, &at)).await?;
            }
            Err(error)
        }
    }
}
/// Runs `$call` on the DSP's Drive, with `$google` and its access token `$token`; once more
/// with a fresh token if Google refused the kept one, since an access token lapses early when
/// the account signs out everywhere. Answers the connection with what the call answered.
macro_rules! on_drive {
    ($state:expr, $dsp:expr, |$google:ident, $token:ident| $call:expr) => {{
        let drive = open($state, $dsp, false).await?;
        let ($google, $token) = (&drive.google, drive.access.as_str());
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                forget($dsp);
                let drive = open($state, $dsp, true).await?;
                let ($google, $token) = (&drive.google, drive.access.as_str());
                $call.await.map(|value| (drive.connection, value))
            }
            answer => answer.map(|value| (drive.connection, value)),
        }
    }};
}
/// The DSP's Documents, as Drive holds it now.
async fn tree(state: &Arc<State>, dsp: &str) -> Result<(Connection, Tree)> {
    let (connection, all) = on_drive!(state, dsp, |google, token| google.list(token))?;
    let tree = Tree::new(&connection.folder_id, all);
    Ok((connection, tree))
}
fn not_found() -> Error {
    Error::new("documents_item_not_found", 404)
}

/// The folder `id` of the DSP's Documents, or its main folder, with what it holds; with
/// `query`, everything inside it whose name holds its words.
pub async fn folder(
    state: &Arc<State>,
    c: &Context,
    id: Option<String>,
    query: Option<String>,
) -> Result<DocumentsFolder> {
    let (connection, tree) = tree(state, &c.dsp.id).await?;
    let at = id.clone().unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&at), "documents_item_not_found", 404)?;
    let path: Vec<FolderStep> = tree
        .path(&at)
        .into_iter()
        .map(|step| FolderStep {
            id: step.id.clone(),
            name: step.name.clone(),
        })
        .collect();
    let (shown, searched) = match query.as_deref().map(str::trim) {
        Some(words) if !words.is_empty() => (tree.search(&at, words), true),
        _ => (tree.children(&at), false),
    };
    let ids: Vec<String> = shown.iter().map(|item| item.id.clone()).collect();
    let dsp = c.dsp.id.clone();
    let (records, names) = state.read(move |db| known(db, &dsp, &ids)).await?;
    let described = Described {
        tree: &tree,
        records: &records,
        names: &names,
        account: &connection.account.email,
    };
    Ok(DocumentsFolder {
        name: path
            .last()
            .map_or_else(|| connection.folder_name.clone(), |step| step.name.clone()),
        url: folder_url(&at),
        id,
        path,
        items: shown
            .into_iter()
            .map(|item| described.item(item, searched))
            .collect(),
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
    let (_, tree) = tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let mime = match kind {
        NewKind::Folder => FOLDER,
        NewKind::Doc => drive::DOC,
        NewKind::Sheet => drive::SHEET,
        NewKind::Slides => drive::SLIDES,
    };
    let (connection, made) = on_drive!(state, &dsp, |google, token| google.create(
        token,
        &name,
        mime,
        Some(&parent)
    ))?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.record_documents_change(&dsp, &id, c.actor())?;
            c.audit(db, "documents.created", &title)?;
            known(db, &dsp, &[id])
        })
        .await?;
    // A file just made holds nothing, so an empty tree counts it right.
    let empty = Tree::new(&connection.folder_id, Vec::new());
    Ok(Described {
        tree: &empty,
        records: &records,
        names: &names,
        account: &connection.account.email,
    }
    .item(&made, false))
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
    let (_, tree) = tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let length = upload.length;
    let (_, session) = on_drive!(state, &dsp, |google, token| google
        .start_upload(token, &name, &mime, &parent, length))?;
    // The bytes go once, with the token that just started the upload.
    let drive = open(state, &dsp, false).await?;
    let made = drive
        .google
        .finish_upload(&drive.access, &session, upload)
        .await?;
    let (id, title) = (made.id.clone(), made.name.clone());
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.record_documents_change(&dsp, &id, c.actor())?;
            c.audit(db, "documents.uploaded", &title)?;
            known(db, &dsp, &[id])
        })
        .await?;
    let empty = Tree::new(&drive.connection.folder_id, Vec::new());
    Ok(Described {
        tree: &empty,
        records: &records,
        names: &names,
        account: &drive.connection.account.email,
    }
    .item(&made, false))
}

/// Adds the files `ids`, picked with Google's picker, to the folder `parent`, or the main one.
/// One already inside Documents only becomes reachable; one elsewhere in the account's Drive
/// moves in.
pub async fn add(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    ids: Vec<String>,
    parent: Option<String>,
) -> Result<Vec<DocumentsItem>> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let drive = open(state, &dsp, false).await?;
    drive.google.grant(&drive.access, &ids)?;
    let mut added = Vec::new();
    for id in &ids {
        // Picked as another Google account, the file was given to that account instead.
        let item = match drive.google.get(&drive.access, id).await {
            Err(error) if error.code == "documents_item_not_found" => {
                return Err(Error::new("documents_picked_elsewhere", 409));
            }
            found => found?,
        };
        ensure(!item.folder(), "invalid_input", 400)?;
        let inside = item.parents.iter().any(|folder| tree.is_folder(folder));
        added.push(if inside {
            item
        } else {
            drive
                .google
                .move_into(&drive.access, &item, &parent)
                .await?
        });
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
    let (connection, tree) = self::tree(state, &dsp).await?;
    let ids: Vec<String> = added.iter().map(|item| item.id.clone()).collect();
    let (records, names) = state.read(move |db| known(db, &dsp, &ids)).await?;
    let described = Described {
        tree: &tree,
        records: &records,
        names: &names,
        account: &connection.account.email,
    };
    Ok(added
        .iter()
        .map(|item| described.item(item, false))
        .collect())
}

/// The file `id`, to save, as Google sends it.
pub async fn download(state: &Arc<State>, c: &Context, id: String) -> Result<Download> {
    let (_, tree) = tree(state, &c.dsp.id).await?;
    let item = tree
        .get(&id)
        .filter(|item| !item.folder())
        .ok_or_else(not_found)?;
    let (_, download) = on_drive!(state, &c.dsp.id, |google, token| google
        .download(token, item))?;
    Ok(download)
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
    let (_, tree) = tree(state, &dsp).await?;
    let before = tree.get(&id).ok_or_else(not_found)?.name.clone();
    let (connection, renamed) = on_drive!(state, &dsp, |google, token| google
        .rename(token, &id, &name))?;
    let after = renamed.name.clone();
    let (records, names) = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.record_documents_change(&dsp, &id, c.actor())?;
            db.audit_with(
                Some(c.actor()),
                Some(&dsp),
                "documents.renamed",
                &after,
                None,
                &[("name", Some(before), Some(after.clone()))],
            )?;
            known(db, &dsp, &[id])
        })
        .await?;
    Ok(Described {
        tree: &tree,
        records: &records,
        names: &names,
        account: &connection.account.email,
    }
    .item(&renamed, false))
}

/// Moves the file or folder `id`, and whatever it holds, to the account's trash.
pub async fn trash(state: &Arc<State>, c: Context, access: Dsp, id: String) -> Result<()> {
    let dsp = c.dsp.id.clone();
    let (_, tree) = tree(state, &dsp).await?;
    let name = tree.get(&id).ok_or_else(not_found)?.name.clone();
    on_drive!(state, &dsp, |google, token| google.trash(token, &id))?;
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            c.audit(db, "documents.trashed", &name)
        })
        .await
}

/// What Dispatch recorded of the files `ids`, with the names of whoever it names.
fn known(
    db: &Store,
    dsp: &str,
    ids: &[String],
) -> Result<(BTreeMap<String, Record>, BTreeMap<String, String>)> {
    let mut records = db.documents_records(dsp)?;
    records.retain(|id, _| ids.contains(id));
    let people: BTreeSet<&str> = records
        .values()
        .flat_map(|record| [record.added_by.as_str(), record.changed_by.as_str()])
        .collect();
    let mut names = BTreeMap::new();
    for user in people {
        if let Some(name) = db.actor_name(dsp, user)? {
            names.insert(user.to_owned(), name);
        }
    }
    Ok((records, names))
}

/// Files as the page shows them.
struct Described<'a> {
    tree: &'a Tree,
    records: &'a BTreeMap<String, Record>,
    names: &'a BTreeMap<String, String>,
    /// The connected account, which Google names for every change Dispatch makes.
    account: &'a str,
}
impl Described<'_> {
    fn item(&self, item: &Item, located: bool) -> DocumentsItem {
        let record = self.records.get(&item.id);
        let named = |user: &String| self.names.get(user).cloned();
        let modifier = item.last_modifying_user.as_ref();
        // Google names the account for what Dispatch did, and the person for what they did
        // in Google.
        let by_account = modifier
            .and_then(|person| person.email_address.as_deref())
            .is_none_or(|email| email.eq_ignore_ascii_case(self.account));
        let modified_by = if by_account {
            record.and_then(|record| named(&record.changed_by))
        } else {
            modifier.and_then(|person| person.display_name.clone())
        };
        DocumentsItem {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: kind(&item.mime_type),
            size: item.size.as_deref().and_then(|size| size.parse().ok()),
            modified_at: item.modified_time.clone(),
            modified_by,
            added_by: record.and_then(|record| named(&record.added_by)),
            items: item
                .folder()
                .then(|| self.tree.children(&item.id).len() as u32),
            url: item.web_view_link.clone(),
            location: item
                .parents
                .first()
                .and_then(|parent| self.tree.get(parent))
                .filter(|_| located)
                .map(|parent| parent.name.clone()),
        }
    }
}

pub(crate) fn kind(mime: &str) -> ItemKind {
    match mime {
        FOLDER => ItemKind::Folder,
        drive::DOC => ItemKind::Doc,
        drive::SHEET => ItemKind::Sheet,
        drive::SLIDES => ItemKind::Slides,
        "application/pdf" => ItemKind::Pdf,
        image if image.starts_with("image/") => ItemKind::Image,
        _ => ItemKind::File,
    }
}
