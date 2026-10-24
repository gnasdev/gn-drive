//! Argon2id password hashing — port of `Argon2.swift`.
//!
//! Wire format matches alexedwards/argon2id used by the Go version:
//! `$argon2id$v=19$m=65536,t=3,p=4$<salt_b64_no_pad>$<hash_b64_no_pad>`

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;

pub const MEMORY: u32 = 64 * 1024; // KiB
pub const ITERATIONS: u32 = 3;
pub const PARALLELISM: u32 = 4;
pub const KEY_LENGTH: usize = 32;
pub const SALT_LENGTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Argon2Error {
    #[error("argon2: hash failed ({0})")]
    HashFailed(String),
    #[error("argon2: invalid encoded hash format")]
    InvalidFormat,
}

fn hash_raw(password: &[u8], salt: &[u8], out: &mut [u8]) -> Result<(), Argon2Error> {
    let params = Params::new(MEMORY, ITERATIONS, PARALLELISM, Some(out.len()))
        .map_err(|e| Argon2Error::HashFailed(e.to_string()))?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, out)
        .map_err(|e| Argon2Error::HashFailed(e.to_string()))
}

/// Derive a raw 32-byte key (used for the AES-256 encryption key).
pub fn derive_key(password: &str, salt: &[u8]) -> Result<Vec<u8>, Argon2Error> {
    let mut out = vec![0u8; KEY_LENGTH];
    hash_raw(password.as_bytes(), salt, &mut out)?;
    Ok(out)
}

/// Create an encoded hash string with a fresh random 32-byte salt.
pub fn create_hash(password: &str) -> Result<String, Argon2Error> {
    let mut salt = [0u8; SALT_LENGTH];
    rand::fill(&mut salt[..]);
    let mut out = [0u8; KEY_LENGTH];
    hash_raw(password.as_bytes(), &salt, &mut out)?;
    Ok(format!(
        "$argon2id$v=19$m={MEMORY},t={ITERATIONS},p={PARALLELISM}${}${}",
        STANDARD_NO_PAD.encode(salt),
        STANDARD_NO_PAD.encode(out)
    ))
}

fn b64_raw(s: &str) -> Option<Vec<u8>> {
    // Swift pads then decodes with Foundation (padding is tolerated); accept
    // either padded or unpadded input.
    STANDARD_NO_PAD.decode(s.trim_end_matches('=')).ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Parse `m=..,t=..,p=..`.
fn parse_params(s: &str) -> Option<(u32, u32, u32)> {
    let (mut m, mut t, mut p) = (None, None, None);
    for kv in s.split(',') {
        let (k, v) = kv.split_once('=')?;
        let v: u32 = v.parse().ok()?;
        match k {
            "m" => m = Some(v),
            "t" => t = Some(v),
            "p" => p = Some(v),
            _ => return None,
        }
    }
    Some((m?, t?, p?))
}

/// Verify a password against an encoded hash. Accepts both the
/// `$argon2id$...` (6-part) and legacy `argon2id$...` (5-part) formats.
pub fn verify(password: &str, encoded: &str) -> bool {
    let parts: Vec<&str> = encoded.split('$').collect();
    if encoded.starts_with("$argon2id$") {
        // ["", "argon2id", "v=19", "m=..,t=..,p=..", salt, hash]
        if parts.len() != 6 || parts[2] != "v=19" {
            return false;
        }
        let Some((m, t, p)) = parse_params(parts[3]) else {
            return false;
        };
        let (Some(salt), Some(stored)) = (b64_raw(parts[4]), b64_raw(parts[5])) else {
            return false;
        };
        let Ok(params) = Params::new(m, t, p, Some(stored.len())) else {
            return false;
        };
        let mut out = vec![0u8; stored.len()];
        if Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), &salt, &mut out)
            .is_err()
        {
            return false;
        }
        return constant_time_eq(&stored, &out);
    }
    // Legacy 5-part format: argon2id$v=19$m=...,t=..,p=..$salt$hash (fixed params).
    if parts.len() != 5 {
        return false;
    }
    let (Some(salt), Some(stored)) = (b64_raw(parts[3]), b64_raw(parts[4])) else {
        return false;
    };
    let mut out = vec![0u8; stored.len()];
    if hash_raw(password.as_bytes(), &salt, &mut out).is_err() {
        return false;
    }
    constant_time_eq(&stored, &out)
}

