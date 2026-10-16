//! Driver Profiles' endpoints, each registered with its path and access. Its roster bar, on
//! Driver Match's tab, shows when the roster was collected and collects it on demand.
use crate::DriverProfilesStore;
use dispatch_core::{
    Result,
    collection::api::routes::{jobs::job_cancel, schedules::schedule_routes},
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Member, Route, read, write},
    },
};
use dispatch_cortex::associates::JOB_KIND;

const COLLECT: Dsp = Dsp("driver_profiles.collect");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/driver-profiles/status", COLLECT, status),
        write("/api/dsp/driver-profiles/collect", COLLECT, collect),
        job_cancel(
            "/api/dsp/driver-profiles/jobs/{id}/cancel",
            COLLECT,
            &[JOB_KIND],
        ),
    ];
    routes.extend(schedule_routes(
        "/api/dsp/driver-profiles/schedules",
        COLLECT,
        "associates",
    ));
    routes
}

fn status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.roster_status(c.dsp_id())?)
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let job = db.enqueue_roster(c.dsp_id(), Some(c.actor()), key)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "driver_profiles.collection_requested",
        "",
    )?;
    Ok(Reply::status(job, 202))
}
