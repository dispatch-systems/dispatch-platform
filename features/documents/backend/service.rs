//! What Documents offers every other feature: somewhere to keep a DSP's files (core's
//! `services::files`), in its Google Drive, as Slack keeps the files posted in a channel. A file
//! saved through it is Documents' like any other, shown, opened and moved as an upload is, with
//! who it came from as who added it. Every call works only inside the DSP's own folder.
use super::{
    files::{self, make, put},
    google::FOLDER,
    storage::DocumentsStore,
    tree::Row,
};
use dispatch_core::{
    Result, State, ensure,
    manifest::services::{
        Answer,
        files::{Author, Browsed, Files, Folder, Incoming, Saved},
    },
    server::http::upload::UPLOAD_LIMIT,
};
use std::sync::Arc;

/// How the service refuses a folder that isn't one of the DSP's.
const FOLDER_NOT_FOUND: &str = "files_folder_not_found";

/// Documents, as the feature keeping a DSP's files.
pub(crate) struct DocumentsFiles;
pub(crate) static FILES: DocumentsFiles = DocumentsFiles;

impl Files for DocumentsFiles {
    fn name(&self) -> &'static str {
        "Documents"
    }
    fn permission(&self) -> &'static str {
        "documents.use"
    }
    fn largest(&self) -> u64 {
        UPLOAD_LIMIT
    }
    fn unready<'a>(&'a self, state: &'a Arc<State>, dsp: &'a str) -> Answer<'a, Option<String>> {
        Box::pin(async move {
            let at = dsp.to_owned();
            let connection = state.read(move |db| db.documents_connection(&at)).await?;
            Ok(match connection {
                None => Some("Google Drive isn't connected".to_owned()),
                Some(connection) if connection.broken => {
                    Some("Google Drive needs reconnecting".to_owned())
                }
                Some(_) => None,
            })
        })
    }
    fn browse<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: Option<&'a str>,
    ) -> Answer<'a, Browsed> {
        Box::pin(async move {
            let (connection, tree) = files::tree(state, dsp).await?;
            let at = id.unwrap_or(tree.root());
            ensure(tree.is_folder(at), FOLDER_NOT_FOUND, 404)?;
            let folder = |row: &Row| Folder {
                id: row.id.to_string(),
                name: row.name.to_string(),
            };
            let top = Folder {
                id: tree.root().to_owned(),
                name: connection.folder_name,
            };
            Ok(Browsed {
                path: std::iter::once(top)
                    .chain(tree.path(at).into_iter().map(folder))
                    .collect(),
                folders: tree
                    .children(at)
                    .into_iter()
                    .filter(|row| row.folder())
                    .map(folder)
                    .collect(),
            })
        })
    }
    fn folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
    ) -> Answer<'a, Option<Folder>> {
        Box::pin(async move {
            let (connection, tree) = files::tree(state, dsp).await?;
            if id == tree.root() {
                return Ok(Some(Folder {
                    id: id.to_owned(),
                    name: connection.folder_name,
                }));
            }
            Ok(tree
                .get(id)
                .filter(|_| tree.is_folder(id))
                .map(|row| Folder {
                    id: id.to_owned(),
                    name: row.name.to_string(),
                }))
        })
    }
    fn create_folder<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        name: &'a str,
        by: &'a Author,
    ) -> Answer<'a, Folder> {
        Box::pin(async move {
            let (_, tree) = files::tree(state, dsp).await?;
            let (_, made) = make(state, dsp, tree.root(), FOLDER, name).await?;
            let folder = Folder {
                id: made.id,
                name: made.name,
            };
            record(
                state,
                dsp,
                &folder.id,
                by,
                "documents.created",
                &folder.name,
            )
            .await?;
            Ok(folder)
        })
    }
    fn save<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        folder: &'a str,
        file: Incoming,
    ) -> Answer<'a, Saved> {
        Box::pin(async move {
            ensure(file.bytes.length <= self.largest(), "upload_too_large", 413)?;
            let (_, tree) = files::tree(state, dsp).await?;
            ensure(tree.is_folder(folder), FOLDER_NOT_FOUND, 404)?;
            let (_, made) = put(state, dsp, folder, &file.name, &file.mime, file.bytes).await?;
            let detail = format!("{} from {}", made.name, file.from);
            record(state, dsp, &made.id, &file.by, "documents.saved", &detail).await?;
            Ok(Saved {
                id: made.id,
                name: made.name,
                link: made.web_view_link,
            })
        })
    }
}

/// Records who `by` made or saved the file `id`, and logs it as `action`: as the member, or
/// with no member as its actor for someone known only by name.
async fn record(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    by: &Author,
    action: &'static str,
    detail: &str,
) -> Result<()> {
    let (at, id, by, detail) = (dsp.to_owned(), id.to_owned(), by.clone(), detail.to_owned());
    state
        .run(move |db| {
            db.record_documents_change_by(&at, &id, &by)?;
            let actor = match &by {
                Author::Member(user) => Some(user.as_str()),
                Author::Named(_) => None,
            };
            db.audit(actor, Some(&at), action, &detail)
        })
        .await
}

#[cfg(test)]
#[path = "../tests/backend/service.rs"]
mod tests;
