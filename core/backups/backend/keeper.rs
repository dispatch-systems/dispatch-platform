//! What takes the DSPs' backups: once a minute, or as soon as one is asked for by hand, it asks
//! for a backup by each rule that's due, takes each that waits, one at a time, the oldest first,
//! then lets go of those that expired and of the files no backup holds any more. It works
//! outside requests, its own copies of the databases made beside the server's, so a page never
//! waits on it. Without somewhere to keep backups it does nothing.
use super::{
    FILES, MANIFESTS, Vault, history, schedule,
    take::{self, Asked},
};
use crate::{Result, State, db::now, foundation::observability};
use serde_json::json;
use std::{collections::HashSet, sync::Arc, time::Duration};

/// How often it looks for a rule that's due.
const EVERY: Duration = Duration::from_secs(60);

pub async fn keeper(state: Arc<State>, mut stop: tokio::sync::watch::Receiver<bool>) -> Result<()> {
    let Some(vault) = state.backups.as_ref() else {
        crate::cancelled(&mut stop).await;
        return Ok(());
    };
    // What a backup cut short left on its way.
    let _ = std::fs::remove_dir_all(take::staging(&state));
    loop {
        tokio::select! {
            _ = crate::cancelled(&mut stop) => return Ok(()),
            result = pass(&state, vault) => if let Err(error) = result {
                observability::event("error", "backups.pass_failed", json!({"error": error.code}));
            },
        }
        tokio::select! {
            _ = crate::cancelled(&mut stop) => return Ok(()),
            _ = tokio::time::sleep(EVERY) => {}
            _ = state.backups_wake.notified() => {}
        }
    }
}

/// One pass: what's due asked for, what waits taken, what expired let go of.
pub async fn pass(state: &Arc<State>, vault: &Vault) -> Result<()> {
    state
        .run_bookkeeping(|db| {
            history::interrupted(&db.platform)?;
            let dsps: Vec<(String,)> = db.platform.query_as(
                "SELECT id FROM dsps WHERE status IN ('active','suspended')",
                [],
            )?;
            for (dsp,) in dsps {
                let tz = schedule::timezone(db, &dsp)?;
                schedule::start(db, &dsp, tz)?;
                db.platform.transaction(|| {
                    let at = now();
                    for rule in schedule::due(&db.platform, &dsp, tz, at)? {
                        history::ask(
                            &db.platform,
                            &dsp,
                            rule.id.as_deref(),
                            None,
                            rule.keep,
                            &rule.skipped,
                        )?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
        .await?;
    while let Some(history::Waiting { id, dsp, skipped }) = state
        .run_bookkeeping(|db| history::next(&db.platform))
        .await?
    {
        let asked = {
            let (id, dsp) = (id.clone(), dsp.clone());
            state
                .run_bookkeeping(move |db| {
                    history::started(&db.platform, &id)?;
                    let generation = schedule::generation_of(&db.platform, &dsp)?;
                    Ok(Asked {
                        id,
                        dsp,
                        skipped,
                        generation,
                    })
                })
                .await?
        };
        let result = take::take(state, vault, &asked).await;
        let (code, size) = match &result {
            Ok(taken) => (None, taken.size),
            Err(error) => (Some(error.code.clone()), 0),
        };
        state
            .run_bookkeeping(move |db| match result {
                Ok(taken) => history::succeeded(&db.platform, &id, &taken.files, taken.size),
                Err(error) => history::failed(&db.platform, &id, &error.code),
            })
            .await?;
        observability::event(
            if code.is_some() { "error" } else { "info" },
            if code.is_some() {
                "backups.failed"
            } else {
                "backups.taken"
            },
            json!({"dsp": dsp, "error": code, "size": size}),
        );
    }
    prune(state, vault).await
}

/// Lets go of the backups that expired: their descriptions, then each file one held that no
/// backup kept holds, then the backups themselves, so a pass cut short leaves them to the next.
/// Nothing the platform doesn't know a backup of held goes: a bucket a new server starts with
/// keeps what an older one took.
async fn prune(state: &Arc<State>, vault: &Vault) -> Result<()> {
    let expired = state
        .run_bookkeeping(|db| {
            history::outnumbered(&db.platform, now())?;
            history::expired(&db.platform, now())
        })
        .await?;
    for (dsp, gone) in expired {
        let held = {
            let dsp = dsp.clone();
            state
                .run_bookkeeping(move |db| history::held(&db.platform, &dsp, now()))
                .await?
        };
        let mut objects = HashSet::new();
        for backup in &gone {
            vault
                .objects
                .delete(&dsp, MANIFESTS, &vault.manifest(&backup.id))
                .await?;
            objects.extend(backup.objects.iter().cloned());
        }
        for object in objects.difference(&held) {
            vault.objects.delete(&dsp, FILES, object).await?;
        }
        let ids: Vec<String> = gone.into_iter().map(|backup| backup.id).collect();
        state
            .run_bookkeeping(move |db| history::forget(&db.platform, &ids))
            .await?;
    }
    Ok(())
}
