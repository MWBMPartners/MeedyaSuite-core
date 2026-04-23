use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::error::CredentialError;

/// Source from which a credential was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    /// Environment variable.
    Environment,
    /// Configuration file / map.
    Config,
    /// OS keyring (macOS Keychain, Windows Credential Manager, etc.).
    Keyring,
    /// Local credentials file.
    LocalFile,
}

/// A resolved credential with its source.
///
/// The `Debug` impl masks the value to prevent accidental logging of secrets.
#[derive(Clone)]
pub struct ResolvedCredential {
    pub value: String,
    pub source: CredentialSource,
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("value", &"***REDACTED***")
            .field("source", &self.source)
            .finish()
    }
}

/// 4-tier credential resolution for metadata providers.
///
/// Resolution priority (highest to lowest):
/// 1. Environment variables: `MEEDYA_<PROVIDER>_<KEY>`
/// 2. Config map: in-memory key-value pairs
/// 3. OS keyring: platform-native secure storage
/// 4. Local file: `credentials.json` on disk
pub struct CredentialStore {
    config_map: HashMap<String, String>,
    credentials_file: Option<PathBuf>,
    /// Keyring service identifier (e.g., "meedya-suite"). Used when the
    /// `keyring-credentials` feature is enabled.
    #[allow(dead_code)]
    service_name: String,
}

impl CredentialStore {
    /// Create a new credential store.
    ///
    /// - `service_name`: keyring service identifier (e.g., "meedya-suite")
    /// - `credentials_file`: optional path to a local credentials.json file
    pub fn new(service_name: impl Into<String>, credentials_file: Option<PathBuf>) -> Self {
        Self {
            config_map: HashMap::new(),
            credentials_file,
            service_name: service_name.into(),
        }
    }

    /// Set a credential in the config map (tier 2).
    pub fn set_config(&mut self, provider: &str, key: &str, value: String) {
        let config_key = format!("{provider}.{key}");
        self.config_map.insert(config_key, value);
    }

    /// Resolve a credential using the 4-tier strategy.
    pub fn resolve(&self, provider: &str, key: &str) -> Result<ResolvedCredential, CredentialError> {
        let provider_upper = provider.to_uppercase().replace('-', "_");
        let key_upper = key.to_uppercase().replace('-', "_");

        // Tier 1: Environment variable
        let env_key = format!("MEEDYA_{provider_upper}_{key_upper}");
        if let Ok(value) = std::env::var(&env_key) {
            if !value.is_empty() {
                debug!("Credential {provider}/{key} resolved from environment");
                return Ok(ResolvedCredential {
                    value,
                    source: CredentialSource::Environment,
                });
            }
        }

        // Tier 2: Config map
        let config_key = format!("{provider}.{key}");
        if let Some(value) = self.config_map.get(&config_key) {
            debug!("Credential {provider}/{key} resolved from config");
            return Ok(ResolvedCredential {
                value: value.clone(),
                source: CredentialSource::Config,
            });
        }

        // Tier 3: OS keyring
        #[cfg(feature = "keyring-credentials")]
        {
            let keyring_key = format!("{provider}/{key}");
            if let Ok(entry) = keyring::Entry::new(&self.service_name, &keyring_key) {
                if let Ok(value) = entry.get_password() {
                    debug!("Credential {provider}/{key} resolved from keyring");
                    return Ok(ResolvedCredential {
                        value,
                        source: CredentialSource::Keyring,
                    });
                }
            }
        }

        // Tier 4: Local credentials file
        if let Some(ref cred_path) = self.credentials_file {
            if let Some(value) = self.read_from_file(cred_path, provider, key) {
                debug!("Credential {provider}/{key} resolved from local file");
                return Ok(ResolvedCredential {
                    value,
                    source: CredentialSource::LocalFile,
                });
            }
        }

        Err(CredentialError::NotFound {
            provider: provider.to_string(),
            key: key.to_string(),
        })
    }

