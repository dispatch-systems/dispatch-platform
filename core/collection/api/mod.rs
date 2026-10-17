//! Collection's API: jobs, schedules and connections, and the types they answer with. Features
//! register their collection controls and schedules with the builders in `routes`.
pub mod jobs;
pub mod metrics;
pub mod types;
pub mod routes {
    pub mod connections;
    pub mod jobs;
    pub mod schedules;
}

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        jobs::JobStatus,
        jobs::PublicJob,
        metrics::DocumentState,
        metrics::JobMetrics,
        metrics::JobOutcome,
        metrics::JobPhase,
        metrics::PageRead,
        metrics::PageReads,
        metrics::PageStage,
        types::Cadence,
        types::CollectionChange,
        types::CollectionSchedule,
        types::CollectionSchedules,
        types::CollectionUpdates,
        types::Connection,
        types::ConnectionStatus,
        types::ScheduleCollection,
        types::SchedulePreview,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    let mut all = routes::jobs::routes();
    all.extend(routes::connections::routes());
    all
}
