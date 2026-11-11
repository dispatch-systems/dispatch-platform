//! A rolling minute of events, for counting what a key or an address did in the last one.
use std::collections::VecDeque;

/// A minute, in milliseconds.
pub const WINDOW: i64 = 60_000;

/// When each event in the last minute happened, oldest first.
#[derive(Default)]
pub struct Minute(VecDeque<i64>);
impl Minute {
    /// The events in the minute before `at`, forgetting those older.
    pub fn count(&mut self, at: i64) -> usize {
        while self.0.front().is_some_and(|seen| *seen <= at - WINDOW) {
            self.0.pop_front();
        }
        self.0.len()
    }
    /// Counts an event at `at`, forgetting those over a minute before it.
    pub fn record(&mut self, at: i64) {
        self.count(at);
        self.0.push_back(at);
    }
    /// Whether anything happened in the minute before `at`.
    pub fn active(&self, at: i64) -> bool {
        self.0.back().is_some_and(|seen| *seen > at - WINDOW)
    }
}
