use std::sync::Arc;

use resonance_core::providers::{ProviderKey, normalise_key};
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore, OsCredentialStore};

use crate::Api;
use crate::command::{ProviderKeyKind, ProviderKeyStatus, ProviderKeyValue};
use crate::error::ApiError;

/// End-to-end runs set `RESONANCE_MEMORY_CREDENTIALS=1` so a test key never reaches the
/// user's real credential store.
pub(crate) fn default_credential_store() -> Arc<dyn CredentialStore> {
    if std::env::var("RESONANCE_MEMORY_CREDENTIALS").as_deref() == Ok("1") {
        Arc::new(MemoryCredentialStore::new())
    } else {
        Arc::new(OsCredentialStore::new())
    }
}

fn to_core(kind: ProviderKeyKind) -> ProviderKey {
    match kind {
        ProviderKeyKind::AcoustId => ProviderKey::AcoustId,
        ProviderKeyKind::Discogs => ProviderKey::Discogs,
        ProviderKeyKind::AudD => ProviderKey::AudD,
    }
}

fn to_command(key: ProviderKey) -> ProviderKeyKind {
    match key {
        ProviderKey::AcoustId => ProviderKeyKind::AcoustId,
        ProviderKey::Discogs => ProviderKeyKind::Discogs,
        ProviderKey::AudD => ProviderKeyKind::AudD,
    }
}

impl Api {
    fn provider_key_status(&self, key: ProviderKey) -> Result<ProviderKeyStatus, ApiError> {
        let configured = self
            .credentials
            .read(key)
            .map_err(ApiError::credentials_unavailable)?
            .is_some();
        Ok(ProviderKeyStatus {
            provider: to_command(key),
            configured,
            paid: key.is_paid(),
        })
    }

    pub(crate) fn provider_keys(&self) -> Result<Vec<ProviderKeyStatus>, ApiError> {
        ProviderKey::ALL
            .into_iter()
            .map(|key| self.provider_key_status(key))
            .collect()
    }

    pub(crate) fn set_provider_key(
        &self,
        provider: ProviderKeyKind,
        value: &ProviderKeyValue,
    ) -> Result<ProviderKeyStatus, ApiError> {
        let key = to_core(provider);
        let secret = normalise_key(&value.0).map_err(ApiError::invalid_input)?;
        self.credentials
            .write(key, secret)
            .map_err(ApiError::credentials_unavailable)?;
        self.provider_key_status(key)
    }

    pub(crate) fn clear_provider_key(
        &self,
        provider: ProviderKeyKind,
    ) -> Result<ProviderKeyStatus, ApiError> {
        let key = to_core(provider);
        self.credentials
            .clear(key)
            .map_err(ApiError::credentials_unavailable)?;
        self.provider_key_status(key)
    }
}
