//! AES-256-GCM file encryption — port of `Crypto.swift`.
//! Wire format: `[12-byte nonce][AES-256-GCM ciphertext || 16-byte tag]`.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use std::fs;
use std::io::Write;
use std::path::Path;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Atomically write `data` to `dst` (temp file in same dir + rename), mode 0600.
pub fn write_atomic_0600(dst: &Path, data: &[u8]) -> std::io::Result<()> {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let suffix: u64 = rand::random();
    let tmp = dst.with_file_name(format!(".{name}.{suffix:016x}.tmp"));
    let res = (|| {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        set_mode_0600(&tmp)?;
        fs::rename(&tmp, dst)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

#[cfg(unix)]
fn set_mode_0600(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o600))
}
#[cfg(not(unix))]
fn set_mode_0600(_p: &Path) -> std::io::Result<()> {
    Ok(())
}

fn cipher(key: &[u8]) -> Result<Aes256Gcm, CryptoError> {
    Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::DecryptionFailed)
}

/// Encrypt file at `src` into `dst`.
pub fn encrypt_file(src: &str, dst: &str, key: &[u8]) -> Result<(), CryptoError> {
    let plain = fs::read(src)?;
    let c = cipher(key)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::fill(&mut nonce_bytes[..]);
    let nonce = Nonce::from(nonce_bytes);
    let ct = c
        .encrypt(&nonce, plain.as_slice())
        .map_err(|_| CryptoError::DecryptionFailed)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    write_atomic_0600(Path::new(dst), &out)?;
    Ok(())
}

/// Decrypt `.enc` file at `src` into `dst`.
pub fn decrypt_file(src: &str, dst: &str, key: &[u8]) -> Result<(), CryptoError> {
    let blob = fs::read(src)?;
    if blob.len() <= NONCE_LEN + TAG_LEN {
        return Err(CryptoError::DecryptionFailed);
    }
    let c = cipher(key)?;
    let nonce_arr: [u8; NONCE_LEN] = blob[..NONCE_LEN].try_into().expect("12 bytes");
    let nonce = Nonce::from(nonce_arr);
    let plain = c
        .decrypt(&nonce, &blob[NONCE_LEN..])
        .map_err(|_| CryptoError::DecryptionFailed)?;
    write_atomic_0600(Path::new(dst), &plain)?;
    Ok(())
}

/// Overwrite a buffer with zeros (not optimized away) and clear it.
pub fn zero_bytes(data: &mut Vec<u8>) {
    for b in data.iter_mut() {
        // SAFETY: valid &mut u8; volatile prevents the write being elided.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    data.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Vec<u8> {
        (0..32u8).collect()
    }

    #[test]
    fn roundtrip_and_format() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("plain.txt");
        fs::write(&src, "secret-config-contents").unwrap();
        let enc = d.path().join("plain.txt.enc");
        let out = d.path().join("plain.out");
        encrypt_file(src.to_str().unwrap(), enc.to_str().unwrap(), &key()).unwrap();
        let blob = fs::read(&enc).unwrap();
        assert_eq!(blob.len(), 12 + "secret-config-contents".len() + 16);
        decrypt_file(enc.to_str().unwrap(), out.to_str().unwrap(), &key()).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "secret-config-contents");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&enc).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&out).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn nonce_is_random() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("p");
        fs::write(&src, "x").unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        encrypt_file(src.to_str().unwrap(), a.to_str().unwrap(), &key()).unwrap();
        encrypt_file(src.to_str().unwrap(), b.to_str().unwrap(), &key()).unwrap();
        assert_ne!(fs::read(a).unwrap()[..12], fs::read(b).unwrap()[..12]);
    }

    #[test]
    fn tamper_and_wrong_key_fail() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("p");
        fs::write(&src, "hello world").unwrap();
        let enc = d.path().join("p.enc");
        let out = d.path().join("p.out");
        encrypt_file(src.to_str().unwrap(), enc.to_str().unwrap(), &key()).unwrap();

        let wrong: Vec<u8> = vec![7; 32];
        assert!(matches!(
            decrypt_file(enc.to_str().unwrap(), out.to_str().unwrap(), &wrong),
            Err(CryptoError::DecryptionFailed)
        ));

        let mut blob = fs::read(&enc).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 1;
        fs::write(&enc, &blob).unwrap();
        assert!(matches!(
            decrypt_file(enc.to_str().unwrap(), out.to_str().unwrap(), &key()),
            Err(CryptoError::DecryptionFailed)
        ));
        assert!(!out.exists());
    }

    #[test]
    fn short_blob_rejected() {
        let d = tempfile::tempdir().unwrap();
        let enc = d.path().join("s.enc");
        let out = d.path().join("s.out");
        for n in [0usize, 11, 12, 27, 28] {
            fs::write(&enc, vec![0u8; n]).unwrap();
            assert!(
                matches!(
                    decrypt_file(enc.to_str().unwrap(), out.to_str().unwrap(), &key()),
                    Err(CryptoError::DecryptionFailed)
                ),
                "len {n}"
            );
        }
    }

    #[test]
    fn bad_key_size_and_missing_src() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("p");
        fs::write(&src, "x").unwrap();
        let dst = d.path().join("q");
        assert!(encrypt_file(src.to_str().unwrap(), dst.to_str().unwrap(), &[1, 2, 3]).is_err());
        assert!(matches!(
            encrypt_file("/nonexistent/zzz", dst.to_str().unwrap(), &key()),
            Err(CryptoError::Io(_))
        ));
    }

    #[test]
    fn zero_bytes_clears() {
        let mut v = vec![1u8, 2, 3];
        zero_bytes(&mut v);
        assert!(v.is_empty());
    }
}
