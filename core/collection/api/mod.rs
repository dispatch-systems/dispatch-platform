//! Collection's API: jobs, schedules and connections, and the types they answer with. Features
//! register their collection controls and schedules with the builders in `routes`.
pub mod jobs;
pub mod metrics;
pub mod types;
pub mod routes {
    pub mod collectors;
    pub mod connections;
    pub mod jobs;
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
        types::CollectionChange,
        types::CollectionUpdates,
        types::Collectors,
        types::Connection,
        types::ConnectionStatus,
        types::LastRun,
        types::RuleKind,
        types::RunCounts,
        types::ScheduleChoice,
        types::ScheduleCollection,
        types::ScheduleOption,
        types::ScheduleRule,
        types::ScheduleRuns,
        types::ScheduledCollection,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    let mut all = routes::jobs::routes();
    all.extend(routes::connections::routes());
    all.extend(routes::collectors::routes());
    all
}
