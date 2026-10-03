//! The error codes Paycom's collection raises and branches on. Their text is the wire format.
use crate::Code;

pub const TIMECARD_EXTRACTION_FAILED: Code = Code::new("timecard_extraction_failed");
pub const INVALID_TIMECARD_HOURS: Code = Code::new("invalid_timecard_hours");

/// Every code above.
pub const ALL: &[Code] = &[TIMECARD_EXTRACTION_FAILED, INVALID_TIMECARD_HOURS];
