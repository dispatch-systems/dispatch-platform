//! Both ends of Slack's HTTP API: its routes, Import Files' in `imports` and Sync Canvases' in
//! `canvases`, the types they answer with, and in `client.ts` the frontend's function for each.
pub(crate) mod canvases;
pub(crate) mod imports;
pub(crate) mod routes;
pub(crate) mod types;
