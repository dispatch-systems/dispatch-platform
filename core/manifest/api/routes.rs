//! Everyone a DSP's features name, as the directory tells them apart, for every member: who
//! they are, their contacts and their photo. Each answers nothing in a build without the
//! directory.
use crate::{
    Error, Result,
    db::Store,
    manifest::people,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read},
    },
    tenancy::roles,
};

/// Every member of the DSP, whatever their role.
const MEMBERS: Dsp = Dsp(roles::ACCESS);
pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/people", MEMBERS, everyone),
        read("/api/dsp/people/{code}", MEMBERS, person),
        read("/api/dsp/people/{code}/photo", MEMBERS, photo),
    ]
}
/// Everyone, by name.
fn everyone(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let everyone = people::everyone(db, c.state, c.dsp_id())?;
    Reply::of(&everyone.iter().collect::<Vec<_>>())
}
/// One person; a merged person's code opens whoever kept their IDs.
fn person(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let everyone = people::everyone(db, c.state, c.dsp_id())?;
    Reply::of(
        everyone
            .person(input.param("code"))
            .ok_or_else(|| Error::new("not_found", 404))?,
    )
}
/// Their photo. The address carries its version, so the browser keeps it until the photo
/// changes.
fn photo(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (kind, bytes) = people::photo(db, c.dsp_id(), input.param("code"))?
        .ok_or_else(|| Error::new("not_found", 404))?;
    Ok(Reply::picture(kind, bytes.into()))
}
