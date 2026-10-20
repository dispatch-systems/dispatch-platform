//! Vans' endpoints, each registered with its path and access. Its fleet bar, on its Settings
//! page, shows when the fleet was collected and collects it on demand.
use crate::VansStore;
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
use dispatch_cortex::vans::JOB_KIND;

const COLLECT: Dsp = Dsp("vans.collect");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/vans/status", COLLECT, status),
        // Queuing a collection writes only its job and its log entry, which no cached read
        // uses: the caches stay as they are.
        write("/api/dsp/vans/collect", COLLECT, collect).bookkeeping(),
        job_cancel("/api/dsp/vans/jobs/{id}/cancel", COLLECT, &[JOB_KIND]),
    ];
    routes.extend(schedule_routes("/api/dsp/vans/schedules", COLLECT, "vans"));
    routes
}

fn status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&db.fleet_status(c.dsp_id())?)
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["requestId"])?;
    let key = v::text(b, "requestId", 1, 128)?;
    let job = db.enqueue_fleet(c.dsp_id(), Some(c.actor()), key)?;
    db.audit(
        Some(c.actor()),
        Some(c.dsp_id()),
        "vans.collection_requested",
        "",
    )?;
    Ok(Reply::status(job, 202))
}
