//! The job engine, schedules, live results, the live readers, metrics, the collection browser
//! and the registry of collectors.
#[path = "../api/mod.rs"]
pub mod api;
pub mod browser;
pub mod connections;
pub mod diagnostics;
pub mod jobs;
pub mod live;
pub mod metrics;
pub mod readers;
pub mod registry;
pub mod rules;
pub mod schedules;
pub mod settings;
pub mod verification;

/// Hands the pages a collection or a live read freed back to the host. Each parses tens of
/// megabytes on blocking threads, whose allocator arenas keep them resident otherwise: a day
/// of live reads held over a gigabyte the server had finished with.
pub(crate) async fn release_memory() {
    let _ = tokio::task::spawn_blocking(|| {
        #[cfg(target_env = "gnu")]
        unsafe {
            libc::malloc_trim(0);
        }
    })
    .await;
}
