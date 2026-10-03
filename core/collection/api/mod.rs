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
