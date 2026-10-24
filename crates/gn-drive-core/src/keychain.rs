//! Secret store abstraction + macOS Keychain backend — port of `KeychainStore.swift`.
//!
//! Values are stored RAW (not base64) as generic-password data, identical to the
//! Swift implementation, so items saved by either side read back.

use parking_lot::Mutex;
use std::collections::HashMap;

/// Service name used by the real app.
pub const DEFAULT_SERVICE: &str = "gn-drive";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecureStoreError {
    #[error("secure store: item not found")]
    NotFound,
    /// OS-level failure (macOS `OSStatus`).
    #[error("secure store: OSStatus {0}")]
    Os(i32),
    #[error("secure store: {0}")]
    Other(String),
}

pub trait SecureStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Vec<u8>, SecureStoreError>;
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecureStoreError>;
    /// Deleting a missing item is not an error.
    fn delete(&self, account: &str) -> Result<(), SecureStoreError>;
}

/// Login-keychain generic-password store (service + account).
#[derive(Debug, Clone)]
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    pub fn service(&self) -> &str {
        &self.service
    }
}

#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn map_err(e: security_framework::base::Error) -> SecureStoreError {
    if e.code() == ERR_SEC_ITEM_NOT_FOUND {
        SecureStoreError::NotFound
    } else {
        SecureStoreError::Os(e.code())
    }
}

#[cfg(target_os = "macos")]
impl SecureStore for KeychainStore {
    fn get(&self, account: &str) -> Result<Vec<u8>, SecureStoreError> {
        security_framework::passwords::get_generic_password(&self.service, account).map_err(map_err)
    }

    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecureStoreError> {
        // Updates the existing item or adds a new one.
        security_framework::passwords::set_generic_password(&self.service, account, value)
            .map_err(map_err)
    }

    fn delete(&self, account: &str) -> Result<(), SecureStoreError> {
        match security_framework::passwords::delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl SecureStore for KeychainStore {
    fn get(&self, _account: &str) -> Result<Vec<u8>, SecureStoreError> {
        Err(SecureStoreError::Other(
            "keychain is only available on macOS".into(),
        ))
    }
    fn set(&self, _account: &str, _value: &[u8]) -> Result<(), SecureStoreError> {
        Err(SecureStoreError::Other(
            "keychain is only available on macOS".into(),
        ))
    }
    fn delete(&self, _account: &str) -> Result<(), SecureStoreError> {
        Err(SecureStoreError::Other(
            "keychain is only available on macOS".into(),
        ))
    }
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct MemorySecureStore {
    map: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemorySecureStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecureStore for MemorySecureStore {
    fn get(&self, account: &str) -> Result<Vec<u8>, SecureStoreError> {
        self.map
            .lock()
            .get(account)
            .cloned()
            .ok_or(SecureStoreError::NotFound)
    }
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecureStoreError> {
        self.map.lock().insert(account.to_string(), value.to_vec());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), SecureStoreError> {
        self.map.lock().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_semantics() {
        let s = MemorySecureStore::new();
        assert_eq!(s.get("a"), Err(SecureStoreError::NotFound));
        s.set("a", b"one").unwrap();
        assert_eq!(s.get("a").unwrap(), b"one");
        s.set("a", b"two").unwrap();
        assert_eq!(s.get("a").unwrap(), b"two");
        s.delete("a").unwrap();
        s.delete("a").unwrap();
        assert_eq!(s.get("a"), Err(SecureStoreError::NotFound));
    }

    #[test]
    fn default_service_constant() {
        assert_eq!(DEFAULT_SERVICE, "gn-drive");
    }

    /// Opt-in: touches the real login keychain under a unique throwaway service.
    #[test]
    #[ignore = "set GN_DRIVE_TEST_KEYCHAIN=1 and run with --ignored"]
    #[cfg(target_os = "macos")]
    fn real_keychain_roundtrip() {
        if std::env::var("GN_DRIVE_TEST_KEYCHAIN").as_deref() != Ok("1") {
            return;
        }
        let service = format!("gn-drive-test-{}", uuid::Uuid::new_v4());
        let s = KeychainStore::new(&service);
        let acct = "acct";
        let result = std::panic::catch_unwind(|| {
            assert_eq!(s.get(acct), Err(SecureStoreError::NotFound));
            let raw: Vec<u8> = (0..32u8).collect();
            s.set(acct, &raw).unwrap();
            assert_eq!(s.get(acct).unwrap(), raw);
            s.set(acct, b"v2").unwrap();
            assert_eq!(s.get(acct).unwrap(), b"v2");
        });
        let _ = s.delete(acct);
        assert_eq!(s.get(acct), Err(SecureStoreError::NotFound));
        s.delete(acct).unwrap();
        result.unwrap();
    }
}
