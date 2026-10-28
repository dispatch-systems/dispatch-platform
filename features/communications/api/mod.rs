//! Both ends of Communications's HTTP API: its routes, the types they answer with, and in
//! `client.ts` the frontend's function for each; and apart from them, the address the DSP's
//! dispatch phones talk to (`phone`), which no member's session reaches.
pub(crate) mod display;
mod phone;
pub(crate) mod routes;
pub(crate) mod types;
