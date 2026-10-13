//! Browsing and changing a DSP's Documents: what a folder holds, and making, uploading,
//! downloading, renaming and trashing files in it. A folder opens from the copy of the DSP's
//! Drive kept in memory; a change asks Google outside the database, inside the DSP's own tree
//! only, puts Google's answer in the copy, then checks the member's access again before it
//! records anything.
use super::{
    connection,
    drive::{self, Download},
    google::{FOLDER, Google, folder_url},
    mirror,
    storage::{Connection, DocumentsStore, Record},
    team,
    tree::{Row, Tree},
};
use crate::api::types::{
    AddedFiles, DocumentsFolder, DocumentsItem, FolderStep, ItemKind, NewKind,
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::Store,
    ensure,
    foundation::observability,
    server::http::{route::Dsp, upload::Upload},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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

/// Google's links to pictures of what each DSP's Documents showed, by DSP and file. A picture
/// is fetched only for a file kept here: so only one the DSP's own Documents showed, and never
/// by a link a browser sent.
static PICTURES: LazyLock<Mutex<HashMap<(String, String), Shown>>> =
    LazyLock::new(Default::default);
/// Google's link to a file's picture, when Google gave the link, and when Documents showed
/// the file.
#[derive(Clone)]
struct Shown {
    link: String,
    given: Instant,
    shown: Instant,
}
/// How long a picture is fetched after Documents showed its file, and how long a link Google
/// gave is used, well inside the hours Google's last: an older one is asked for again.
const PICTURE_LINK_LASTS: Duration = Duration::from_secs(60 * 60);
/// The most links kept across every DSP; past them, a card keeps its drawing.
const PICTURE_LINKS: usize = 20_000;
/// The picture of the file `id`, when the DSP's Documents showed it within the hour.
fn shown(dsp: &str, id: &str) -> Option<Shown> {
    PICTURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&(dsp.to_owned(), id.to_owned()))
        .filter(|shown| shown.shown.elapsed() < PICTURE_LINK_LASTS)
        .cloned()
}

