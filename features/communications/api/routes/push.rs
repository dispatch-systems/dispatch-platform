//! Telling the member of new messages in a browser with Dispatch closed (`backend::push`): the key
//! its subscription is made with, and keeping or letting go of the subscription, theirs alone.
use crate::{
    api::types::PushKey,
    backend::{
        push::{
            self,
            storage::{Address, PushStore, Subscription},
            webpush,
        },
        storage::DOMAIN,
    },
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    ensure,
    foundation::{config::Site, validate as v},
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member},
    },
};
use serde_json::Value;
use std::sync::Arc;

/// The longest a push service's address for a browser may be, in characters.
const ENDPOINT_MOST: usize = 2000;
/// The push services browsers subscribe with, by their hosts': Chrome's and those built on it,
/// Firefox's, Safari's and Edge's. Pushes go to no other address.
const SERVICES: &[&str] = &[
    "fcm.googleapis.com",
    "push.services.mozilla.com",
    "push.apple.com",
    "notify.windows.com",
];

/// The key a browser subscribes with: none while this server pushes nothing.
pub(super) fn key(db: &Store, _: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&PushKey {
        key: webpush::vapid(&db.config).map(|vapid| vapid.public.clone()),
    })
}

/// Keeps the browser the subscription names, `endpoint` and `keys` (`p256dh`, `auth`), as the
/// member's, then watches the DSP for what to push.
pub(super) async fn subscribe(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    ensure(
        webpush::vapid(&state.config).is_some(),
        "communications_push_off",
        409,
    )?;
    v::fields(&input.body, &["endpoint", "keys"])?;
    let endpoint = endpoint(&input.body)?;
    let keys = &input.body["keys"];
    v::fields(keys, &["p256dh", "auth"])?;
    let (p256dh, auth) = (
        v::text(keys, "p256dh", 1, 100)?,
        v::text(keys, "auth", 1, 50)?,
    );
    ensure(webpush::keys_valid(p256dh, auth), "invalid_input", 400)?;
    // Where it was subscribed, whose directory holds the member's account.
    let address = match input.site {
        Site::Admin => Address::Admin,
        Site::Dsp(_) => Address::Dsp,
        Site::Invite => return Err(Error::new("invalid_input", 400)),
    };
    let subscription = Subscription {
        endpoint,
        user: c.actor().to_owned(),
        p256dh: p256dh.to_owned(),
        auth: auth.to_owned(),
        address,
    };
    let dsp = c.dsp.id.clone();
    state
        .run_scoped(&dsp, DOMAIN, move |db| {
            let c = access.revalidate(db, &c)?;
            db.keep_communications_push(&c.dsp.id, &subscription)
        })
        .await?;
    push::watch(&state, &dsp);
    Ok(Reply::ok())
}

/// Lets go of the member's browser at `endpoint`.
pub(super) fn unsubscribe(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["endpoint"])?;
    let endpoint = v::text(&input.body, "endpoint", 1, ENDPOINT_MOST)?;
    db.remove_communications_push(c.dsp_id(), c.actor(), endpoint)?;
    Ok(Reply::ok())
}

/// The address a browser's pushes go to, `endpoint`: one at a push service browsers use.
fn endpoint(body: &Value) -> Result<String> {
    let text = v::text(body, "endpoint", 1, ENDPOINT_MOST)?;
    let address = url::Url::parse(text).map_err(|_| Error::new("invalid_input", 400))?;
    let host = address.host_str().unwrap_or_default();
    let known = SERVICES
        .iter()
        .any(|service| host == *service || host.ends_with(&format!(".{service}")));
    ensure(
        address.scheme() == "https"
            && known
            && address.port().is_none()
            && address.username().is_empty()
            && address.password().is_none(),
        "invalid_input",
        400,
    )?;
    Ok(text.to_owned())
}
