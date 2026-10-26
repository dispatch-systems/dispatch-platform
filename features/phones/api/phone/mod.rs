//! The address a DSP's phones talk to, on the DSP's own address, by concern: who a call comes
//! from (`access`), pairing by a code (`pairing`), the texts a phone reports (`texts`), waiting on
//! the texts Dispatch has for it and saying how they went (`outbox`), and their pictures' bytes
//! (`files`). No browser calls these: a request carrying a cookie, or at another address than a
//! DSP's, is refused.
mod access;
mod files;
mod outbox;
mod pairing;
mod texts;

pub(super) use access::Phone;
pub(super) use files::{file_in, file_out};
pub(super) use outbox::{outbox, statuses};
pub(super) use pairing::pair;
pub(super) use texts::texts;