/// Extract the salt from an encoded hash (either format).
pub fn extract_salt(encoded: &str) -> Result<Vec<u8>, Argon2Error> {
    let parts: Vec<&str> = encoded.split('$').collect();
    let idx = if encoded.starts_with("$argon2id$") {
        if parts.len() != 6 {
            return Err(Argon2Error::InvalidFormat);
        }
        4
    } else {
        if parts.len() != 5 {
            return Err(Argon2Error::InvalidFormat);
        }
        3
    };
    b64_raw(parts[idx]).ok_or(Argon2Error::InvalidFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with the reference C implementation (`argon2` CLI 20190702):
    //   echo -n "correct horse battery" | argon2 0123456789abcdef0123456789abcdef -id -t 3 -k 65536 -p 4 -l 32 -e
    const VECTOR_ENCODED: &str = "$argon2id$v=19$m=65536,t=3,p=4$MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY$8NzLqIEkiqq9xKPWRlUxhDrI+gvfaXSzhbPDZPwSu70";
    const VECTOR_RAW_HEX: &str = "f0dccba881248aaabdc4a3d6465531843ac8fa0bdf6974b385b3c364fc12bbbd";
    const VECTOR_PASSWORD: &str = "correct horse battery";
    const VECTOR_SALT: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn known_vector_raw_derivation() {
        let key = derive_key(VECTOR_PASSWORD, VECTOR_SALT).unwrap();
        assert_eq!(hex::encode(key), VECTOR_RAW_HEX);
    }

    #[test]
    fn known_vector_verify_six_part() {
        assert!(verify(VECTOR_PASSWORD, VECTOR_ENCODED));
        assert!(!verify("wrong", VECTOR_ENCODED));
        assert_eq!(extract_salt(VECTOR_ENCODED).unwrap(), VECTOR_SALT);
    }

    #[test]
    fn legacy_five_part_format() {
        let legacy = VECTOR_ENCODED.strip_prefix('$').unwrap();
        assert!(verify(VECTOR_PASSWORD, legacy));
        assert!(!verify("wrong", legacy));
        assert_eq!(extract_salt(legacy).unwrap(), VECTOR_SALT);
    }

    #[test]
    fn create_hash_roundtrip_and_format() {
        let hash = create_hash("hunter2").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=65536,t=3,p=4$"));
        let parts: Vec<&str> = hash.split('$').collect();
        assert_eq!(parts.len(), 6);
        // Only the `=` in `v=`, `m=`, `t=`, `p=`: no base64 padding.
        assert_eq!(hash.matches('=').count(), 4);
        assert!(verify("hunter2", &hash));
        assert!(!verify("wrong", &hash));
        let salt = extract_salt(&hash).unwrap();
        assert_eq!(salt.len(), SALT_LENGTH);
        // derive_key with the extracted salt equals the embedded hash.
        let key = derive_key("hunter2", &salt).unwrap();
        assert_eq!(STANDARD_NO_PAD.encode(key), parts[5]);
    }

    #[test]
    fn malformed_inputs() {
        assert!(!verify("x", ""));
        assert!(!verify("x", "$argon2id$v=19$m=65536,t=3,p=4$onlyfive"));
        assert!(!verify("x", "argon2id$a$b$c"));
        assert!(!verify("x", "$argon2id$v=19$m=65536,t=3,p=4$!!!$!!!"));
        assert_eq!(extract_salt("garbage"), Err(Argon2Error::InvalidFormat));
        assert_eq!(
            extract_salt("$argon2id$a$b"),
            Err(Argon2Error::InvalidFormat)
        );
        assert_eq!(
            extract_salt("$argon2id$v=19$m=1,t=1,p=1$***$x"),
            Err(Argon2Error::InvalidFormat)
        );
    }
}