/// What a call to the DSP's Drive works with.
pub(crate) struct Drive {
    pub google: Google,
    pub connection: Connection,
    pub access: String,
}
/// The DSP's connection as Dispatch keeps it, with its refresh token: none once it's gone or
/// broken.
async fn connected(state: &Arc<State>, dsp: &str) -> Result<(Connection, String)> {
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
    Ok((connection, refresh))
}
/// The DSP's connection, and an access token for it: the one kept unless `fresh`. A
/// connection Google stopped accepting is marked broken, as the hourly check marks it.
pub(crate) async fn open(state: &Arc<State>, dsp: &str, fresh: bool) -> Result<Drive> {
    let google = Google::of(&state.config)?;
    let (connection, refresh) = connected(state, dsp).await?;
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
/// The DSP's Documents, from the copy kept of its Drive: brought up to date in the background
/// once it's a few seconds old, and before it answers once it's a minute old.
pub(crate) async fn tree(state: &Arc<State>, dsp: &str) -> Result<(Connection, Arc<Tree>)> {
    let (connection, _) = connected(state, dsp).await?;
    let root = connection.folder_id.clone();
    let mirror = mirror::of(dsp);
    match mirror.age(&root) {
        Some(age) if age < mirror::FRESH => {}
        Some(age) if age < mirror::STALE => {
            if let Some(turn) = mirror.try_turn() {
                let (state, dsp, root) = (Arc::clone(state), dsp.to_owned(), root.clone());
                let mirror = Arc::clone(&mirror);
                tokio::spawn(async move {
                    if let Err(error) = follow(&state, &dsp, &root).await {
                        mirror.fail(&error);
                        observability::event(
                            "warn",
                            "documents_follow_failed",
                            json!({"error": error.code}),
                        );
                    }
                    drop(turn);
                });
            }
        }
        _ => {
            let _turn = mirror.wait().await;
            // Whoever had the turn may have brought it up to date, or just failed to: then
            // their answer stands, rather than each waiting request asking Google in turn.
            if mirror.age(&root).is_none_or(|age| age >= mirror::FRESH) {
                if let Some(error) = mirror.failed() {
                    return Err(error);
                }
                if let Err(error) = follow(state, dsp, &root).await {
                    mirror.fail(&error);
                    return Err(error);
                }
            }
        }
    }
    let tree = mirror
        .tree(&root)
        .ok_or_else(|| Error::new("google_unreachable", 502))?;
    Ok((connection, tree))
}
/// Asks Google what changed in the DSP's Drive since its copy was brought up to date, or lists
/// the Drive whole when there's no copy, or the feed lost its place. The caller has the
/// mirror's turn.
async fn follow(state: &Arc<State>, dsp: &str, root: &str) -> Result<()> {
    let mirror = mirror::of(dsp);
    if let Some(from) = mirror.changes(root) {
        match on_drive!(state, dsp, |google, token| google.changes(token, &from)) {
            Ok((_, changes)) => {
                // A folder that came in from elsewhere brings files the copy doesn't know.
                if !mirror.follow(root, changes) {
                    return Ok(());
                }
            }
            Err(error)
                if ["google_connection_broken", "google_unreachable"]
                    .contains(&error.code.as_str()) =>
            {
                return Err(error);
            }
            Err(_) => {}
        }
    }
    let (_, (changes, all)) = on_drive!(state, dsp, |google, token| async {
        let changes = google.start_changes(token).await?;
        Ok::<_, Error>((changes, google.list(token).await?))
    })?;
    mirror.replace(root, all, changes);
    Ok(())
}
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
    let (connection, tree) = tree(state, &c.dsp.id).await?;
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
    Ok(DocumentsFolder {
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
    mirror::of(&dsp).put(&connection.folder_id, made.clone());
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
    mirror::of(&dsp).put(&drive.connection.folder_id, made.clone());
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
    let empty = Tree::new(&drive.connection.folder_id, Vec::new());
    Ok(Described {
        dsp: &dsp,
        tree: &empty,
        records: &records,
        names: &names,
        account: &drive.connection.account.email,
    }
    .item(&Row::from(made), false))
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
    let (_, tree) = tree(state, &dsp).await?;
    let parent = parent.unwrap_or_else(|| tree.root().to_owned());
    ensure(tree.is_folder(&parent), "documents_item_not_found", 404)?;
    let drive = open(state, &dsp, false).await?;
    drive.google.grant(&drive.access, &ids)?;
    let mut added = Vec::new();
    // Whether it made a shortcut to a file someone else owns, to share with the team, and
    // those the account may not share on.
    let mut shortcuts = false;
    let mut unshared = Vec::new();
    for id in &ids {
        // Picked as another Google account, the file was given to that account instead.
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
            mirror::of(&dsp).put(&drive.connection.folder_id, item.clone());
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
        let mirror = mirror::of(&dsp);
        mirror.put(&drive.connection.folder_id, item.clone());
        if let Some(file) = file {
            mirror.put(&drive.connection.folder_id, file);
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
    let (connection, tree) = self::tree(state, &dsp).await?;
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
    let (connection, tree) = tree(state, &c.dsp.id).await?;
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
    let drive = open(state, &c.dsp.id, false).await?;
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
    let (_, tree) = tree(state, &dsp).await?;
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

/// Moves the file or folder `id`, and whatever it holds, to the account's trash.
pub async fn trash(state: &Arc<State>, c: Context, access: Dsp, id: String) -> Result<()> {
    let dsp = c.dsp.id.clone();
    let (connection, tree) = tree(state, &dsp).await?;
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
    dsp: &'a str,
    tree: &'a Tree,
    records: &'a BTreeMap<String, Record>,
    names: &'a BTreeMap<String, String>,
    /// The connected account, which Google names for every change Dispatch makes.
    account: &'a str,
}
impl Described<'_> {
    fn item(&self, row: &Row, located: bool) -> DocumentsItem {
        let record = self.records.get(&*row.id);
        let named = |user: &String| self.names.get(user).cloned();
        // A shortcut shows the file it stands for: its name, and who changed it last.
        let shown = row.shown();
        let modifier = shown.by.as_deref();
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
            id: row.id.to_string(),
            name: shown.name.to_string(),
            kind: kind(row.mime.as_str()),
            size: shown.size,
            modified_at: shown.modified.to_string(),
            modified_by,
            added_by: record.and_then(|record| named(&record.added_by)),
            items: row.folder().then(|| self.tree.count(&row.id) as u32),
            url: row.link.to_string(),
            location: row
                .parent
                .as_deref()
                .and_then(|parent| self.tree.get(parent))
                .filter(|_| located)
                .map(|parent| parent.name.to_string()),
            thumbnail: self.picture(shown),
            extension: extension(&shown.name, row.mime.as_str()).map(str::to_owned),
            shortcut: row.target.is_some(),
            unavailable: row.unavailable(),
        }
    }
    /// Where the page fetches Google's picture of `row`, keeping its link for the DSP to
    /// fetch it by: none when Google made none. The address names the picture's version, so
    /// the browser keeps each until the file changes.
    fn picture(&self, row: &Row) -> Option<String> {
        let link = row.thumbnail.as_deref()?.to_owned();
        let mut links = PICTURES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (self.dsp.to_owned(), row.id.to_string());
        if links.len() >= PICTURE_LINKS && !links.contains_key(&key) {
            links.retain(|_, shown| shown.shown.elapsed() < PICTURE_LINK_LASTS);
            if links.len() >= PICTURE_LINKS {
                return None;
            }
        }
        // Google's version tells its pictures apart; without one, when the file changed does.
        let version: String = match row.version {
            Some(version) => version.to_string(),
            None => row
                .modified
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .collect(),
        };
        links.insert(
            key,
            Shown {
                link,
                given: row.seen,
                shown: Instant::now(),
            },
        );
        Some(format!(
            "/api/dsp/documents/items/{}/thumbnail?v={version}",
            row.id
        ))
    }
}

/// The extension a file's name keeps, such as `.pdf`: the letters and digits after its last
/// dot, up to eight, with something before the dot. Folders and Google's own Docs, Sheets and
/// Slides have none.
fn extension<'a>(name: &'a str, mime: &str) -> Option<&'a str> {
    if mime.starts_with("application/vnd.google-apps.") {
        return None;
    }
    let dot = name.rfind('.')?;
    let after = &name[dot + 1..];
    let named = dot > 0 && (1..=8).contains(&after.len());
    (named && after.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| &name[dot..])
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
