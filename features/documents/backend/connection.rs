//! Connecting a DSP's Google account, one of the DSP's own accounts on Settings' DSP
//! Connections: starting a sign-in, finishing it once Google sends the browser back there, and
//! letting it go. Google is called outside the database, and the member's permission is checked
//! again after every wait, before anything is written.
use super::{
    access, cache,
    google::{self, Google},
    grant::{self, Grant},
    mirror, picker,
    storage::{Connection, DocumentsStore},
    team,
};
use crate::api::types::{
    AccountKind, ConnectionStatus, DocumentsAccount, DocumentsConnection, DocumentsOverview,
    DriveStorage, GoogleConnected, GoogleSignIn,
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, iso},
    ensure,
    foundation::observability,
    foundation::{config::Site, crypto},
    manifest::Upkeep,
    server::http::route::Dsp,
};
use serde_json::json;
use std::sync::Arc;

/// The DSP's connection, as its members see it.
pub fn overview(db: &Store, c: &Context) -> Result<DocumentsOverview> {
    let found = db.documents_connection(&c.dsp.id)?;
    let me = team::mine(db, c)?;
    // Each member picks from Google Drive as themselves: where the folder is shared with them.
    let picking = me
        .as_ref()
        .map_or_else(|| c.auth.user.email.clone(), |mine| mine.email.clone());
    let picker = match (&found, Google::of(&db.config)) {
        (Some(found), Ok(google)) => picker::setup(&google, found, picking)?,
        _ => None,
    };
    Ok(DocumentsOverview {
        connection: described(db, &c.dsp.id, found)?,
        available: Google::available(&db.config),
        me,
        picker,
    })
}
/// The connection `found`, as the dashboard shows it.
fn described(
    db: &Store,
    dsp: &str,
    found: Option<Connection>,
) -> Result<Option<DocumentsConnection>> {
    Ok(match found {
        Some(found) => Some(DocumentsConnection {
            grant_generation: found.generation.clone(),
            status: if found.broken {
                ConnectionStatus::Broken
            } else {
                ConnectionStatus::Connected
            },
            account_kind: if found.account.workspace {
                AccountKind::Workspace
            } else {
                AccountKind::Personal
            },
            account_email: found.account.email,
            folder_url: google::folder_url(&found.folder_id),
            folder_name: found.folder_name,
            connected_by: db.actor_name(dsp, &found.connected_by)?,
            connected_at: found.connected_at,
            full_access: found.full,
        }),
        None => None,
    })
}
/// The DSP's Google account as those who manage its connections see it: with how full its
/// storage is, while Google answers.
pub async fn account(state: &Arc<State>, dsp: &str) -> Result<DocumentsAccount> {
    let at = dsp.to_owned();
    let found = state.read(move |db| db.documents_connection(&at)).await?;
    let storage = match &found {
        Some(found) if !found.broken => {
            let owner = grant::capture(state, dsp, Some(found.generation.clone())).await?;
            match access::open_for(state, &owner, false).await {
                Ok(drive) => {
                    drive.admit(state).await?;
                    drive.google.storage(&drive.access).await.ok()
                }
                Err(_) => None,
            }
        }
        _ => None,
    }
    .map(|storage| DriveStorage {
        used: storage.used,
        limit: storage.limit,
    });
    let at = dsp.to_owned();
    let connection = state
        .read(move |db| {
            db.ensure_dsp_active(&at)?;
            ensure(
                db.documents_connection(&at)? == found,
                "documents_not_connected",
                409,
            )?;
            described(db, &at, found)
        })
        .await?;
    Ok(DocumentsAccount {
        connection,
        available: Google::available(&state.config),
        storage,
    })
}

/// Starts a sign-in with Google for the member, at the address `site`. Reconnecting asks
/// Google for the account already connected, since only it can reach the files Dispatch made.
pub fn start(db: &Store, c: &Context, site: &Site) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let hint = db
        .documents_connection(&c.dsp.id)?
        .map(|found| found.account.email);
    // The state Google carries back names the DSP, and the address `site` the member is at,
    // so the browser comes back to its Connections there.
    let state = format!(
        "{}.connect.{}",
        google::sign_in_of(&c.dsp.id, site),
        crypto::token()?
    );
    let verifier = crypto::token()?;
    db.start_documents_sign_in(&c.dsp.id, c.actor(), &state, &verifier)?;
    Ok(GoogleSignIn {
        url: google.sign_in_url(&db.config.origin, &state, &verifier, hint.as_deref()),
    })
}

