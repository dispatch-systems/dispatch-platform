//! The agent API under `/api/v1/` with the Agents page's keys, OAuth's protocol endpoints, and
//! their types.
pub mod types;
pub(crate) mod routes {
    pub mod agent_api;
    pub mod oauth;
}