    /// Store a credential in the OS keyring (tier 3).
    #[cfg(feature = "keyring-credentials")]
    pub fn store_keyring(&self, provider: &str, key: &str, value: &str) -> Result<(), CredentialError> {
        let keyring_key = format!("{provider}/{key}");
        let entry = keyring::Entry::new(&self.service_name, &keyring_key)
            .map_err(|e| CredentialError::KeyringError(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| CredentialError::KeyringError(e.to_string()))?;
        Ok(())
    }

    /// Delete a credential from the OS keyring.
    #[cfg(feature = "keyring-credentials")]
    pub fn delete_keyring(&self, provider: &str, key: &str) -> Result<(), CredentialError> {
        let keyring_key = format!("{provider}/{key}");
        let entry = keyring::Entry::new(&self.service_name, &keyring_key)
            .map_err(|e| CredentialError::KeyringError(e.to_string()))?;
        entry
            .delete_credential()
            .map_err(|e| CredentialError::KeyringError(e.to_string()))?;
        Ok(())
    }

    /// Store a credential in the local credentials file (tier 4).
    pub fn store_local_file(
        &self,
        provider: &str,
        key: &str,
        value: &str,
    ) -> Result<(), CredentialError> {
        let cred_path = self
            .credentials_file
            .as_ref()
            .ok_or_else(|| CredentialError::IoError("no credentials file configured".into()))?;

        let mut data = self.read_file_data(cred_path);
        let provider_map = data
            .entry(provider.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if let serde_json::Value::Object(map) = provider_map {
            map.insert(key.to_string(), serde_json::Value::String(value.to_string()));
        }

        // Atomic write: write to unique temp file then rename
        let parent = cred_path
            .parent()
            .ok_or_else(|| CredentialError::IoError("invalid credentials path".into()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| CredentialError::IoError(e.to_string()))?;

        let tmp_path = parent.join(format!(
            ".credentials.{}.tmp",
            std::process::id()
        ));
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| CredentialError::IoError(e.to_string()))?;
        std::fs::write(&tmp_path, &json)
            .map_err(|e| CredentialError::IoError(e.to_string()))?;

        // Set restrictive permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&tmp_path, perms);
        }

        std::fs::rename(&tmp_path, cred_path)
            .map_err(|e| CredentialError::IoError(e.to_string()))?;

        Ok(())
    }

    fn read_from_file(&self, path: &Path, provider: &str, key: &str) -> Option<String> {
        let data = self.read_file_data(path);
        data.get(provider)?
            .get(key)?
            .as_str()
            .map(|s| s.to_string())
    }

    fn read_file_data(&self, path: &Path) -> HashMap<String, serde_json::Value> {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("Corrupted credentials file at {}: {e}", path.display());
                    HashMap::new()
                }
            },
            Err(_) => HashMap::new(), // File doesn't exist yet
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_from_config_map() {
        let mut store = CredentialStore::new("test", None);
        store.set_config("spotify", "client_id", "test-id".to_string());

        let resolved = store.resolve("spotify", "client_id").unwrap();
        assert_eq!(resolved.value, "test-id");
        assert_eq!(resolved.source, CredentialSource::Config);
    }

    #[test]
    fn resolve_not_found() {
        let store = CredentialStore::new("test", None);
        let result = store.resolve("nonexistent", "key");
        assert!(matches!(result, Err(CredentialError::NotFound { .. })));
    }

    #[test]
    fn resolve_env_takes_priority() {
        let mut store = CredentialStore::new("test", None);
        store.set_config("testprov", "api_key", "config-value".to_string());

        // Set env var
        std::env::set_var("MEEDYA_TESTPROV_API_KEY", "env-value");

        let resolved = store.resolve("testprov", "api_key").unwrap();
        assert_eq!(resolved.value, "env-value");
        assert_eq!(resolved.source, CredentialSource::Environment);

        // Cleanup
        std::env::remove_var("MEEDYA_TESTPROV_API_KEY");
    }

    #[test]
    fn store_and_read_local_file() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("credentials.json");

        let store = CredentialStore::new("test", Some(cred_path.clone()));
        store.store_local_file("spotify", "client_id", "abc123").unwrap();

        let resolved = store.resolve("spotify", "client_id").unwrap();
        assert_eq!(resolved.value, "abc123");
        assert_eq!(resolved.source, CredentialSource::LocalFile);
    }

    #[test]
    fn env_key_normalization() {
        // Hyphens in provider/key names become underscores
        let store = CredentialStore::new("test", None);
        std::env::set_var("MEEDYA_APPLE_MUSIC_API_KEY", "test-val");

        let resolved = store.resolve("apple-music", "api-key").unwrap();
        assert_eq!(resolved.value, "test-val");

        std::env::remove_var("MEEDYA_APPLE_MUSIC_API_KEY");
    }
}
