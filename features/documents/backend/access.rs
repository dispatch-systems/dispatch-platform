//! Captured-generation Drive access and RAM refresh. Provider awaits happen outside State
//! turns; each provider operation and every publication uses the captured owner only.
use super::{
    cache, connection,
    google::Google,
    grant::{self, Grant},
    mirror,
    storage::DocumentsStore,
    team,
    tree::Tree,
};
use dispatch_core::{Result, State, foundation::observability};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Drive {
    pub google: Google,
    pub connection: super::storage::Connection,
    pub access: String,
    pub owner: Grant,
}
impl Drive {
    pub async fn admit(&self, state: &Arc<State>) -> Result<()> {
        self.owner.read(state, |_, _| Ok(())).await
    }
}
pub(super) async fn open_for(state: &Arc<State>, owner: &Grant, fresh: bool) -> Result<Drive> {
    let google = Google::of(&state.config)?;
    let found = owner
        .read(state, |_, owner| Ok(cache::kept(&owner.key)))
        .await?;
    if let Some(access) = found.filter(|_| !fresh) {
        return Ok(Drive {
            google,
            connection: owner.connection.clone(),
            access,
            owner: owner.clone(),
        });
    }
    let _renewing = owner.mirror.renewal.lock().await;
    // Recheck after waiting: concurrent normal opens and refused-token retries coalesce.
    let (cached, refresh) = owner
        .read(state, |db, owner| {
            Ok((
                cache::kept(&owner.key),
                db.documents_refresh_token(&owner.key.dsp)?,
            ))
        })
        .await?;
    if let Some(access) = cached.filter(|_| !fresh) {
        return Ok(Drive {
            google,
            connection: owner.connection.clone(),
            access,
            owner: owner.clone(),
        });
    }
    let refresh =
        refresh.ok_or_else(|| dispatch_core::Error::new("google_connection_broken", 409))?;
    match google.access(&refresh).await {
        Ok(access) => {
            let until = Instant::now() + access.lasts.saturating_sub(Duration::from_secs(60));
            let token = access.token.clone();
            owner
                .read(state, move |_, owner| {
                    cache::keep(&owner.key, &token, until);
                    Ok(())
                })
                .await?;
            Ok(Drive {
                google,
                connection: owner.connection.clone(),
                access: access.token,
                owner: owner.clone(),
            })
        }
        Err(error) => {
            if error.code == "google_connection_broken" {
                owner
                    .run(state, |db, owner| connection::broken(db, &owner.key.dsp))
                    .await?;
            } else {
                owner.read(state, |_, _| Ok(())).await?;
            }
            Err(error)
        }
    }
}
/// Revalidation of active DSP/feature/connection, without replacing member checks.
pub(super) async fn current(state: &Arc<State>, owner: &Grant) -> Result<()> {
    owner.read(state, |_, _| Ok(())).await
}
/// One retry after a refused cached access token, strictly under the original generation.
macro_rules! on_drive {
    ($state:expr, $owner:expr, |$google:ident, $token:ident| $call:expr) => {{
        let owner = $owner;
        let drive = $crate::backend::access::open_for($state, owner, false).await?;
        drive.admit($state).await?;
        let ($google, $token) = (&drive.google, drive.access.as_str());
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                let refused = drive.access.clone();
                owner
                    .read($state, move |_, owner| {
                        $crate::backend::cache::refused(&owner.key, &refused);
                        Ok(())
                    })
                    .await?;
                let drive = $crate::backend::access::open_for($state, owner, false).await?;
                drive.admit($state).await?;
                let ($google, $token) = (&drive.google, drive.access.as_str());
                $call.await.map(|value| (drive.connection, value))
            }
            answer => answer.map(|value| (drive.connection, value)),
        }
    }};
    ($state:expr, $owner:expr, $drive:ident, $call:expr) => {{
        let owner = $owner;
        let mut $drive = $crate::backend::access::open_for($state, owner, false).await?;
        $drive.admit($state).await?;
        match $call.await {
            Err(error) if error.code == "google_connection_broken" => {
                let refused = $drive.access.clone();
                owner
                    .read($state, move |_, owner| {
                        $crate::backend::cache::refused(&owner.key, &refused);
                        Ok(())
                    })
                    .await?;
                $drive = $crate::backend::access::open_for($state, owner, false).await?;
                $drive.admit($state).await?;
                $call.await.map(|value| ($drive, value))
            }
            answer => answer.map(|value| ($drive, value)),
        }
    }};
}
pub(super) use on_drive;

