//! How a live reader reads for the DSP, which Settings shows with the section that sets when it
//! reads (`Section::reader`), under that section's permission, while its feature is on; and the
//! readers' clock, which only a test server lets its platform owner move.
use crate::{
    Error, Result, State,
    collection::readers,
    db::Store,
    ensure,
    foundation::validate as v,
    manifest::registry,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member, PlatformOwner, Route, async_post, read},
    },
};
use std::sync::{Arc, LazyLock};

/// Every permission a section showing a live reader is under, any of which reaches the route.
static READING: LazyLock<String> = LazyLock::new(|| {
    let mut permissions: Vec<&str> = registry()
        .sections()
        .filter(|(_, section)| section.reader.is_some())
        .map(|(_, section)| section.permission)
        .collect();
    permissions.sort();
    permissions.dedup();
    permissions.join("|")
});

/// The readers' clock, and how a reader reads for the DSP, which a build whose features show no
/// live reader has no route for.
pub fn routes() -> Vec<Route> {
    let mut routes = vec![async_post(
        "/api/platform/live-readers/clock",
        PlatformOwner,
        move_clock,
    )];
    if !READING.is_empty() {
        routes.push(read(
            "/api/dsp/live-readers/{reader}",
            Dsp(READING.as_str()),
            reading,
        ));
    }
    routes
}

/// The reader the path names, as the section showing it is seen: its feature and part on for
/// the DSP and shown to the member, and its permission held.
fn reading(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let reader = input.param("reader");
    let (_, section) = registry()
        .sections()
        .find(|(feature, section)| {
            section.reader == Some(reader)
                && feature.is_some_and(|feature| c.has(feature.switch.id))
                && section.part.is_none_or(|part| c.has(part))
        })
        .ok_or_else(|| Error::new("not_found", 404))?;
    ensure(c.allows(section.permission), "permission_denied", 403)?;
    Reply::of(&readers::reading(db, &c.dsp, reader)?)
}

/// Moves the readers' clock on to `at`, in milliseconds since the epoch, never back, so a test
/// reads at each slot rather than wait for it: refused unless the server was started to let a
/// test move it (`fixture_forbidden`), as only a development server with fixture providers is.
async fn move_clock(state: Arc<State>, input: Input, access: PlatformOwner) -> Result<Reply> {
    let at = state
        .read(move |db| {
            access.authorize(db, &input)?;
            ensure(db.config.movable_clock, "fixture_forbidden", 403)?;
            v::fields(&input.body, &["at"])?;
            v::integer(&input.body, "at", 0, i64::MAX)
        })
        .await?;
    readers::move_clock(at);
    Ok(Reply::ok())
}
