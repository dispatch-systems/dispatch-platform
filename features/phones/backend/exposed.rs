//! What Phones exposes to every other feature: `texting`, through its contract
//! (`dispatch_phones_contract`).
use super::{
    LIMITS, files, live,
    storage::{DOMAIN, DeviceStore, FileStore, TextStore},
};
use dispatch_core::{
    Result, State,
    db::{self, Store},
    ensure,
    server::http::{input::Reply, upload::Upload},
};
use dispatch_phones_contract::{
    Added, Answer, Changes, Condition, File, Limits, Outgoing, Phone, Text, Texting,
};
use std::sync::Arc;

/// Phones' Texting, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

/// Refuses, in the write that queues a text, once Phones went off for the DSP since its caller
/// found it on; then as its caller's `condition` refuses.
fn queuable(db: &Store, dsp: &str, condition: Condition) -> Result<()> {
    ensure(
        db.feature_enabled(dsp, crate::FEATURE.switch.id)?,
        "phones_off",
        409,
    )?;
    condition(db)
}

impl Texting for Exposed {
    fn phones(&self, db: &Store, dsp: &str) -> Result<Vec<Phone>> {
        let now = db::now();
        Ok(db
            .phones_devices(dsp)?
            .iter()
            .map(|device| device.phone(now))
            .collect())
    }
    fn changes(&self, db: &Store, dsp: &str, after: i64) -> Result<Changes> {
        db.phones_changes(dsp, after)
    }
    fn send<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        outgoing: Outgoing,
        condition: Condition,
    ) -> Answer<'a, Text> {
        Box::pin(async move {
            let at = dsp.to_owned();
            let text = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    queuable(db, &at, condition)?;
                    let now = db::now();
                    let text = db.phones_send(&at, &outgoing, now)?;
                    // The demo's stand-in phone, which no app holds, says it delivered at once.
                    if db.phones_stand_in(&at, &text.phone)? {
                        db.phones_stand_in_delivered(&at, &text.id, now)?;
                        return db
                            .phones_text(&at, &text.id)
                            .map(|delivered| delivered.unwrap_or(text));
                    }
                    Ok(text)
                })
                .await?;
            live::queued(state, dsp);
            live::changed(state, dsp).await;
            Ok(text)
        })
    }
    fn retry<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        condition: Condition,
    ) -> Answer<'a, Text> {
        Box::pin(async move {
            let (at, wanted) = (dsp.to_owned(), id.to_owned());
            let text = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    queuable(db, &at, condition)?;
                    db.phones_retry(&at, &wanted, db::now())
                })
                .await?;
            live::queued(state, dsp);
            live::changed(state, dsp).await;
            Ok(text)
        })
    }
    fn add_file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        added: Added,
        bytes: Upload,
    ) -> Answer<'a, File> {
        Box::pin(async move {
            ensure(bytes.length <= LIMITS.largest, "upload_too_large", 413)?;
            let arrived = files::receive(state, dsp, bytes).await?;
            let at = dsp.to_owned();
            state
                .run_scoped(dsp, DOMAIN, move |db| {
                    files::keep_added(db, &at, added, &arrived)
                })
                .await
        })
    }
    fn remove_file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        by: &'a str,
    ) -> Answer<'a, ()> {
        Box::pin(async move {
            let (at, id, by) = (dsp.to_owned(), id.to_owned(), by.to_owned());
            state
                .run_scoped(dsp, DOMAIN, move |db| {
                    match db.phones_remove_file(&at, &id, &by)? {
                        Some(sha256) => files::forget(db, &at, &sha256),
                        None => Ok(()),
                    }
                })
                .await
        })
    }
    fn limits(&self) -> Limits {
        LIMITS
    }
    fn file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        viewer: &'a str,
    ) -> Answer<'a, Option<Reply>> {
        Box::pin(async move {
            let (at, wanted) = (dsp.to_owned(), id.to_owned());
            let Some(stored) = state.read(move |db| db.phones_file(&at, &wanted)).await? else {
                return Ok(None);
            };
            let shown = stored.text_id.is_some() || stored.added_by.as_deref() == Some(viewer);
            match stored.sha256 {
                Some(sha256) if shown => Ok(Some(
                    files::answer(state, dsp, &sha256, &stored.file.name, &stored.file.mime)
                        .await?,
                )),
                _ => Ok(None),
            }
        })
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
