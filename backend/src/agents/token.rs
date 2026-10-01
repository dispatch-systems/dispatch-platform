//! The key an agent signs in with: `dsk_live_` on production and `dsk_dev_` elsewhere, then
//! 32 random letters and digits and a 6-character checksum. The prefix says what it is
//! and where it works; the checksum turns away a mistyped or made-up key before any lookup.
use crate::{Result, contracts::Environment, crypto};

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const RANDOM: usize = 32;
const CHECK: usize = 6;

pub fn prefix(environment: Environment) -> &'static str {
    match environment {
        Environment::Production => "dsk_live_",
        Environment::Preview => "dsk_dev_",
    }
}

/// A new key for this environment.
pub fn new(environment: Environment) -> Result<String> {
    let mut body = String::with_capacity(RANDOM);
    // Bytes from 248 up are skipped, so every character is equally likely.
    while body.len() < RANDOM {
        for byte in crypto::random::<64>()? {
            if body.len() < RANDOM && byte < 248 {
                body.push(ALPHABET[usize::from(byte % 62)] as char);
            }
        }
    }
    let head = format!("{}{body}", prefix(environment));
    let check = checksum(&head);
    Ok(head + &check)
}

/// Whether `token` is shaped like this environment's key, with a checksum that matches.
pub fn well_formed(token: &str, environment: Environment) -> bool {
    let Some(rest) = token.strip_prefix(prefix(environment)) else {
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
mod tests {
    use super::*;

    #[test]
    fn keys_carry_their_environment_and_a_checksum() {
        let key = new(Environment::Production).unwrap();
        assert!(key.starts_with("dsk_live_") && key.len() == 9 + RANDOM + CHECK);
        assert!(well_formed(&key, Environment::Production));
        // A key from one environment never passes for the other's.
        assert!(!well_formed(&key, Environment::Preview));
        let dev = new(Environment::Preview).unwrap();
        assert!(dev.starts_with("dsk_dev_") && well_formed(&dev, Environment::Preview));
        // One changed character fails the checksum.
        let mut typo = key.clone().into_bytes();
        typo[12] = if typo[12] == b'a' { b'b' } else { b'a' };
        assert!(!well_formed(
            &String::from_utf8(typo).unwrap(),
            Environment::Production
        ));
        assert!(!well_formed("dsk_live_short", Environment::Production));
        assert_ne!(key, new(Environment::Production).unwrap());
    }
    #[test]
    fn crc32_is_the_standard_one() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
