//! The public pages an invitation links to: reading it, and accepting it, which makes the
//! member and, for a DSP's first owner, its profile. Whoever invites does so through a
//! feature's routes, calling `Store::invite`.
use crate::{
    Result, State,
    accounts::api::requests::InvitationRequest,
    db::Store,
    server::http::{
        input::{Input, Reply},
        route::{Anyone, Public, Route, async_post, write},
    },
};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        // Reading an invitation counts against a throttle, which is a write.
        write("/api/invitations/{token}", Public, invitation).get(),
        async_post("/api/invitations/{token}/accept", Public, accept_invitation),
    ]
}

fn invitation(db: &Store, _: &Anyone, input: &Input) -> Result<Reply> {
    db.throttle_ip("invite-read", &input.ip, 60, 60000)?;
    Ok(Reply::json(db.invitation_link(input.param("token"))?))
}

async fn accept_invitation(state: Arc<State>, input: Input, _: Public) -> Result<Reply> {
    let request = InvitationRequest::parse(&input.body)?;
    let ip = input.ip.clone();
    state
        .run(move |db| db.throttle_ip("invite-ip", &ip, 20, 3600000))
        .await?;
    let token = input.param("token").to_owned();
    let setup = request.dsp_profile.is_some();
    let joined = state.accept_invitation(token, request, input.ip).await?;
    if setup {
        state
            .schedule_revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    Ok(Reply::json(joined))
}
