//! GIFs of KLIPY's, while the DSP has them (`gifs::PART`): where the page searches them, and in
//! fixture mode the server's stand-in for KLIPY, so the dev server and tests never call it.
use crate::backend::gifs;
use dispatch_core::{
    Error, Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text},
        route::{Dsp, Grant, Member},
    },
};
use std::sync::Arc;

/// Where the page searches KLIPY's GIFs, for the member.
pub(super) fn search(_: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    ensure(c.has(gifs::PART), "not_found", 404)?;
    Reply::of(&gifs::search(&c.state.config, c.dsp_id(), c.actor())?)
}

/// Fixture mode's stand-in for KLIPY's trending GIFs, `.../gifs/trending`, or its search for `q`,
/// `.../gifs/search`, as KLIPY answers either.
pub(super) fn fixture_results(_: &Store, c: &Member, input: &Input) -> Result<Reply> {
    fixture(c)?;
    let q = optional_text(&input.query, "q", 200)?;
    let found = match input.param("list") {
        "trending" => gifs::fixture_results(None),
        "search" => gifs::fixture_results(Some(q)),
        _ => return Err(Error::new("not_found", 404)),
    };
    Ok(Reply::json(found))
}

/// Fixture mode's stand-in for KLIPY hearing a GIF was shared.
pub(super) async fn fixture_shared(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let c = state.read(move |db| access.authorize(db, &input)).await?;
    ensure(c.has(gifs::PART) && state.config.fixture, "not_found", 404)?;
    Ok(Reply::json(serde_json::json!({ "result": true })))
}

/// Fixture mode's stand-in for a GIF at KLIPY's address.
pub(super) fn fixture_media(_: &Store, c: &Member, _: &Input) -> Result<Reply> {
    fixture(c)?;
    Ok(Reply::picture("image/gif", gifs::FIXTURE_GIF.into()))
}

/// Refused but in fixture mode, while the DSP has GIFs.
fn fixture(c: &Member) -> Result<()> {
    ensure(c.has(gifs::PART), "not_found", 404)?;
    if c.state.config.fixture {
        Ok(())
    } else {
        Err(Error::new("not_found", 404))
    }
}
