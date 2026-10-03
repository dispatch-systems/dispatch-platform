//! The error codes Cortex's collections raise and branch on. Their text is the wire format.
use crate::Code;

pub const CORTEX_SOURCE_CHANGED: Code = Code::new("cortex_source_changed");
pub const ROUTES_MOVE_UNANSWERED: Code = Code::new("routes_move_unanswered");
pub const CORTEX_CONTENT_INCOMPLETE: Code = Code::new("cortex_content_incomplete");
pub const CORTEX_SCOPE_MISMATCH: Code = Code::new("cortex_scope_mismatch");
pub const CORTEX_TIMEZONE_MISMATCH: Code = Code::new("cortex_timezone_mismatch");
pub const CORTEX_SOURCE_TOO_LARGE: Code = Code::new("cortex_source_too_large");
pub const CORTEX_STATION_UNAVAILABLE: Code = Code::new("cortex_station_unavailable");
pub const CORTEX_PROVIDER_AMBIGUOUS: Code = Code::new("cortex_provider_ambiguous");
pub const CORTEX_INVALID_MEAL_EVIDENCE: Code = Code::new("cortex_invalid_meal_evidence");
pub const CORTEX_INVALID_IDENTITY: Code = Code::new("cortex_invalid_identity");
pub const INVALID_CORTEX_SCOPE: Code = Code::new("invalid_cortex_scope");
pub const DVIC_SOURCE_CHANGED: Code = Code::new("dvic_source_changed");
pub const SCORECARD_API_UNREADABLE: Code = Code::new("scorecard_api_unreadable");

/// Every code above.
pub const ALL: &[Code] = &[
    CORTEX_SOURCE_CHANGED,
    ROUTES_MOVE_UNANSWERED,
    CORTEX_CONTENT_INCOMPLETE,
    CORTEX_SCOPE_MISMATCH,
    CORTEX_TIMEZONE_MISMATCH,
    CORTEX_SOURCE_TOO_LARGE,
    CORTEX_STATION_UNAVAILABLE,
    CORTEX_PROVIDER_AMBIGUOUS,
    CORTEX_INVALID_MEAL_EVIDENCE,
    CORTEX_INVALID_IDENTITY,
    INVALID_CORTEX_SCOPE,
    DVIC_SOURCE_CHANGED,
    SCORECARD_API_UNREADABLE,
];
/// A failed attempt with one of these is queued again while attempts remain.
pub const RETRYABLE: &[Code] = &[
    CORTEX_SOURCE_CHANGED,
    DVIC_SOURCE_CHANGED,
    CORTEX_CONTENT_INCOMPLETE,
    SCORECARD_API_UNREADABLE,
];
