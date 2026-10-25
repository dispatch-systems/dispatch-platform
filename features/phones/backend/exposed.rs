//! What Phones exposes to every other feature: `texting`, through its contract
//! (`dispatch_phones_contract`).
use super::{
    files, live,
    storage::{DOMAIN, PhonesStore, ReportedFile},
};
use dispatch_core::{
    Result, State,
    db::{self, Store},
    server::http::{input::Reply, upload::Upload},
};
use dispatch_phones_contract::{
    Added, Answer, Changes, File, Outgoing, Phone, Status, Text, Texting,
};
use std::sync::Arc;

/// Phones' Texting, as its contract says.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl Texting for Exposed {
    fn phones(&self, db: &Store, dsp: &str) -> Result<Vec<Phone>> {
        let now = db::now();
        Ok(db
            .phones_devices(dsp)?
            .iter()
            .map(|device| device.phone(now))
            .collect())
    }
    fn changes(&self, db: &Store, dsp: &str, after: i64, most: usize) -> Result<Changes> {
        db.phones_changes(dsp, after, most)
    }
    fn send<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        outgoing: Outgoing,
    ) -> Answer<'a, Text> {
        Box::pin(async move {
            let at = dsp.to_owned();
            let text = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    let now = db::now();
                    let text = db.phones_send(&at, &outgoing, now)?;
                    // The demo's stand-in phone, which no app holds, says it delivered at once.
                    if db.phones_stand_in(&at, &text.phone)? {
                        db.phones_status(&at, &text.phone, &text.id, Status::Sent, now, None)?;
                        db.phones_status(&at, &text.phone, &text.id, Status::Delivered, now, None)?;
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
    fn retry<'a>(&'a self, state: &'a Arc<State>, dsp: &'a str, id: &'a str) -> Answer<'a, Text> {
        Box::pin(async move {
            let (at, wanted) = (dsp.to_owned(), id.to_owned());
            let text = state
                .run_scoped(dsp, DOMAIN, move |db| db.phones_retry(&at, &wanted, db::now()))
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
            let (sha256, size) = files::keep(state, dsp, bytes).await?;
            let (at, kept) = (dsp.to_owned(), sha256.clone());
            let file = ReportedFile {
                name: added.name,
                mime: added.mime,
                size,
            };
            let stored = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    db.phones_add_file(&at, &added.by, &file, &kept, db::now())
                })
                .await;
            if stored.is_err() {
                files::discard(state, dsp, &sha256).await?;
            }
            stored
        })
    }
    fn largest(&self) -> u64 {
        files::LARGEST
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
