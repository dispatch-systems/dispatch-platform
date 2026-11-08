//! The DSP's quick replies: read as the composer offers them, and saved whole from Settings, by
//! anyone who uses Communications.
use crate::backend::quick_replies;
use dispatch_core::{
    Error, Result,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Member,
    },
};

/// The DSP's quick replies, and their revision.
pub(super) fn quick_replies(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&quick_replies::quick_replies(db, c.dsp_id())?)
}
/// Saves `replies` as the DSP's, in their order, on the `revision` they were read at: as many
/// and as long as `quick_replies` allows.
pub(super) fn save_quick_replies(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["replies", "revision"])?;
    let revision = v::integer(&input.body, "revision", 0, i64::MAX)?;
    let replies = input.body["replies"]
        .as_array()
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(|reply| {
            reply
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::new("invalid_input", 400))
        })
        .collect::<Result<Vec<_>>>()?;
    Reply::of(&quick_replies::save_quick_replies(
        db,
        c.dsp_id(),
        &replies,
        revision,
        c.actor(),
    )?)
}
