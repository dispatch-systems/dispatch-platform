//! HR Line's endpoints, each registered with its path and access: managing its phones, as its card
//! on Settings' DSP Connections does, behind `cjf87gqd_hr_line.phones` (Manage HR Line Phones) or
//! core's `connections.manage` (Manage DSP Connections). Each is Dispatch Phone's for a line of
//! its phones (`lines`), for HR Line's alone: it never lists or changes Dispatch Phone's own, nor
//! does Dispatch Phone's card list or change HR Line's.
use crate::LINE;
use dispatch_communications::lines;
use dispatch_core::server::http::route::{Dsp, Route, async_post, read, write};

/// Managing HR Line's phones, as managing all the DSP's connections includes.
const MANAGE: Dsp = Dsp("connections.manage|cjf87gqd_hr_line.phones");

pub fn routes() -> Vec<Route> {
    vec![
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
