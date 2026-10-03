//! Typed boundaries: what routes answer with, what they accept, and the closed sets of
//! text the database stores. The wire format and the stored schema do not change when
//! an endpoint moves here. Each type is defined by its owner, in its `api/`; this module
//! gathers them under one path.
#[path = "../../core/accounts/api/types.rs"]
mod accounts;
#[path = "../../core/mcp/api/types.rs"]
mod agents;
#[path = "../../features/timecard/api/assessment.rs"]
mod assessment;
#[path = "../../core/tenancy/api/audit.rs"]
mod audit;
#[path = "../../core/collection/api/types.rs"]
mod collections;
#[path = "../../features/driver_match/api/types.rs"]
mod driver_match;
#[path = "../../features/dvic/api/types.rs"]
mod dvic;
#[cfg(test)]
#[path = "export.rs"]
mod generated;
#[path = "../../core/collection/api/jobs.rs"]
mod jobs;
#[path = "../../features/timecard/api/meals.rs"]
mod meals;
#[path = "../../core/collection/api/metrics.rs"]
mod metrics;
#[path = "../../core/platform_owner/api/types.rs"]
mod platform;
#[path = "../../core/accounts/api/requests.rs"]
mod requests;
#[path = "../../features/routes/api/types.rs"]
mod routedata;
#[path = "../../features/scorecard/api/types.rs"]
mod scorecard;
#[path = "../../core/server/api/types.rs"]
mod server;
#[path = "../../features/timecard/api/settings.rs"]
mod settings;
#[path = "../../core/tenancy/api/types.rs"]
mod tenancy;
#[path = "../../features/uniforms/api/types.rs"]
mod uniforms;
#[path = "../../features/timecard/api/types.rs"]
mod workforce;

pub use crate::collectors::paycom::timecards::EmployeeTimecardPeriod;
pub use crate::config::{Environment, ProviderMode};
pub use crate::wire::request;
pub use accounts::*;
pub use agents::*;
pub use assessment::*;
pub use audit::*;
pub use collections::*;
pub use driver_match::*;
pub use dvic::*;
pub use jobs::*;
pub use meals::*;
pub use metrics::*;
pub use platform::*;
pub use requests::*;
pub use routedata::*;
pub use scorecard::*;
pub use server::*;
pub use settings::*;
pub use tenancy::*;
pub use uniforms::*;
pub use workforce::*;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn missing_or_mistyped_fields_do_not_become_empty_values() {
        for input in [
            json!({"email":"a@example.com"}),
            json!({"email":"a@example.com","password":123}),
            json!({"email":"a@example.com","password":"","extra":true}),
        ] {
            assert!(LoginRequest::parse(&input).is_err());
        }
        assert!(CollectionRequest::parse(&json!({"requestId":"test","date":null}), false).is_err());
        assert!(CollectionRequest::parse(&json!({"requestId":"test"}), true).is_err());
        assert!(
            CollectionRequest::parse(&json!({"requestId":"test","date":"2026-02-30"}), false)
                .is_err()
        );
        assert!(JobStatus::parse("finished").is_none());
        let list = |statuses: &[JobStatus]| {
            let quoted: Vec<_> = statuses
                .iter()
                .map(|s| format!("'{}'", s.as_str()))
                .collect();
            format!("({})", quoted.join(","))
        };
        assert_eq!(crate::job_statuses!(active), list(JobStatus::ACTIVE));
        assert_eq!(crate::job_statuses!(leased), list(JobStatus::LEASED));
        assert!(serde_json::from_value::<JobStatus>(json!("finished")).is_err());
    }
}
