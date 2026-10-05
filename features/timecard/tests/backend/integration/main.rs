//! Timecard's integration tests, one program for all of them, each file a module named for it:
//! one program links the crate and its collectors once, where a program per file linked them
//! five times.
mod meal_comparison;
mod meal_sync;
mod meals;
mod punches;
mod range_reads;
