use thiserror::Error;

/// Errors from metadata provider operations.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("network error: {0}")]
    Network(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("rate limited by provider: {0}")]
    RateLimited(String),

    #[error("operation not supported: {0}")]
    NotSupported(String),

    #[error("provider is disabled: {0}")]
    Disabled(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Errors from tag I/O operations.
#[derive(Debug, Error)]
pub enum TagError {
    #[error("failed to read file: {0}")]
    ReadFailed(String),

    #[error("failed to write tags: {0}")]
    WriteFailed(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("tag not found: {0}")]
    TagNotFound(String),
}

/// Errors from credential operations.
#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential not found for {provider}: {key}")]
    NotFound { provider: String, key: String },

    #[error("keyring error: {0}")]
    KeyringError(String),

    #[error("file I/O error: {0}")]
    IoError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_display() {
        let err = ProviderError::Network("timeout".into());
        assert_eq!(err.to_string(), "network error: timeout");
    }

    #[test]
    fn tag_error_display() {
        let err = TagError::UnsupportedFormat("avi".into());
        assert_eq!(err.to_string(), "unsupported format: avi");
    }

    #[test]
    fn credential_error_display() {
        let err = CredentialError::NotFound {
            provider: "spotify".into(),
            key: "client_id".into(),
        };
        assert!(err.to_string().contains("spotify"));
    }
}
