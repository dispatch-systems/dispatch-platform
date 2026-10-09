//! Connecting a DSP's Google account, one of the DSP's own accounts on Settings' DSP
//! Connections: starting a sign-in, finishing it once Google sends the browser back there, and
//! letting it go. Google is called outside the database, and the member's permission is checked
//! again after every wait, before anything is written.
use super::{
    files,
    google::{self, Google},
    picker,
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
    foundation::crypto,
    foundation::observability,
    server::http::route::Dsp,
};
use serde_json::json;
use std::sync::Arc;

/// The DSP's connection, as its members see it.
pub fn overview(db: &Store, c: &Context) -> Result<DocumentsOverview> {
    let found = db.documents_connection(&c.dsp.id)?;
    let picker = match (&found, Google::of(&db.config)) {
        (Some(found), Ok(google)) => picker::setup(c, &google, found)?,
        _ => None,
    };
    Ok(DocumentsOverview {
        connection: described(db, &c.dsp.id, found)?,
        available: Google::available(&db.config),
        me: team::mine(db, c)?,
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
        Some(found) if !found.broken => match files::open(state, dsp, false).await {
            Ok(drive) => drive.google.storage(&drive.access).await.ok(),
            Err(_) => None,
        },
        _ => None,
    }
    .map(|storage| DriveStorage {
        used: storage.used,
        limit: storage.limit,
    });
    let at = dsp.to_owned();
    let connection = state.read(move |db| described(db, &at, found)).await?;
    Ok(DocumentsAccount {
        connection,
        available: Google::available(&state.config),
        storage,
    })
}

/// Starts a sign-in with Google for the member. Reconnecting asks Google for the account
/// already connected, since only it can reach the files Dispatch made.
pub fn start(db: &Store, c: &Context) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let hint = db
        .documents_connection(&c.dsp.id)?
        .map(|found| found.account.email);
    // The state Google carries back names the DSP, so the browser comes back to its
    // Connections.
    let state = format!("{}.connect.{}", c.dsp.id, crypto::token()?);
    let verifier = crypto::token()?;
    db.start_documents_sign_in(&c.dsp.id, c.actor(), &state, &verifier)?;
    Ok(GoogleSignIn {
        url: google.sign_in_url(&db.config.origin, &state, &verifier, hint.as_deref()),
    })
}

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
    let (verifier, existing) = state
        .run(move |db| {
            let verifier = db.finish_documents_sign_in(&dsp, &actor, &sign_in)?;
            Ok((verifier, db.documents_connection(&dsp)?))
        })
        .await?;
    let google = Google::of(&state.config)?;
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
    let (folder_id, folder_name) = match kept {
        Some(kept) => kept,
        None => {
            let name = format!("{} Documents", c.dsp.name);
            (google.create_folder(&granted.access, &name).await?, name)
        }
    };
    let connection = Connection {
        broken: false,
        account: granted.account,
        folder_id,
        folder_name,
        connected_by: c.actor().to_owned(),
        connected_at: iso(),
    };
    let refresh = granted.refresh;
    files::forget(&c.dsp.id);
    let shared = c.dsp.id.clone();
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.save_documents_connection(&c.dsp.id, &connection, &refresh)?;
            let action = if existing.is_some() {
                "documents.reconnected"
            } else {
                "documents.connected"
            };
            c.audit(db, action, &connection.account.email)
        })
        .await?;
    // The team gets the folder shared with them while the owner looks around.
    let sharing = Arc::clone(state);
    let at = shared.clone();
    tokio::spawn(async move {
        if let Err(error) = team::sync(&sharing, &at).await {
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
    let id = c.dsp.id.clone();
    let dsp = id.clone();
    let token = state
        .read(move |db| db.documents_refresh_token(&dsp))
        .await?;
    if let (Some(token), Ok(google)) = (token, Google::of(&state.config)) {
        google.revoke(&token).await;
    }
    files::forget(&c.dsp.id);
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            if let Some(found) = db.documents_connection(&c.dsp.id)? {
                db.remove_documents_connection(&c.dsp.id)?;
                c.audit(db, "documents.disconnected", &found.account.email)?;
            }
            Ok(())
        })
        .await?;
    account(state, &id).await
}

/// Marks the DSP's connection broken, as Google refused its token: once, in the activity log
/// too, whoever noticed.
pub(crate) fn broken(db: &Store, dsp: &str) -> Result<()> {
    if db.break_documents_connection(dsp)? {
        db.audit(None, Some(dsp), "documents.connection_broken", "")?;
    }
    Ok(())
}
