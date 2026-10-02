//! Driver Match: every person the DSP's collections know, and the decisions that join or
//! part them. The tab lives in Settings; the feature switch and its one permission gate it.
use crate::{
    Result,
    contracts::DriverSource,
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
    validate as v,
};

const MANAGE: Dsp = Dsp("driver_match.manage");
pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/driver-match", MANAGE, overview),
        read("/api/dsp/driver-match/counts", MANAGE, counts),
        read("/api/dsp/driver-match/drivers/{code}", MANAGE, details),
        write("/api/dsp/driver-match/merge", MANAGE, merge),
        write("/api/dsp/driver-match/split", MANAGE, split),
        write("/api/dsp/driver-match/apart", MANAGE, apart),
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
    let source = DriverSource::parse(v::choice(b, "source", &["paycom", "amazon"])?)
        .expect("a listed choice");
    Reply::of(&db.split_driver(
        c,
        v::text(b, "code", 6, 6)?,
        source,
        v::text(b, "id", 1, 64)?,
    )?)
}
fn apart(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["code", "other"])?;
    Reply::of(&db.keep_drivers_apart(c, v::text(b, "code", 6, 6)?, v::text(b, "other", 6, 6)?)?)
}
