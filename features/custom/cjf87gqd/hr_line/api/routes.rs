//! HR Line's endpoints, each registered with its path and access: its texting, as its page reads
//! and answers it, behind `cjf87gqd_hr_line.use` (Use HR Line); and managing its phones, as its
//! card on Settings' DSP Connections does, behind `cjf87gqd_hr_line.phones` (Manage HR Line
//! Phones) or core's `connections.manage` (Manage DSP Connections). Each is Dispatch Phone's for a
//! line of its phones (`lines`), for HR Line's alone: it never lists, opens or changes Dispatch
//! Phone's own conversations or phones, nor does Dispatch Phone list, open or change HR Line's.
use crate::{LINE, PUSHED};
use dispatch_communications::lines::{self, DOMAIN};
use dispatch_core::server::http::{
    route::{Dsp, Route, async_get, async_post, read, upload, write},
    upload::UPLOAD_LIMIT,
};

/// Reading HR Line's conversations and answering them.
const USE: Dsp = Dsp("cjf87gqd_hr_line.use");
/// Managing HR Line's phones, as managing all the DSP's connections includes.
const MANAGE: Dsp = Dsp("connections.manage|cjf87gqd_hr_line.phones");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/cjf87gqd-hr-line", USE, |db, c, input| {
            lines::overview(db, c, input, Some(LINE))
        }),
        read("/api/dsp/cjf87gqd-hr-line/waiting", USE, |db, c, input| {
            lines::waiting(db, c, input, Some(LINE))
        }),
        async_get(
            "/api/dsp/cjf87gqd-hr-line/updates",
            USE,
            |state, input, access| lines::updates(state, input, access, Some(LINE)),
        ),
        read(
            "/api/dsp/cjf87gqd-hr-line/conversations",
            USE,
            |db, c, input| lines::found(db, c, input, Some(LINE)),
        ),
        read(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}",
            USE,
            |db, c, input| lines::conversation(db, c, input, Some(LINE)),
        ),
        read(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/window",
            USE,
            |db, c, input| lines::window(db, c, input, Some(LINE)),
        ),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/messages",
            USE,
            |state, input, access| lines::send(state, input, access, Some(LINE)),
        ),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/messages/{message}/retry",
            USE,
            |state, input, access| lines::retry(state, input, access, Some(LINE)),
        ),
        write(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/done",
            USE,
            |db, c, input| lines::done(db, c, input, Some(LINE)),
        )
        .changes(DOMAIN),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/typing",
            USE,
            |state, input, access| lines::typing(state, input, access, Some(LINE)),
        ),
        write(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/contact",
            USE,
            |db, c, input| lines::save_contact(db, c, input, Some(LINE)),
        )
        .changes(DOMAIN),
        write(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/block",
            USE,
            |db, c, input| lines::block(db, c, input, Some(LINE)),
        )
        .changes(DOMAIN),
        write(
            "/api/dsp/cjf87gqd-hr-line/conversations/{id}/unblock",
            USE,
            |db, c, input| lines::unblock(db, c, input, Some(LINE)),
        )
        .changes(DOMAIN),
        write(
            "/api/dsp/cjf87gqd-hr-line/texts/conversations",
            USE,
            |db, c, input| lines::text_first(db, c, input, Some(LINE)),
        )
        .changes(DOMAIN),
        read(
            "/api/dsp/cjf87gqd-hr-line/texts/contacts",
            USE,
            |db, c, input| lines::contacts(db, c, input, Some(LINE)),
        ),
        async_get(
            "/api/dsp/cjf87gqd-hr-line/texts/files/{id}",
            USE,
            |state, input, access| lines::file(state, input, access, Some(LINE)),
        ),
        async_get(
            "/api/dsp/cjf87gqd-hr-line/texts/files/{id}/preview",
            USE,
            |state, input, access| lines::file_preview(state, input, access, Some(LINE)),
        ),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/texts/files/{id}/fetch",
            USE,
            |state, input, access| lines::fetch_file(state, input, access, Some(LINE)),
        ),
        // As large as any upload: the phone side refuses a picture past what a text carries.
        upload(
            "/api/dsp/cjf87gqd-hr-line/texts/uploads",
            USE,
            UPLOAD_LIMIT,
            |state, input, access, photo| lines::upload(state, input, access, photo, Some(LINE)),
        ),
        read("/api/dsp/cjf87gqd-hr-line/push", USE, lines::push_key),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/push",
            USE,
            |state, input, access| lines::push_subscribe(state, input, access, &PUSHED),
        ),
        write(
            "/api/dsp/cjf87gqd-hr-line/push/remove",
            USE,
            |db, c, input| lines::push_unsubscribe(db, c, input, &PUSHED),
        )
        .changes(DOMAIN),
        read(
            "/api/dsp/cjf87gqd-hr-line/phones",
            MANAGE,
            |db, c, input| lines::listed(db, c, input, Some(LINE)),
        ),
        write(
            "/api/dsp/cjf87gqd-hr-line/phones/pairing",
            MANAGE,
            |db, c, input| lines::new_pairing(db, c, input, Some(LINE)),
        ),
        read(
            "/api/dsp/cjf87gqd-hr-line/phones/pairing/{id}",
            MANAGE,
            |db, c, input| lines::pairing_status(db, c, input, Some(LINE)),
        ),
        write(
            "/api/dsp/cjf87gqd-hr-line/phones/{id}/rename",
            MANAGE,
            |db, c, input| lines::rename(db, c, input, Some(LINE)),
        ),
        async_post(
            "/api/dsp/cjf87gqd-hr-line/phones/{id}/remove",
            MANAGE,
            |state, input, access| lines::remove(state, input, access, Some(LINE)),
        ),
    ]
}