/// One Google grant changes at a time on this server: a sign-in, from Google giving its tokens
/// until they're kept, and a deleted DSP's account given back, from asking which DSP connects
/// it until Google has ended the grant. So no DSP keeps tokens of a grant Google is ending,
/// and none is ended while a DSP is keeping its tokens.
static GRANTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Finishes the sign-in the member started with `state`, now that Google sent them back with
/// `code`: keeps the account's token, and makes the DSP's main folder unless it has one.
pub async fn finish(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    sign_in: String,
    code: String,
) -> Result<GoogleConnected> {
    ensure(!team::is_link(&sign_in), "documents_connect_expired", 409)?;
    let (dsp, actor) = (c.dsp.id.clone(), c.actor().to_owned());
    let verifier = state
        .run(move |db| db.finish_documents_sign_in(&dsp, &actor, &sign_in))
        .await?;
    let google = Google::of(&state.config)?;
    let granting = GRANTS.lock().await;
    let current = c.clone();
    let existing = state
        .read(move |db| {
            let c = access.revalidate(db, &current)?;
            db.documents_connection(&c.dsp.id)
        })
        .await?;
    let granted = google
        .exchange(&state.config.origin, &code, &verifier)
        .await?;
    if let Some(existing) = &existing
        && existing.account.email != granted.account.email
    {
        google.revoke(&granted.refresh).await;
        return Err(Error::new("documents_account_mismatch", 409));
    }
    let kept = match &existing {
        Some(existing)
            if google
                .folder_usable(&granted.access, &existing.folder_id)
                .await? =>
        {
            Some((existing.folder_id.clone(), existing.folder_name.clone()))
        }
        _ => None,
    };
    let made_folder = kept.is_none();
    // Fixture mode's team made files directly in the folder it kept, meanwhile.
    if let (Google::Fixture, Some((folder, _))) = (&google, &kept) {
        super::drive::fixture_made_in_folder(&granted.access, folder)?;
    }
    let (folder_id, folder_name) = match kept {
        Some(kept) => kept,
        None => {
            let name = format!("{} Documents", c.dsp.name);
            (google.create_folder(&granted.access, &name).await?, name)
        }
    };
    let connection = Connection {
        generation: String::new(), // Assigned by the storage replacement operation.
        broken: false,
        full: true,
        account: granted.account,
        folder_id,
        folder_name,
        connected_by: c.actor().to_owned(),
        connected_at: iso(),
    };
    let refresh = granted.refresh;
    let shared = c.dsp.id.clone();
    let owner = state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            ensure(
                db.documents_connection(&c.dsp.id)? == existing,
                "documents_not_connected",
                409,
            )?;
            // Invalidation and replacement are one exclusive State transition.
            cache::forget(&db.config.environment_root(), &c.dsp.id);
            mirror::forget(&db.config.environment_root(), &c.dsp.id);
            db.save_documents_connection(&c.dsp.id, &connection, &refresh)?;
            let action = if existing.is_some() {
                "documents.reconnected"
            } else {
                "documents.connected"
            };
            c.audit(db, action, &connection.account.email)?;
            Grant::capture(db, &c.dsp.id, None)
        })
        .await?;
    drop(granting);
    // The team gets the folder shared with them while the owner looks around.
    let sharing = Arc::clone(state);
    tokio::spawn(async move {
        if let Err(error) = team::sync_for(&sharing, &owner).await {
            observability::event(
                "warn",
                "documents_share_failed",
                json!({"error": error.code}),
            );
        }
    });
    Ok(GoogleConnected {
        account: account(state, &shared).await?,
        made_folder,
    })
}

/// Lets the account go: Google takes back Dispatch's access and Dispatch forgets the token.
/// The folder and everything in it stay in the account's Drive.
pub async fn disconnect(state: &Arc<State>, c: Context, access: Dsp) -> Result<DocumentsAccount> {
    let _granting = GRANTS.lock().await;
    let id = c.dsp.id.clone();
    let asked = c.clone();
    let (found, token) = state
        .read(move |db| {
            let c = access.revalidate(db, &asked)?;
            Ok((
                db.documents_connection(&c.dsp.id)?,
                db.documents_refresh_token(&c.dsp.id)?,
            ))
        })
        .await?;
    if let (Some(token), Ok(google)) = (token, Google::of(&state.config)) {
        google.revoke(&token).await;
    }
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            ensure(
                db.documents_connection(&c.dsp.id)? == found,
                "documents_not_connected",
                409,
            )?;
            cache::forget(&db.config.environment_root(), &c.dsp.id);
            mirror::forget(&db.config.environment_root(), &c.dsp.id);
            if let Some(found) = found {
                db.remove_documents_connection(&c.dsp.id)?;
                c.audit(db, "documents.disconnected", &found.account.email)?;
            }
            Ok(())
        })
        .await?;
    account(state, &id).await
}

/// Gives back the Google account of a DSP being deleted, as disconnecting it does, unless
/// another DSP connects that account too: best effort, as the DSP goes whatever Google answers.
/// The folder stays in the account's Drive; the token goes with the DSP.
pub fn revoke(state: Arc<State>, dsp: String) -> Upkeep {
    Box::pin(async move {
        let Ok(google) = Google::of(&state.config) else {
            return;
        };
        let _granting = GRANTS.lock().await;
        match state.read(move |db| own_grant(db, &dsp)).await {
            Ok(Some(token)) => google.revoke(&token).await,
            Ok(None) => {}
            Err(error) => observability::event(
                "error",
                "documents_revoke_failed",
                json!({"error": error.code}),
            ),
        }
    })
}
/// The refresh token of the DSP's Google account, when no other DSP kept connects that
/// account: Google ends the account's grant to Dispatch for every token given back, the other
/// DSP's too.
fn own_grant(db: &Store, dsp: &str) -> Result<Option<String>> {
    let account = db
        .documents_connection(dsp)?
        .map(|found| found.account.email);
    for other in db.kept_dsps()?.iter().filter(|other| *other != dsp) {
        let connected = db.documents_connection(other)?;
        if connected.is_some_and(|found| Some(&found.account.email) == account.as_ref()) {
            return Ok(None);
        }
    }
    db.documents_refresh_token(dsp)
}
/// Forgets what's held in memory of a deleted DSP's files.
pub fn forgotten(state: &State, dsp: &str) {
    let namespace = state.config.environment_root();
    cache::forget_deleted(&namespace, dsp);
    mirror::forget(&namespace, dsp);
}

/// Marks the DSP's connection broken, as Google refused its token: once, in the activity log
/// too, whoever noticed.
pub(crate) fn broken(db: &Store, dsp: &str) -> Result<()> {
    if db.break_documents_connection(dsp)? {
        cache::forget(&db.config.environment_root(), dsp);
        mirror::forget(&db.config.environment_root(), dsp);
        db.audit(None, Some(dsp), "documents.connection_broken", "")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/backend/connection.rs"]
mod tests;
