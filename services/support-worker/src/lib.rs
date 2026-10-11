//! Dev-host orchestration of private AI Support jobs from both platform environments.
pub mod config;
pub mod dispatch;
pub mod github;
pub mod hermes;
pub mod jobs;
pub mod process;
pub mod verification;
pub mod worktrees;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
