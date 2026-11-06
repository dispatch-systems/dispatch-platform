//! Deleting a DSP for good: everything of it, wherever it is, and its short code freed for
//! another. Only a disabled DSP is deleted, never the permanent one, and only by its name as
//! the platform owner typed it.
//!
//! Each step holds if the server stops partway. The DSP is held for its delete from the first
//! check that it can be deleted until its rows are gone, so it isn't enabled meanwhile; one that
//! stops short of that stays disabled. What the DSP connected outside Dispatch is given back
//! first, best effort. One transaction then deletes every row of it in the
//! platform's database, its own row included, and notes the delete in `dsp_deletions`: from
//! then on nothing finds the DSP, and startup no longer looks for its folder. `finish` deletes
//! what it kept elsewhere, its jobs, the mail development captured for it and its folder, and
//! lets the note go once all of it has gone. A delete cut short is finished by the next upkeep
//! pass, as the server starts too.
use crate::{
    Error, Result, State,
    db::{self, Store, iso},
    ensure,
    foundation::observability,
    manifest::registry,
    server::mail,
    support,
    tenancy::api::types::DspStatus,
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

/// How long the object store may take to settle an upload it was sent, once it's given up.
const SETTLING: std::time::Duration = std::time::Duration::from_secs(60);

/// The DSPs being deleted, by their folders: each from the turn that found it could be until
/// its delete ends (`Reserved`).
static DELETING: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
fn deleting() -> MutexGuard<'static, BTreeSet<PathBuf>> {
    DELETING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
/// A DSP held for its delete, until this drops.
pub struct Reserved(PathBuf);
impl Drop for Reserved {
    fn drop(&mut self) {
        deleting().remove(&self.0);
    }
}

impl Store {
    /// Refuses deleting the DSP `id` unless it's disabled, isn't the permanent one, and `name`
    /// is its name, exactly.
    pub fn ensure_deletable(&self, id: &str, name: &str) -> Result<()> {
        let dsp = self.find_dsp(id)?;
        ensure(!dsp.permanent, "permanent_dev_required", 409)?;
        ensure(dsp.status == DspStatus::Suspended, "dsp_not_disabled", 409)?;
        ensure(dsp.name == name, "dsp_name_mismatch", 400)
    }
    /// Holds the DSP `id` for its delete, as `ensure_deletable` allows, until what it answers
    /// drops: another delete and enabling it are refused meanwhile (`dsp_deleting`).
    fn reserve_deletion(&self, id: &str, name: &str) -> Result<Reserved> {
        self.ensure_deletable(id, name)?;
        let folder = self.dsp_folder(id)?;
        ensure(deleting().insert(folder.clone()), "dsp_deleting", 409)?;
        Ok(Reserved(folder))
    }
    /// Refuses the DSP `id` while it's being deleted.
    pub fn ensure_not_deleting(&self, id: &str) -> Result<()> {
        let folder = self.dsp_folder(id)?;
        ensure(!deleting().contains(&folder), "dsp_deleting", 409)
    }
    /// Deletes every row of the DSP `id` in the platform's database, as `ensure_deletable`
    /// allows: its log, with what its people, those it has and those it removed, did to their
    /// own accounts, its mail, support tickets and switches, what each feature and the agents
    /// keep of it there, what the platform's directory holds of its people, and its own row,
    /// which frees its short code. Notes that what it kept elsewhere is still to delete
    /// (`finish`), and records who deleted it, by its name, as nothing else names it now.
    pub fn delete_dsp(&self, id: &str, name: &str, actor: &str) -> Result<()> {
        self.ensure_deletable(id, name)?;
        let own: Vec<(String,)> = self.dsp(id)?.query_as("SELECT id FROM users", [])?;
        let own: Vec<String> = own
            .into_iter()
            .map(|(id,)| id)
            .chain(self.removed_members(id)?)
            .collect();
        self.platform.transaction(|| {
            self.forget_audit(id, &self.people_alone(id, &own)?)?;
            mail::forget_dsp(self, id)?;
            support::tickets::forget_dsp(self, id)?;
            if let Some(agents) = registry().agents {
                (agents.forget_dsp)(self, id)?;
            }
            for forget in registry().features.iter().filter_map(|f| f.deleting.forget) {
                forget(self, id)?;
            }
            self.forget_legacy_people(id)?;
            self.platform
                .exec("DELETE FROM dsp_features WHERE dsp_id=?", [id])?;
            self.platform.exec("DELETE FROM dsps WHERE id=?", [id])?;
            self.platform.exec(
                "INSERT INTO dsp_deletions(dsp_id,started_at) VALUES (?,?)",
                [id, iso().as_str()],
            )?;
            self.audit(Some(actor), None, "dsp.deleted", name)
        })
    }
    /// When the delete of `dsp` began, in milliseconds since the epoch: none once it's finished.
    fn deletion_started(&self, dsp: &str) -> Result<Option<i64>> {
        let started: Vec<(String,)> = self
            .platform
            .query_as("SELECT started_at FROM dsp_deletions WHERE dsp_id=?", [dsp])?;
        Ok(started.first().and_then(|(at,)| {
            chrono::DateTime::parse_from_rfc3339(at)
                .ok()
                .map(|at| at.timestamp_millis())
        }))
    }
    /// The DSPs whose rows are gone and whose delete isn't finished.
    fn deletions(&self) -> Result<Vec<String>> {
        let deleting: Vec<(String,)> = self
            .platform
            .query_as("SELECT dsp_id FROM dsp_deletions ORDER BY started_at", [])?;
        Ok(deleting.into_iter().map(|(id,)| id).collect())
    }
    /// Moves the DSP's folder out of reach of anything looking for it, in a turn no other
    /// work is under way in, and closes this store's connections to its databases: where the
    /// folder is now, to delete. One a delete cut short moved is where it went. Nothing makes
    /// the folder again, even what began writing in it before (`db::retire`).
    fn set_folder_aside(&self, dsp: &str) -> Result<PathBuf> {
        let folder = self.dsp_folder(dsp)?;
        db::retire(&folder);
        let aside = folder.with_extension("deleting");
        if folder.try_exists()? {
            // What a delete cut short left aside goes first, as one folder takes its place.
            remove(&aside)?;
            std::fs::rename(&folder, &aside)?;
        }
        self.forget_dsp(dsp);
        Ok(aside)
    }
}

/// Deletes the DSP `dsp` for good, as `allowed` lets in each turn that acts on it, answering
/// its name as the platform owner typed it and who they are. Held for the delete from the first
/// (`reserve_deletion`), the DSP's connections outside Dispatch are given back, then its rows
/// go, then what it kept elsewhere, then what the server holds of it in memory.
pub async fn delete<F>(state: &Arc<State>, dsp: &str, allowed: F) -> Result<()>
where
    F: Fn(&Store) -> Result<(String, String)> + Clone + Send + 'static,
{
    let (at, asked) = (dsp.to_owned(), allowed.clone());
    let _reserved = state
        .read(move |db| db.reserve_deletion(&at, &asked(db)?.0))
        .await?;
    revoke(state, dsp).await;
    let at = dsp.to_owned();
    state
        .run(move |db| {
            let (name, actor) = allowed(db)?;
            db.delete_dsp(&at, &name, &actor)
        })
        .await?;
    finish(state).await;
    Ok(())
}

/// Gives back what the DSP connected outside Dispatch through each feature, as its features
/// declare, whether or not they're on there: best effort, each logging its own failures.
pub async fn revoke(state: &Arc<State>, dsp: &str) {
    for revoke in registry().features.iter().filter_map(|f| f.deleting.revoke) {
        revoke(state.clone(), dsp.to_owned()).await;
    }
}

/// Finishes deleting each DSP whose rows are gone, one at a time: its jobs and what its live
/// readers recorded, then the mail development captured for it and its folder, then what the
/// server holds of it in memory, then its objects in the object store, then the note that it was
/// still to do. Each step is done again each pass until the note goes, a delete cut short
/// included; what fails is logged, and the next pass tries it again.
pub async fn finish(state: &Arc<State>) {
    static FINISHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _finishing = FINISHING.lock().await;
    let deleting = match state.read(|db| db.deletions()).await {
        Ok(deleting) => deleting,
        Err(error) => return failed(None, &error),
    };
    for dsp in deleting {
        if let Err(error) = finish_one(state, &dsp).await {
            failed(Some(&dsp), &error);
        }
    }
}

async fn finish_one(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    state
        .run_bookkeeping(move |db| db.forget_dsp_jobs(&at))
        .await?;
    let at = dsp.to_owned();
    let aside = state
        .run_bookkeeping(move |db| {
            mail::forget_captured(db)?;
            db.set_folder_aside(&at)
        })
        .await?;
    tokio::task::spawn_blocking(move || remove(&aside))
        .await
        .map_err(|_| Error::new("operation_failed", 500))??;
    forget(state, dsp).await;
    if let Some(objects) = &state.objects {
        // No upload starts once the DSP is disabled, which it was before its delete began, and
        // one let through before is given up `UPLOAD` after: a listing begun once that, and a
        // minute for the object store to settle what it was sent, has gone by since the delete
        // began finds every object an upload left. One begun before, however long it takes, is
        // done again on the next pass.
        let at = dsp.to_owned();
        let started = state.read(move |db| db.deletion_started(&at)).await?;
        let uploading = (db::objects::UPLOAD + SETTLING).as_millis() as i64;
        let last = started.is_none_or(|started| db::now() >= started + uploading);
        objects.delete_dsp(dsp).await?;
        if !last {
            return Ok(());
        }
    }
    let at = dsp.to_owned();
    state
        .run_bookkeeping(move |db| {
            db.platform
                .exec("DELETE FROM dsp_deletions WHERE dsp_id=?", [at])?;
            Ok(())
        })
        .await?;
    observability::event("info", "dsp.deleted", json!({"dspId": dsp}));
    Ok(())
}

/// Lets go of everything the server holds of a deleted DSP in memory: its browsers, its cached
/// reads, the connections idle stores keep to its databases, and what the agents and each
/// feature hold.
pub async fn forget(state: &Arc<State>, dsp: &str) {
    state.browsers.revoke(dsp).await;
    state.forget_dsp(dsp);
    if let Some(agents) = registry().agents {
        (agents.forgotten)(state, dsp);
    }
    for forgotten in registry()
        .features
        .iter()
        .filter_map(|f| f.deleting.forgotten)
    {
        forgotten(dsp);
    }
}

/// Deletes the folder and everything in it, as one already gone is.
fn remove(folder: &std::path::Path) -> Result<()> {
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

fn failed(dsp: Option<&str>, error: &Error) {
    observability::event(
        "error",
        "dsp.delete_failed",
        json!({"dspId": dsp, "error": error.code}),
    );
}

#[cfg(test)]
#[path = "../tests/backend/deletion.rs"]
mod tests;
