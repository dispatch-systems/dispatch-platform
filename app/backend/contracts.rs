//! Typed boundaries: what routes answer with, what they accept, and the closed sets of
//! text the database stores. The wire format and the stored schema do not change when
//! an endpoint moves here. Each type is defined by its owner, in its `api/`; this module
//! gathers them under one path.
#[path = "../../features/timecard/api/assessment.rs"]
mod assessment;
#[path = "../../features/driver_match/api/types.rs"]
mod driver_match;
#[path = "../../features/dvic/api/types.rs"]
mod dvic;
#[cfg(test)]
#[path = "export.rs"]
mod generated;
#[path = "../../features/timecard/api/meals.rs"]
mod meals;
#[path = "../../features/routes/api/types.rs"]
mod routedata;
#[path = "../../features/scorecard/api/types.rs"]
mod scorecard;
#[path = "../../features/timecard/api/settings.rs"]
mod settings;
#[path = "../../features/uniforms/api/types.rs"]
mod uniforms;
#[path = "../../features/timecard/api/types.rs"]
mod workforce;

pub use assessment::*;
pub use dispatch_core::accounts::api::{requests::*, types::*};
pub use dispatch_core::collection::api::{jobs::*, metrics::*, types::*};
pub use dispatch_core::foundation::config::{Environment, ProviderMode};
pub use dispatch_core::foundation::wire::request;
pub use dispatch_core::mcp::api::types::*;
pub use dispatch_core::platform_owner::api::types::*;
pub use dispatch_core::server::api::types::*;
pub use dispatch_core::tenancy::api::{audit::*, types::*};
pub use dispatch_paycom::timecards::EmployeeTimecardPeriod;
pub use driver_match::*;
pub use dvic::*;
pub use meals::*;
pub use routedata::*;
pub use scorecard::*;
pub use settings::*;
pub use uniforms::*;
pub use workforce::*;

#[cfg(test)]
#[path = "../tests/backend/contracts.rs"]
mod tests;
