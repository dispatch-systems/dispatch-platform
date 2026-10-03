//! Collection jobs: the platform's list of every DSP's, and the routes a feature lists and
//! cancels the jobs of the collections it keeps with.
use crate::{
    Result, State,
    db::Store,
    http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member, PlatformOwner, Route, User, async_post, read},
    },
    validate as v,
};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![read("/api/platform/jobs", PlatformOwner, all_jobs)]
}

fn all_jobs(db: &Store, _: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.recent_jobs(None)?)
}

/// `GET path`: the DSP's recent jobs of `kinds`, however many of other kinds came since.
/// Each feature lists only the jobs of what it keeps, so a feature added later is apart
/// from the others' without a list to keep.
pub fn job_list(path: &'static str, access: Dsp, kinds: &'static [&'static str]) -> Route {
    read(path, access, move |db: &Store, c: &Member, _: &Input| {
        let jobs = match kinds {
            [kind] => db.recent_jobs_of(c.dsp_id(), kind)?,
            kinds => db.recent_jobs_in(c.dsp_id(), kinds)?,
        };
        Reply::of(&jobs)
    })
}

/// `POST path`, where `{id}` names one of the DSP's jobs: cancels it, if it is of one of
/// `kinds` and the member holds the permission its collection runs under.
pub fn job_cancel(path: &'static str, access: Dsp, kinds: &'static [&'static str]) -> Route {
    async_post(path, access, move |state, input, access| {
        cancel(state, input, access, kinds)
    })
}

// A running job owns a browser, which is closed outside the database; the
// member's permission is then checked once more before the answer is given.
async fn cancel(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    kinds: &'static [&'static str],
) -> Result<Reply> {
    let job = input.param("id").to_owned();
    let (context, result, active_revision, required) = state
        .run(move |db| {
            let c = access.authorize(db, &input)?;
            v::fields(&input.body, &[])?;
            let row = db.job_row(&job, Some(&c.dsp.id))?;
            // A feature's route cancels only the jobs of what it keeps, and only for a
            // member who holds the permission they run under.
            let required = crate::features::collection_permission(row.kind.as_str());
            crate::ensure(
                kinds.contains(&row.kind.as_str()) && c.allows(&required),
                "permission_denied",
                403,
            )?;
            let active_revision = row
                .status
                .is_leased()
                .then(|| (row.connection_revision, row.provider()));
            let result = db.cancel(&job, &c.dsp.id)?;
            c.audit(db, "collection.cancelled", "")?;
            Ok((c, result, active_revision, required))
        })
        .await?;
    if let Some((revision, provider)) = active_revision {
        state
            .browsers
            .revoke_provider_revision(context.dsp.id.as_str(), revision, provider)
            .await;
    }
    state
        .run(move |db| {
            let context = access.revalidate(db, &context)?;
            crate::ensure(context.allows(&required), "permission_denied", 403)
        })
        .await?;
    Reply::of(&result)
}
