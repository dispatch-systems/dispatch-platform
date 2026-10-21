//! Driver Profiles' endpoints, each registered with its path and access. The roster is
//! collected, on demand or by its rules, from Settings' Collectors page.
use dispatch_core::{
    collection::api::routes::jobs::job_cancel,
    server::http::route::{Dsp, Route},
};
use dispatch_cortex::associates::JOB_KIND;

const COLLECT: Dsp = Dsp("driver_profiles.collect");

pub fn routes() -> Vec<Route> {
    vec![job_cancel(
        "/api/dsp/driver-profiles/jobs/{id}/cancel",
        COLLECT,
        &[JOB_KIND],
    )]
}
