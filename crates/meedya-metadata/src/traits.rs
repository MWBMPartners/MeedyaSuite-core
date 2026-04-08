use async_trait::async_trait;

use crate::error::ProviderError;
use crate::types::{ProviderCapabilities, ProviderResult, SearchQuery};

/// Async trait implemented by all metadata providers.
///
/// Each provider (MusicBrainz, Spotify, Apple Music, TMDb, etc.) implements this
/// trait to participate in the unified provider registry.
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Unique name for this provider (e.g., "musicbrainz", "spotify").
    fn name(&self) -> &str;

    /// Declare what this provider can do.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Whether this provider is currently enabled and configured.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Search for metadata matching the given query.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError>;

    /// Look up a specific item by provider-specific ID.
    async fn lookup(&self, id: &str) -> Result<Option<ProviderResult>, ProviderError> {
        let _ = id;
        Err(ProviderError::NotSupported(format!(
            "{} does not support direct lookup",
            self.name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestProvider;

    #[async_trait]
    impl MetadataProvider for TestProvider {
        fn name(&self) -> &str {
            "test"
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::default()
        }

        async fn search(&self, _query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
            Ok(vec![ProviderResult::new("test")])
        }
    }

    #[tokio::test]
    async fn test_provider_trait() {
        let provider = TestProvider;
        assert_eq!(provider.name(), "test");
        assert!(provider.is_enabled());

        let results = provider.search(&SearchQuery::default()).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_default_lookup_not_supported() {
        let provider = TestProvider;
        let result = provider.lookup("123").await;
        assert!(matches!(result, Err(ProviderError::NotSupported(_))));
    }
}