pub(super) async fn tree_for(state: &Arc<State>, owner: &Grant) -> Result<Arc<Tree>> {
    let root = owner.connection.folder_id.clone();
    let mirror = &owner.mirror;
    current(state, owner).await?;
    match mirror.age(&root) {
        Some(age) if age < mirror::FRESH => {}
        Some(age) if age < mirror::STALE => {
            if let Some(turn) = mirror.try_turn() {
                let (state, owner) = (Arc::clone(state), owner.clone());
                tokio::spawn(async move {
                    if let Err(error) = follow(&state, &owner).await {
                        let code = error.code.clone();
                        // An obsolete generation's failure cannot poison a replacement.
                        let _ = owner
                            .read(&state, move |_, owner| {
                                owner.mirror.fail(&error);
                                Ok(())
                            })
                            .await;
                        observability::event(
                            "warn",
                            "documents_follow_failed",
                            json!({"error": code}),
                        );
                    }
                    drop(turn);
                });
            }
        }
        _ => {
            let _turn = mirror.wait().await;
            current(state, owner).await?;
            if mirror.age(&root).is_none_or(|age| age >= mirror::FRESH) {
                if let Some(error) = mirror.failed() {
                    return Err(error);
                }
                if let Err(error) = follow(state, owner).await {
                    let code = dispatch_core::Error::new(&error.code, error.status);
                    owner
                        .read(state, move |_, owner| {
                            owner.mirror.fail(&code);
                            Ok(())
                        })
                        .await?;
                    return Err(error);
                }
            }
        }
    }
    owner
        .read(state, |_, owner| {
            owner
                .mirror
                .tree(&owner.connection.folder_id)
                .ok_or_else(|| dispatch_core::Error::new("google_unreachable", 502))
        })
        .await
}
async fn follow(state: &Arc<State>, owner: &Grant) -> Result<()> {
    let root = &owner.connection.folder_id;
    if let Some(from) = owner.mirror.changes(root) {
        match on_drive!(state, owner, |google, token| google.changes(token, &from)) {
            Ok((_, changes)) => {
                let relist = owner
                    .read(state, move |_, owner| {
                        Ok(owner.mirror.follow(&owner.connection.folder_id, changes))
                    })
                    .await?;
                if !relist {
                    return Ok(());
                }
            }
            Err(error)
                if ["google_connection_broken", "google_unreachable"]
                    .contains(&error.code.as_str()) =>
            {
                return Err(error);
            }
            Err(_) => {
                current(state, owner).await?;
            }
        }
    }
    let (_, changes) = on_drive!(state, owner, |google, token| google.start_changes(token))?;
    let (_, all) = on_drive!(state, owner, |google, token| google.inside(token, root))?;
    owner
        .read(state, move |_, owner| {
            owner
                .mirror
                .replace(&owner.connection.folder_id, all, changes);
            Ok(())
        })
        .await
}
/// Hourly grant checking has the same captured-generation failure fence as foreground access.
pub(super) async fn check(state: Arc<State>, _google: &Google, dsp: String) -> Result<()> {
    let at = dsp.clone();
    let owner = state
        .read(move |db| {
            use super::storage::DocumentsStore;
            if db.serves_dsp(&at)?
                && db.feature_enabled(&at, "documents")?
                && db.documents_connection(&at)?.is_some_and(|c| !c.broken)
            {
                Ok(Some(Grant::capture(db, &at, None)?))
            } else {
                Ok(None)
            }
        })
        .await?;
    let Some(owner) = owner else {
        return Ok(());
    };
    open_for(&state, &owner, true).await?;
    team::sync_for(&state, &owner).await?;
    Ok(())
}
