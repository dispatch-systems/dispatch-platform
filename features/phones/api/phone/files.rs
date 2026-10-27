//! The bytes of the pictures and files texts carry: those of a text the phone reported, which it
//! sends, and those of a text Dispatch has for it, which it fetches.
use super::access::Phone;
use crate::backend::{
    files, live,
    storage::{DOMAIN, FileStore},
};
use dispatch_core::{
    Error, Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Grant,
        upload::Upload,
    },
};
use std::sync::Arc;

/// Keeps the bytes of a file of a text the phone reported.
pub(in crate::api) async fn file_in(
    state: Arc<State>,
    input: Input,
    access: Phone,
    bytes: Upload,
) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let asked = input.clone();
    let caller = state.read(move |db| access.authorize(db, &asked)).await?;
    let arrived = files::receive(&state, &caller.dsp, bytes).await?;
    let (asked, id) = (input.clone(), input.param("id").to_owned());
    state
        .run_scoped(caller.dsp.clone(), DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            files::keep_reported(db, &caller.dsp, &caller.device.id, &id, &arrived)
        })
        .await?;
    live::changed(&state, &caller.dsp).await;
    Ok(Reply::ok())
}
/// The bytes of a picture of a text Dispatch has for the phone to send: asked again, once the
/// download is ready, whether the phone still may.
pub(in crate::api) async fn file_out(
    state: Arc<State>,
    input: Input,
    access: Phone,
) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let (asked, id) = (input.clone(), input.param("id").to_owned());
    let (dsp, file) = state
        .read(move |db| {
            let caller = access.authorize(db, &asked)?;
            let file = db.phones_outgoing_file(&caller.dsp, &caller.device.id, &id)?;
            Ok((caller.dsp, file))
        })
        .await?;
    let file = file.ok_or_else(|| Error::new("phones_file_not_found", 404))?;
    let sha256 = file
        .sha256
        .ok_or_else(|| Error::new("phones_file_not_found", 404))?;
    let reply = files::answer(&state, &dsp, &sha256, &file.file.name, &file.file.mime).await?;
    let asked = input.clone();
    state.read(move |db| access.authorize(db, &asked)).await?;
    Ok(reply)
}
