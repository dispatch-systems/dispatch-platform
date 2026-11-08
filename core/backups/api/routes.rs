use super::types::BackupRuns;
use crate::{
    Error, Result, State,
    backups::{download, history, schedule},
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Grant, PlatformOwner, Route, User, async_get, async_post, read, write},
    },
};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/platform/dsps/{id}/backups", PlatformOwner, backups),
        write(
            "/api/platform/dsps/{id}/backups/schedule",
            PlatformOwner,
            save,
        ),
        write(
            "/api/platform/dsps/{id}/backups/switch",
            PlatformOwner,
            switch,
        ),
        write(
            "/api/platform/dsps/{id}/backups/preview",
            PlatformOwner,
            preview,
        ),
        async_post("/api/platform/dsps/{id}/backups/now", PlatformOwner, now),
        async_get(
            "/api/platform/dsps/{id}/backups/{backup}/download",
            PlatformOwner,
            saved,
        ),
    ]
}

fn shown(db: &Store, dsp: &str) -> Result<Reply> {
    Reply::of(&history::shown(db, dsp, db.config.backups.is_some())?)
}

/// The DSP's Backups tab.
fn backups(db: &Store, _: &User, input: &Input) -> Result<Reply> {
    shown(db, input.param("id"))
}

/// Saves the DSP's backup rules.
fn save(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let dsp = input.param("id");
    schedule::save(db, dsp, &input.body)?;
    db.audit(
        Some(owner.actor()),
        Some(dsp),
        "dsp.backup_schedule_changed",
        "",
    )?;
    shown(db, dsp)
}

/// Turns the DSP's backup schedule on or off.
fn switch(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let dsp = input.param("id");
    v::fields(&input.body, &["enabled"])?;
    let enabled = v::boolean(&input.body, "enabled")?;
    schedule::switch(db, dsp, enabled)?;
    db.audit(
        Some(owner.actor()),
        Some(dsp),
        "dsp.backup_schedule_changed",
        if enabled { "on" } else { "off" },
    )?;
    shown(db, dsp)
}

/// When rules not saved yet would run next.
fn preview(db: &Store, _: &User, input: &Input) -> Result<Reply> {
    let (upcoming, calendar) = schedule::preview(db, input.param("id"), &input.body, 5)?;
    Reply::of(&BackupRuns {
        upcoming,
        calendar: history::days(calendar),
    })
}

/// Takes a backup of the DSP now, unless one is waiting or being taken already.
async fn now(state: Arc<State>, input: Input, access: PlatformOwner) -> Result<Reply> {
    let reply = state
        .run(move |db| {
            let owner = access.authorize(db, &input)?;
            let dsp = input.param("id");
            v::fields(&input.body, &[])?;
            crate::ensure(db.config.backups.is_some(), "backups_not_configured", 409)?;
            schedule::start(db, dsp, schedule::timezone(db, dsp)?)?;
            history::ask_by_hand(&db.platform, dsp, &owner.user.id)?;
            db.audit(Some(&owner.user.id), Some(dsp), "dsp.backup_requested", "")?;
            shown(db, dsp)
        })
        .await?;
    state.backups_wake.notify_one();
    Ok(reply)
}

/// One of the DSP's backups, as a zip of its files: the owner is asked for again once it's put
/// together, before any of it is sent.
async fn saved(state: Arc<State>, input: Input, access: PlatformOwner) -> Result<Reply> {
    let vault = state
        .backups
        .as_ref()
        .ok_or_else(|| Error::new("backups_not_configured", 409))?;
    let (dsp, id) = (
        input.param("id").to_owned(),
        input.param("backup").to_owned(),
    );
    {
        let (dsp, id, input) = (dsp.clone(), id.clone(), input.clone());
        state
            .run(move |db| {
                access.authorize(db, &input)?;
                history::files(&db.platform, &dsp, &id).map(drop)
            })
            .await?;
    }
    let zipped = download::download(&state, vault, &dsp, &id).await?;
    state
        .run(move |db| {
            let owner = access.authorize(db, &input)?;
            db.audit(
                Some(&owner.user.id),
                Some(&dsp),
                "dsp.backup_downloaded",
                &id,
            )
        })
        .await?;
    Ok(zipped.reply())
}
