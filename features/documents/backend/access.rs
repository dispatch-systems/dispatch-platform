//! Opening and checking a DSP's Drive, renewing refused tokens, and refreshing its mirror.
//! Google calls stay outside database turns; renewal rechecks the DSP before access is kept.
use super::{
    cache::{keep, kept},
    connection,
    google::Google,
    mirror,
    storage::{Connection, DocumentsStore},
    team,
    tree::Tree,
};
use dispatch_core::{Error, Result, State, foundation::observability};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// What a call to the DSP's Drive works with.
pub(crate) struct Drive {
    pub google: Google,
    pub connection: Connection,
    pub access: String,
}
/// The DSP's connection as Dispatch keeps it, with its refresh token: none once it's gone or
/// broken, nor while the DSP is disabled, so nothing reaches its Drive then.
async fn connected(state: &Arc<State>, dsp: &str) -> Result<(Connection, String)> {
    let at = dsp.to_owned();
    let (connection, refresh) = state
        .read(move |db| {
            db.ensure_dsp_active(&at)?;
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
            // Disabled while Google renewed it, nothing more reaches the DSP's Drive.
            active(state, dsp).await?;
            let until = Instant::now() + access.lasts.saturating_sub(Duration::from_secs(60));
            keep(dsp, &access.token, until);
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
/// Whether Google may still be asked anything for the DSP: not once it's disabled, though
/// what asks began before (`dsp_unavailable`).
pub(crate) async fn active(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    state
        .read(move |db| db.ensure_dsp_active(&at).map(drop))
        .await
}
/// Runs `$call` on the DSP's Drive, with `$google` and its access token `$token`; once more
/// with a fresh token if Google refused the kept one, since an access token lapses early when
/// the account signs out everywhere. Answers the connection with what the call answered.
/// The named-Drive form keeps the renewed Drive for Team's subsequent sharing calls.
macro_rules! on_drive {
    ($state:expr, $dsp:expr, |$google:ident, $token:ident| $call:expr) => {{
        let drive = $crate::backend::access::open($state, $dsp, false).await?;
        let ($google, $token) = (&drive.google, drive.access.as_str());
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                $crate::backend::cache::forget($dsp);
                let drive = $crate::backend::access::open($state, $dsp, true).await?;
                let ($google, $token) = (&drive.google, drive.access.as_str());
                $call.await.map(|value| (drive.connection, value))
            }
            answer => answer.map(|value| (drive.connection, value)),
        }
    }};
    ($state:expr, $dsp:expr, $drive:ident, $call:expr) => {{
        let mut $drive = $crate::backend::access::open($state, $dsp, false).await?;
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                $crate::backend::cache::forget($dsp);
                $drive = $crate::backend::access::open($state, $dsp, true).await?;
                $call.await.map(|value| ($drive, value))
            }
            answer => answer.map(|value| ($drive, value)),
        }
    }};
}
pub(super) use on_drive;

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
        Ok::<_, Error>((changes, google.inside(token, root).await?))
    })?;
    mirror.replace(root, all, changes);
    Ok(())
}

/// Checks Google's grant for an enabled DSP, then reconciles its team shares.
pub(super) async fn check(state: Arc<State>, google: &Google, dsp: String) -> Result<()> {
    let selected = dsp.clone();
    let token = state
        .read(move |db| {
            if db.serves_dsp(&selected)?
                && db.feature_enabled(&selected, "documents")?
                && db
                    .documents_connection(&selected)?
                    .is_some_and(|c| !c.broken)
            {
                db.documents_refresh_token(&selected)
            } else {
                Ok(None)
            }
        })
        .await?;
    let Some(token) = token else {
        return Ok(());
    };
    // Only Google's refusal breaks it: an outage leaves it for next hour.
    match google.access(&token).await {
        Err(error) if error.code == "google_connection_broken" => {
            state.run(move |db| connection::broken(db, &dsp)).await?;
        }
        Err(error) => return Err(error),
        Ok(_) => {
            team::sync(&state, &dsp).await?;
        }
    }
    dispatch_core::Result::Ok(())
}
