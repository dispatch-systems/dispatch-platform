//! Driver Match's endpoints: every person the DSP's features name, and the decisions that
//! join or part them, which answer with nothing: the page reads itself again. Its one
//! permission gates them all.
use crate::backend::DriverMatchStore;
use dispatch_core::{
    Error, Result,
    db::Store,
    foundation::validate as v,
    manifest::people::Source,
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
        read("/api/dsp/driver-match/people/{code}", MANAGE, details),
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
    Reply::of(&db.match_counts(c.dsp_id())?)
}
fn details(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    Reply::of(&db.person_details(c.dsp_id(), input.param("code"))?)
}
/// The source and ID a decision names, of the sources this build has.
fn source_id(b: &serde_json::Value) -> Result<(Source, &str)> {
    let source = Source::parse(v::text(b, "source", 1, 32)?)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    Ok((source, v::text(b, "id", 1, 64)?))
}
/// `code`'s IDs move to `into`.
fn merge(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "into"])?;
    db.merge_people(c, v::text(b, "code", 6, 6)?, v::text(b, "into", 6, 6)?)?;
    Ok(Reply::ok())
}
/// One of `code`'s IDs moves to a new person.
fn split(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "source", "id"])?;
    let (source, id) = source_id(b)?;
    db.split_off(c, v::text(b, "code", 6, 6)?, source, id)?;
    Ok(Reply::ok())
}
/// One of `code`'s IDs a match joined is confirmed as theirs.
fn confirm(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "source", "id"])?;
    let (source, id) = source_id(b)?;
    db.confirm_link(c, v::text(b, "code", 6, 6)?, source, id)?;
    Ok(Reply::ok())
}
fn apart(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "other"])?;
    db.keep_apart(c, v::text(b, "code", 6, 6)?, v::text(b, "other", 6, 6)?)?;
    Ok(Reply::ok())
}
