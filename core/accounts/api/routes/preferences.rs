//! A member's own preferences in the DSP their view is of (`accounts::preferences`), which come
//! with that view and are saved here, one at a time.
use crate::{
    Error, Result,
    accounts::preferences,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, write},
    },
    tenancy::roles,
};

pub fn routes() -> Vec<Route> {
    // A member's own, which no cached read holds.
    vec![write("/api/dsp/preferences/{id}", Dsp(roles::ACCESS), save).bookkeeping()]
}

// Keeps the member's value of a preference they have here, or clears it with none, made from
// the revision they read: theirs alone, so it is not audited and expires no view.
fn save(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let body = &input.body;
    v::fields(body, &["revision", "value"])?;
    let preference = preferences::shown(c)
        .find(|preference| preference.id == input.param("id"))
        .ok_or_else(|| Error::new("not_found", 404))?;
    let revision = body["revision"]
        .as_i64()
        .filter(|revision| *revision >= 0)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let value = Some(&body["value"]).filter(|value| !value.is_null());
    Reply::of(&db.save_preference(c.dsp_id(), c.actor(), preference, revision, value)?)
}
