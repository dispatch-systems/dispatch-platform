//! Driver Match: every person the DSP's collections know, and the decisions that join or
//! part them. The tab lives in Settings; its part's switch and its one permission gate it.
use crate::backend::driver_match::DriverMatchStore;
use dispatch_core::{
    Error, Result,
    db::Store,
    foundation::validate as v,
    manifest::people::DriverSource,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
};

const MANAGE: Dsp = Dsp("driver_match.manage");
pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/driver-match", MANAGE, overview),
        read("/api/dsp/driver-match/counts", MANAGE, counts),
        read("/api/dsp/driver-match/drivers/{code}", MANAGE, details),
        read("/api/dsp/driver-match/drivers/{code}/photo", MANAGE, photo),
        write("/api/dsp/driver-match/merge", MANAGE, merge),
        write("/api/dsp/driver-match/split", MANAGE, split),
        write("/api/dsp/driver-match/apart", MANAGE, apart),
        write("/api/dsp/driver-match/confirm", MANAGE, confirm),
    ]
}
fn overview(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.driver_match(c.dsp_id())?)
}
fn counts(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.driver_counts(c.dsp_id())?)
}
fn details(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    Reply::of(&db.driver_details(c.dsp_id(), input.param("code"))?)
}
/// Their photo from Amazon's roster. The address carries its version, so the browser keeps
/// it until the photo changes.
fn photo(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (kind, bytes) = db
        .driver_photo(c.dsp_id(), input.param("code"))?
        .ok_or_else(|| Error::new("not_found", 404))?;
    Ok(Reply::picture(kind, bytes.into()))
}
/// The source and ID a decision names, of the sources this build has.
fn source_id(b: &serde_json::Value) -> Result<(DriverSource, &str)> {
    let source = DriverSource::parse(v::text(b, "source", 1, 32)?)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    Ok((source, v::text(b, "id", 1, 64)?))
}
/// `code`'s IDs move to `into`.
fn merge(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "into"])?;
    Reply::of(&db.merge_drivers(c, v::text(b, "code", 6, 6)?, v::text(b, "into", 6, 6)?)?)
}
/// One of `code`'s IDs moves to a new person.
fn split(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "source", "id"])?;
    let (source, id) = source_id(b)?;
    Reply::of(&db.split_driver(c, v::text(b, "code", 6, 6)?, source, id)?)
}
/// One of `code`'s IDs a match joined is confirmed as theirs.
fn confirm(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "source", "id"])?;
    let (source, id) = source_id(b)?;
    Reply::of(&db.confirm_driver(c, v::text(b, "code", 6, 6)?, source, id)?)
}
fn apart(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "other"])?;
    Reply::of(&db.keep_drivers_apart(c, v::text(b, "code", 6, 6)?, v::text(b, "other", 6, 6)?)?)
}
