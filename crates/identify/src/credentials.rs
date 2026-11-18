//! Provider keys live in the OS credential store and nowhere else. Nothing here formats a key
//! value or a store error that could carry one.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use keyring_core::{CredentialStore as PlatformStore, Entry, Error as PlatformError};
use resonance_core::providers::{CREDENTIAL_SERVICE, ProviderKey};

/// A key read back from the store. Its `Debug` output never shows the value.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderSecret(String);

impl ProviderSecret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProviderSecret(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// No credential store answers on this machine, such as Linux without a Secret Service.
    Unavailable,
    /// The store is there but refused access, usually because it is locked.
    Refused,
    /// The stored entry could not be read back as text.
    Unreadable,
    /// The store accepted the request and then failed.
    Failed,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "No credential store is available on this computer",
            Self::Refused => "The credential store refused access. Unlock it",
            Self::Unreadable => "The saved key could not be read. Clear it and enter it again",
            Self::Failed => "The credential store could not finish the request",
        })
    }
}

impl std::error::Error for CredentialError {}

/// Where provider keys are kept. The OS store in the app; memory in tests and end-to-end runs, so
/// they never touch the user's real keys and need no keyring daemon on CI.
pub trait CredentialStore: Send + Sync {
    fn read(&self, key: ProviderKey) -> Result<Option<ProviderSecret>, CredentialError>;
    fn write(&self, key: ProviderKey, secret: &str) -> Result<(), CredentialError>;
    fn clear(&self, key: ProviderKey) -> Result<(), CredentialError>;
}

/// Windows Credential Manager, the macOS login keychain or the Secret Service on Linux.
#[derive(Default)]
pub struct OsCredentialStore {
    platform: Mutex<Option<Arc<PlatformStore>>>,
}

impl OsCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    // Opened on first use and retried after a failure, so a keyring daemon that starts late is
    // still found and the app starts even when none is running.
    fn entry(&self, key: ProviderKey) -> Result<Entry, CredentialError> {
        let mut platform = self.platform.lock().unwrap_or_else(PoisonError::into_inner);
        let store = match platform.as_ref() {
            Some(store) => Arc::clone(store),
            None => {
                let store = open_platform_store()?;
                *platform = Some(Arc::clone(&store));
                store
            }
        };
        store
            .build(CREDENTIAL_SERVICE, key.account(), None)
            .map_err(classify)
    }
}

impl CredentialStore for OsCredentialStore {
    fn read(&self, key: ProviderKey) -> Result<Option<ProviderSecret>, CredentialError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(ProviderSecret(value))),
            Err(PlatformError::NoEntry) => Ok(None),
            Err(error) => Err(classify(error)),
        }
    }

    fn write(&self, key: ProviderKey, secret: &str) -> Result<(), CredentialError> {
        self.entry(key)?.set_password(secret).map_err(classify)
    }

    fn clear(&self, key: ProviderKey) -> Result<(), CredentialError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(PlatformError::NoEntry) => Ok(()),
            Err(error) => Err(classify(error)),
        }
    }
}

#[cfg(target_os = "windows")]
fn open_platform_store() -> Result<Arc<PlatformStore>, CredentialError> {
    let store: Arc<PlatformStore> = windows_native_keyring_store::Store::new().map_err(classify)?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn open_platform_store() -> Result<Arc<PlatformStore>, CredentialError> {
    let store: Arc<PlatformStore> =
        apple_native_keyring_store::keychain::Store::new().map_err(classify)?;
    Ok(store)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_platform_store() -> Result<Arc<PlatformStore>, CredentialError> {
    let store: Arc<PlatformStore> = zbus_secret_service_keyring_store::Store::new()
        .map_err(|_| CredentialError::Unavailable)?;
    Ok(store)
}

#[cfg(not(any(target_os = "windows", unix)))]
fn open_platform_store() -> Result<Arc<PlatformStore>, CredentialError> {
    Err(CredentialError::Unavailable)
}

// Platform errors are dropped rather than passed on: `BadEncoding` and `BadDataFormat` carry the
// stored bytes, which are the key itself.
fn classify(error: PlatformError) -> CredentialError {
    match error {
        PlatformError::NoStorageAccess(_) => CredentialError::Refused,
        PlatformError::BadEncoding(_) | PlatformError::BadDataFormat(..) => {
            CredentialError::Unreadable
        }
        PlatformError::NoDefaultStore => CredentialError::Unavailable,
        _ => CredentialError::Failed,
    }
}

/// Keys held for the life of the process only.
#[derive(Default)]
pub struct MemoryCredentialStore {
    keys: Mutex<HashMap<ProviderKey, String>>,
}

impl MemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn read(&self, key: ProviderKey) -> Result<Option<ProviderSecret>, CredentialError> {
        let keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(keys.get(&key).cloned().map(ProviderSecret))
    }

    fn write(&self, key: ProviderKey, secret: &str) -> Result<(), CredentialError> {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.insert(key, secret.to_owned());
        Ok(())
    }

    fn clear(&self, key: ProviderKey) -> Result<(), CredentialError> {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_saves_reads_and_clears_each_key_separately() {
        let store = MemoryCredentialStore::new();
        store.write(ProviderKey::AcoustId, "acoustid-test").unwrap();
        store.write(ProviderKey::Discogs, "discogs-test").unwrap();

        assert_eq!(
            store
                .read(ProviderKey::AcoustId)
                .unwrap()
                .as_ref()
                .map(ProviderSecret::expose),
            Some("acoustid-test")
        );
        store.clear(ProviderKey::AcoustId).unwrap();
        assert_eq!(store.read(ProviderKey::AcoustId).unwrap(), None);
        assert!(store.read(ProviderKey::Discogs).unwrap().is_some());
        store.clear(ProviderKey::AudD).unwrap();
    }

    #[test]
    fn secret_debug_hides_the_value() {
        let secret = ProviderSecret("hidden-value".to_owned());
        assert_eq!(format!("{secret:?}"), "ProviderSecret(..)");
    }

    // Uses the real OS store, so it only runs when asked for and leaves an existing key alone.
    #[test]
    #[ignore = "writes to the OS credential store"]
    fn os_store_round_trip() {
        let store = OsCredentialStore::new();
        if store.read(ProviderKey::AudD).unwrap().is_some() {
            return;
        }
        store.write(ProviderKey::AudD, "round-trip-test").unwrap();
        let read = store.read(ProviderKey::AudD).unwrap();
        store.clear(ProviderKey::AudD).unwrap();
        assert_eq!(
            read.as_ref().map(ProviderSecret::expose),
            Some("round-trip-test")
        );
        assert_eq!(store.read(ProviderKey::AudD).unwrap(), None);
        store.clear(ProviderKey::AudD).unwrap();
    }

    #[test]
    fn errors_that_carry_stored_bytes_lose_them() {
        let error = classify(PlatformError::BadEncoding(b"stored-key".to_vec()));
        assert_eq!(error, CredentialError::Unreadable);
        assert!(!error.to_string().contains("stored-key"));
    }
}
