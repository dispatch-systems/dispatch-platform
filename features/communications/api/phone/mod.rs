//! The address a DSP's phones talk to, `/api/communications/phone`, on the DSP's own address, by
//! concern: who a call comes from (`access`), pairing by a code (`pairing`), the texts a phone
//! reports (`texts`), waiting on the texts Dispatch has for it, and the videos it keeps that
//! someone plays, and saying how they went (`outbox`), with how it stands (`health`), and their
//! pictures' bytes (`files`). No browser calls these: a request carrying a cookie, or at another
//! address than a DSP's, is refused. What a call brings is filed as it's kept, in the same write
//! (`texts::filing::changing`). The app itself (`app`), its details and its APK, is anyone's at the
//! DSP's address, a browser's too, as a phone downloads it before it's paired. The protocol, call by call, is `README.md` beside them.
mod access;
mod app;
mod files;
mod health;
mod outbox;
mod pairing;
mod texts;

use crate::backend::phone::{LIMITS, files::PREVIEW_MOST};
use access::{Address, Phone};
use dispatch_core::server::http::route::{Public, Route, async_get, async_post, upload};

/// Where a phone downloads the app, at its DSP's address, as the phones' card links to it.
pub(in crate::api) const DOWNLOAD: &str = "/api/dispatch-phone/phone/app/download";

/// The phones' routes, at the DSP's own address: pairing by a code, and the app, public; every
/// other call by the key a phone was given.
pub(in crate::api) fn routes() -> Vec<Route> {
    vec![
        async_post("/api/dispatch-phone/phone/pair", Public, pairing::pair)
            .also_at("/api/communications/phone/pair"),
        async_get("/api/dispatch-phone/phone/app", Address, app::details)
            .also_at("/api/communications/phone/app"),
        async_get(DOWNLOAD, Address, app::download)
            .also_at("/api/communications/phone/app/download"),
        async_get("/api/dispatch-phone/phone/outbox", Phone, outbox::outbox)
            .also_at("/api/communications/phone/outbox"),
        async_post("/api/dispatch-phone/phone/texts", Phone, texts::texts)
            .also_at("/api/communications/phone/texts"),
        upload(
            "/api/dispatch-phone/phone/files/{id}",
            Phone,
            LIMITS.largest,
            files::file_in,
        )
        .also_at("/api/communications/phone/files/{id}"),
        async_get(
            "/api/dispatch-phone/phone/files/{id}",
            Phone,
            files::file_out,
        )
        .also_at("/api/communications/phone/files/{id}"),
        upload(
            "/api/dispatch-phone/phone/files/{id}/preview",
            Phone,
            PREVIEW_MOST,
            files::preview_in,
        )
        .also_at("/api/communications/phone/files/{id}/preview"),
        async_post(
            "/api/dispatch-phone/phone/files/{id}/gone",
            Phone,
            files::gone,
        )
        .also_at("/api/communications/phone/files/{id}/gone"),
        async_post(
            "/api/dispatch-phone/phone/statuses",
            Phone,
            outbox::statuses,
        )
        .also_at("/api/communications/phone/statuses"),
    ]
    .into_iter()
    .flatten()
    .collect()
}
