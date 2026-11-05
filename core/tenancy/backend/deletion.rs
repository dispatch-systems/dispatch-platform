//! Deleting a DSP for good: everything of it, wherever it is, and its short code freed for
//! another. Only a disabled DSP is deleted, never the permanent one, and only by its name as
//! the platform owner typed it.
//!
//! Each step holds if the server stops partway. What the DSP connected outside Dispatch is
//! given back first, best effort. One transaction then deletes every row of it in the
//! platform's database, its own row included, and notes the delete in `dsp_deletions`: from
//! then on nothing finds the DSP, and startup no longer looks for its folder. `finish` deletes
//! what it kept elsewhere, its jobs and its folder, and lets the note go once all of it has
//! gone. A delete cut short is finished by the next upkeep pass, as the server starts too.
use crate::{
    Error, Result, State,
    db::{Store, iso},
    ensure,
    foundation::observability,
    manifest::registry,
    server::mail,
    support,
    tenancy::api::types::DspStatus,
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

impl Store {
    /// Refuses deleting the DSP `id` unless it's disabled, isn't the permanent one, and `name`
    /// is its name, exactly.
    pub fn ensure_deletable(&self, id: &str, name: &str) -> Result<()> {
        let dsp = self.find_dsp(id)?;
        ensure(!dsp.permanent, "permanent_dev_required", 409)?;
        ensure(dsp.status == DspStatus::Suspended, "dsp_not_disabled", 409)?;
        ensure(dsp.name == name, "dsp_name_mismatch", 400)
    }
    /// Deletes every row of the DSP `id` in the platform's database, as `ensure_deletable`
    /// allows: its log, mail, support tickets and switches, what each feature and the agents
    /// keep of it there, what the platform's directory holds of its people, and its own row,
    /// which frees its short code. Notes that what it kept elsewhere is still to delete
    /// (`finish`), and records who deleted it, by its name, as nothing else names it now.
    pub fn delete_dsp(&self, id: &str, name: &str, actor: &str) -> Result<()> {
        self.ensure_deletable(id, name)?;
        // Its people: in its own directory, and those the platform's held only for it.
        let own: Vec<(String,)> = self.dsp(id)?.query_as("SELECT id FROM users", [])?;
        let legacy = self.legacy_people(id)?;
        let people: Vec<String> = own
            .into_iter()
            .map(|(id,)| id)
            .chain(legacy.iter().cloned())
            .collect();
        self.platform.transaction(|| {
            self.forget_audit(id, &people)?;
            mail::forget_dsp(self, id)?;
            support::tickets::forget_dsp(self, id)?;
            if let Some(agents) = registry().agents {
                (agents.forget_dsp)(self, id)?;
            }
            for forget in registry().features.iter().filter_map(|f| f.deleting.forget) {
                forget(self, id)?;
            }
            self.forget_legacy_people(id, &legacy)?;
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
    /// The DSPs whose rows are gone and whose delete isn't finished.
    fn deletions(&self) -> Result<Vec<String>> {
        let deleting: Vec<(String,)> = self
            .platform
            .query_as("SELECT dsp_id FROM dsp_deletions ORDER BY started_at", [])?;
        Ok(deleting.into_iter().map(|(id,)| id).collect())
    }
    /// Moves the DSP's folder out of reach of anything looking for it, in a turn no other
    /// work is under way in, and closes this store's connections to its databases: where the
    /// folder is now, to delete. One a delete cut short moved is where it went.
    fn set_folder_aside(&self, dsp: &str) -> Result<PathBuf> {
        let folder = self.dsp_folder(dsp)?;
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

/// Gives back what the DSP connected outside Dispatch through each feature, as its features
/// declare, whether or not they're on there: best effort, each logging its own failures.
pub async fn revoke(state: &Arc<State>, dsp: &str) {
    for revoke in registry().features.iter().filter_map(|f| f.deleting.revoke) {
        revoke(state.clone(), dsp.to_owned()).await;
    }
}

/// Finishes deleting each DSP whose rows are gone, one at a time: its jobs and what its live
/// readers recorded, then its folder, then the note that it was still to do. What fails is
/// logged, and the next pass tries it again.
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
    // The DSP's objects in the object store go here, once the platform keeps them there
    // (`Objects::delete_dsp`): this step is tried again until every one of them has gone.
    let at = dsp.to_owned();
    let aside = state
        .run_bookkeeping(move |db| db.set_folder_aside(&at))
        .await?;
    tokio::task::spawn_blocking(move || remove(&aside))
        .await
        .map_err(|_| Error::new("operation_failed", 500))??;
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
/// reads, the connections idle stores keep to its databases, and what each feature holds.
pub async fn forget(state: &Arc<State>, dsp: &str) {
    state.browsers.revoke(dsp).await;
    state.forget_dsp(dsp);
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
