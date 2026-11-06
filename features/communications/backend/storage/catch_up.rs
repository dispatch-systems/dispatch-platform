//! Slack catch-up progress: completed cursors and where a bounded pass resumes.
use dispatch_core::{Result, db::Store};
use rusqlite::params;

/// The progress of reading Slack history across catch-up passes.
pub trait CatchUpStore {
    /// Notes that catching up read the conversation's messages up to `at`.
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()>;
    /// Notes that catching up read the conversation's messages down to the one Slack knows as
    /// `before`, of those up to `to`, and goes on from there.
    fn catching_up_communications(
        &self,
        dsp: &str,
        conversation: &str,
        to: i64,
        before: &str,
    ) -> Result<()>;
}

impl CatchUpStore for Store {
    fn caught_up_communications(&self, dsp: &str, conversation: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_conversations SET caught_up_at=max(coalesce(caught_up_at,0),?),\
             catching_up_to=NULL,catching_up_before=NULL WHERE id=?",
            params![at, conversation],
        )?;
        Ok(())
    }
    fn catching_up_communications(
        &self,
        dsp: &str,
        conversation: &str,
        to: i64,
        before: &str,
    ) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_conversations SET catching_up_to=?,catching_up_before=? \
             WHERE id=?",
            params![to, before, conversation],
        )?;
        Ok(())
    }
}
