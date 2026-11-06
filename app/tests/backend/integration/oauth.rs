//! Sign in with Dispatch, over HTTP against the real router: discovery, the authorization
//! request and its refusals, the owner's approval, the token endpoint, revocation, and the
//! connected app signing in to the agent API and MCP like a key.
mod harness;
mod lifecycle;
mod pairing;
mod protocol;
