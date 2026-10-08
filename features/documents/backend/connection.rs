//! Connecting a DSP's Google account: starting a sign-in, finishing it once Google sends the
//! browser back, and letting it go. Google is called outside the database, and the member's
//! permission is checked again after every wait, before anything is written.
use super::{
    google::{self, Google},
    storage::{Connection, DocumentsStore},
};
use crate::api::types::{
    AccountKind, ConnectionStatus, DocumentsConnection, DocumentsOverview, GoogleSignIn,
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, iso},
    foundation::crypto,
    server::http::route::Dsp,
};
use std::sync::Arc;

/// The DSP's connection, as its members see it.
pub fn overview(db: &Store, c: &Context) -> Result<DocumentsOverview> {
    let connection = match db.documents_connection(&c.dsp.id)? {
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
            connected_by: db.actor_name(&found.connected_by)?,
            connected_at: found.connected_at,
        }),
        None => None,
    };
    let owners = db
        .members(&c.dsp.id)?
        .into_iter()
        .filter(|member| member.owner)
        .map(|member| member.name)
        .collect();
    Ok(DocumentsOverview {
        connection,
        available: Google::available(&db.config),
        owners,
    })
}

/// Starts a sign-in with Google for the member. Reconnecting asks Google for the account
/// already connected, since only it can reach the files Dispatch made.
pub fn start(db: &Store, c: &Context) -> Result<GoogleSignIn> {
    let google = Google::of(&db.config)?;
    let hint = db
        .documents_connection(&c.dsp.id)?
        .map(|found| found.account.email);
    // The state Google carries back names the DSP, so the browser comes back to its page.
    let state = format!("{}.{}", c.dsp.id, crypto::token()?);
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
) -> Result<DocumentsOverview> {
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
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            db.save_documents_connection(&c.dsp.id, &connection, &refresh)?;
            let action = if existing.is_some() {
                "documents.reconnected"
            } else {
                "documents.connected"
            };
            c.audit(db, action, &connection.account.email)?;
            overview(db, &c)
        })
        .await
}

/// Lets the account go: Google takes back Dispatch's access and Dispatch forgets the token.
/// The folder and everything in it stay in the account's Drive.
pub async fn disconnect(state: &Arc<State>, c: Context, access: Dsp) -> Result<DocumentsOverview> {
    let dsp = c.dsp.id.clone();
    let token = state
        .read(move |db| db.documents_refresh_token(&dsp))
        .await?;
    if let (Some(token), Ok(google)) = (token, Google::of(&state.config)) {
        google.revoke(&token).await;
    }
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            if let Some(found) = db.documents_connection(&c.dsp.id)? {
                db.remove_documents_connection(&c.dsp.id)?;
                c.audit(db, "documents.disconnected", &found.account.email)?;
            }
            overview(db, &c)
        })
        .await
}
