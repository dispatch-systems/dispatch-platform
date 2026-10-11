mod executor;
mod queue;
mod scheduler;
pub use queue::{CancelJobs, JobFacts};
pub use scheduler::start;
/// Trusted isolated verification uses the normal executor; it has no separate collector.
pub async fn verify(
    state: std::sync::Arc<crate::State>,
    job: crate::collection::api::jobs::JobRow,
    owner: String,
) {
    executor::execute(state, job, owner).await;
}
