//! The address a DSP's phones talk to, `/api/communications/phone`, on the DSP's own address, by
//! concern: who a call comes from (`access`), pairing by a code (`pairing`), the texts a phone
//! reports (`texts`), waiting on the texts Dispatch has for it and saying how they went
//! (`outbox`), and their pictures' bytes (`files`). No browser calls these: a request carrying a
//! cookie, or at another address than a DSP's, is refused. Once what a call brings is kept, filing
//! hears of it (`texts::filing::told`). The protocol, call by call, is `README.md` beside them.
mod access;
mod files;
mod outbox;
mod pairing;
mod texts;

use crate::backend::phone::LIMITS;
use access::Phone;
use dispatch_core::server::http::route::{Public, Route, async_get, async_post, upload};

/// The phones' routes, at the DSP's own address: pairing by a code, public; every other call by
/// the key a phone was given.
pub(in crate::api) fn routes() -> Vec<Route> {
    vec![
        async_post("/api/communications/phone/pair", Public, pairing::pair),
        async_get("/api/communications/phone/outbox", Phone, outbox::outbox),
        async_post("/api/communications/phone/texts", Phone, texts::texts),
        upload(
            "/api/communications/phone/files/{id}",
            Phone,
            LIMITS.largest,
            files::file_in,
        ),
        async_get(
            "/api/communications/phone/files/{id}",
            Phone,
            files::file_out,
        ),
        async_post(
            "/api/communications/phone/statuses",
            Phone,
            outbox::statuses,
        ),
    ]
}
