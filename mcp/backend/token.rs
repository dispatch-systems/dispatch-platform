//! The key an agent signs in with: `dsk_live_` on production and `dsk_dev_` elsewhere, then
//! 32 random letters and digits and a 6-character checksum. The prefix says what it is
//! and where it works; the checksum turns away a mistyped or made-up key before any lookup.
//! A connected app's access and refresh tokens are made the same way, as `dsa_` and `dsr_`.
use dispatch_core::{
    Result,
    foundation::{config::Environment, crypto},
};

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const RANDOM: usize = 32;
const CHECK: usize = 6;

/// What a token is: an agent key, or a connected app's access or refresh token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Key,
    Access,
    Refresh,
}

pub fn prefix(kind: Kind, environment: Environment) -> &'static str {
    match (kind, environment) {
        (Kind::Key, Environment::Production) => "dsk_live_",
        (Kind::Key, Environment::Preview) => "dsk_dev_",
        (Kind::Access, Environment::Production) => "dsa_live_",
        (Kind::Access, Environment::Preview) => "dsa_dev_",
        (Kind::Refresh, Environment::Production) => "dsr_live_",
        (Kind::Refresh, Environment::Preview) => "dsr_dev_",
    }
}

/// A new token of this kind for this environment.
pub fn new(kind: Kind, environment: Environment) -> Result<String> {
    let mut body = String::with_capacity(RANDOM);
    // Bytes from 248 up are skipped, so every character is equally likely.
    while body.len() < RANDOM {
        for byte in crypto::random::<64>()? {
            if body.len() < RANDOM && byte < 248 {
                body.push(ALPHABET[usize::from(byte % 62)] as char);
            }
        }
    }
    let head = format!("{}{body}", prefix(kind, environment));
    let check = checksum(&head);
    Ok(head + &check)
}

/// Whether `token` is shaped like this kind of token for this environment, with a checksum
/// that matches.
pub fn well_formed(token: &str, kind: Kind, environment: Environment) -> bool {
    let Some(rest) = token.strip_prefix(prefix(kind, environment)) else {
        return false;
    };
    rest.len() == RANDOM + CHECK
        && rest.bytes().all(|b| b.is_ascii_alphanumeric())
        && checksum(&token[..token.len() - CHECK]) == token[token.len() - CHECK..]
}

fn checksum(head: &str) -> String {
    let mut value = crc32(head.as_bytes());
    let mut out = [b'0'; CHECK];
    for slot in out.iter_mut().rev() {
        *slot = ALPHABET[(value % 62) as usize];
        value /= 62;
    }
    String::from_utf8(out.to_vec()).unwrap_or_default()
}

/// CRC-32 (IEEE), the checksum GitHub's tokens carry too.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
#[path = "../tests/backend/token.rs"]
mod tests;
