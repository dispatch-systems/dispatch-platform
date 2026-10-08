//! Accounts' API: sign-in and security, and what they accept and answer with.
pub mod requests;
pub mod types;
pub(crate) mod routes {
    pub mod invitations;
    pub mod security;
    pub mod sign_in;
}
