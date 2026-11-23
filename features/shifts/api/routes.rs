//! Schedules' endpoints, each registered with its path and access. The week is collected, on
//! demand or by its rules, from Settings' Collectors page.
use dispatch_core::{
    collection::api::routes::jobs::job_cancel,
    server::http::route::{Dsp, Route},
};
use dispatch_cortex::shifts::JOB_KIND;

const COLLECT: Dsp = Dsp("shifts.collect");

pub fn routes() -> Vec<Route> {
    vec![job_cancel(
        "/api/dsp/shifts/jobs/{id}/cancel",
        COLLECT,
        &[JOB_KIND],
    )]
}
