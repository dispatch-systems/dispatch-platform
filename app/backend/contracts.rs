//! Typed boundaries: what routes answer with, what they accept, and the closed sets of
//! text the database stores. The wire format and the stored schema do not change when
//! an endpoint moves here. Each type is defined by its owner, in its `api/`; this module
//! gathers them under one path.
#[cfg(test)]
#[path = "export.rs"]
mod generated;

pub use dispatch_core::accounts::api::{requests::*, types::*};
pub use dispatch_core::collection::api::{jobs::*, metrics::*, types::*};
pub use dispatch_core::foundation::config::{Environment, ProviderMode};
pub use dispatch_core::foundation::wire::request;
pub use dispatch_core::mcp::api::types::*;
pub use dispatch_core::platform_owner::api::types::*;
pub use dispatch_core::server::api::types::*;
pub use dispatch_core::tenancy::api::{audit::*, types::*};
pub use dispatch_driver_match::{
    Driver, DriverActivity, DriverCounts, DriverDay, DriverDetails, DriverEvent, DriverEventKind,
    DriverEvidence, DriverEvidenceKind, DriverId, DriverLink, DriverMatch, DriverPair,
    DriverStrength,
};
pub use dispatch_dvic::{DvicInspection, DvicInspections, DvicReport, DvicStatus, DvicWeek};
pub use dispatch_paycom::timecards::EmployeeTimecardPeriod;
pub use dispatch_routes::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteRetention, RouteStop,
    RouteTask, RouteUnknownStop,
};
pub use dispatch_scorecard::{
    ScorecardDatasetCount, ScorecardPublication, ScorecardWeek, ScorecardWeeks,
};
pub use dispatch_timecard::{
    AssessedClock, CortexMeal, CortexPublication, DailyTimecard, DailyTimecards, DeliveryGap,
    DeliveryGaps, DepartmentOption, Employee, EmployeeTimecard, EmployeeTimecardResponse,
    EmployeesResponse, InPunchKind, LateRule, Lunch, MatchType, MealAssessment, MealComparison,
    MealDriver, MealEmployee, MealPair, MealPaycom, MealSource, MealStatus, NameOrder,
    OutPunchKind, PaycomColumn, PaycomDay, PaycomOptions, PaycomPage, PaycomPreferences,
    PaycomSettings, PaycomSort, PreferenceRevision, Punch, PunchEvent, Timecard,
};
pub use dispatch_uniforms::{
    Uniform, UniformAdjustment, UniformEvent, UniformEventKind, UniformFit, UniformHistory,
    UniformInventory, UniformUpdates, UniformVariant,
};

#[cfg(test)]
#[path = "../tests/backend/contracts.rs"]
mod tests;
