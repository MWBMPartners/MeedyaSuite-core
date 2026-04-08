use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::ProviderError;
use crate::match_scoring::MatchScorer;
use crate::traits::MetadataProvider;
use crate::types::{MediaType, ProviderResult, SearchQuery};

/// Central registry that manages and dispatches to multiple metadata providers.
pub struct ProviderRegistry {
    providers: RwLock<Vec<Arc<dyn MetadataProvider>>>,
    scorer: MatchScorer,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(Vec::new()),
            scorer: MatchScorer::default(),
        }
    }

    /// Register a provider.
    pub async fn register(&self, provider: Arc<dyn MetadataProvider>) {
        info!("Registering metadata provider: {}", provider.name());
        self.providers.write().await.push(provider);
    }

    /// Get all providers that support a given media type and are enabled.
    pub async fn providers_for(&self, media_type: MediaType) -> Vec<Arc<dyn MetadataProvider>> {
        self.providers
            .read()
            .await
            .iter()
            .filter(|p| {
                p.is_enabled() && p.capabilities().media_types.contains(&media_type)
            })
            .cloned()
            .collect()
    }

    /// Find a provider by name.
    pub async fn find_by_name(&self, name: &str) -> Option<Arc<dyn MetadataProvider>> {
        self.providers
            .read()
            .await
            .iter()
            .find(|p| p.name() == name)
            .cloned()
    }

    /// Search across all enabled providers for the query's media type.
    ///
    /// Fans out concurrently to all matching providers, collects results,
    /// scores them, and returns sorted by confidence (highest first).
    pub async fn search(&self, query: &SearchQuery) -> Vec<ProviderResult> {
        let media_type = query.media_type.unwrap_or(MediaType::Music);
        let providers = self.providers_for(media_type).await;

        if providers.is_empty() {
            warn!("No enabled providers for media type {:?}", media_type);
            return Vec::new();
        }

        // Fan out searches concurrently
        let mut handles = Vec::new();
        for provider in providers {
            let q = query.clone();
            handles.push(tokio::spawn(async move {
                match provider.search(&q).await {
                    Ok(results) => results,
                    Err(e) => {
                        warn!("Provider {} search failed: {}", provider.name(), e);
                        Vec::new()
                    }
                }
            }));
        }

        let mut all_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(results) => all_results.extend(results),
                Err(e) => warn!("Provider task panicked: {}", e),
            }
        }

        // Score results against the query
        for result in &mut all_results {
            let computed_score = self.scorer.score(query, result);
            // Use the higher of provider-assigned or computed score
            if computed_score > result.score {
                result.score = computed_score;
            }
        }

        // Sort by score descending
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply max_results limit
        if let Some(max) = query.max_results {
            all_results.truncate(max);
        }

        all_results
    }

    /// Search a single named provider.
    pub async fn search_provider(
        &self,
        provider_name: &str,
        query: &SearchQuery,
    ) -> Result<Vec<ProviderResult>, ProviderError> {
        let provider = self
            .find_by_name(provider_name)
            .await
            .ok_or_else(|| ProviderError::NotSupported(format!("provider not found: {provider_name}")))?;

        if !provider.is_enabled() {
            return Err(ProviderError::Disabled(provider_name.to_string()));
        }

        provider.search(query).await
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderCapabilities;
    use async_trait::async_trait;

    struct MockProvider {
        name: String,
        media_type: MediaType,
        enabled: bool,
    }

    #[async_trait]
    impl MetadataProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                media_types: vec![self.media_type],
                ..Default::default()
            }
        }

        fn is_enabled(&self) -> bool {
            self.enabled
        }

        async fn search(&self, _query: &SearchQuery) -> Result<Vec<ProviderResult>, ProviderError> {
            let mut result = ProviderResult::new(&self.name);
            result.title = Some("Test Title".to_string());
            result.artist = Some("Test Artist".to_string());
            Ok(vec![result])
        }
    }

    #[tokio::test]
    async fn register_and_find() {
        let registry = ProviderRegistry::new();
        let provider = Arc::new(MockProvider {
            name: "test".to_string(),
            media_type: MediaType::Music,
            enabled: true,
        });
        registry.register(provider).await;

        assert!(registry.find_by_name("test").await.is_some());
        assert!(registry.find_by_name("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn providers_filtered_by_media_type() {
        let registry = ProviderRegistry::new();
        registry
            .register(Arc::new(MockProvider {
                name: "music1".to_string(),
                media_type: MediaType::Music,
                enabled: true,
            }))
            .await;
        registry
            .register(Arc::new(MockProvider {
                name: "video1".to_string(),
                media_type: MediaType::Video,
                enabled: true,
            }))
            .await;

        let music = registry.providers_for(MediaType::Music).await;
        assert_eq!(music.len(), 1);
        assert_eq!(music[0].name(), "music1");
    }

    #[tokio::test]
    async fn disabled_providers_excluded() {
        let registry = ProviderRegistry::new();
        registry
            .register(Arc::new(MockProvider {
                name: "disabled".to_string(),
                media_type: MediaType::Music,
                enabled: false,
            }))
            .await;

        let results = registry.providers_for(MediaType::Music).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_returns_sorted_results() {
        let registry = ProviderRegistry::new();
        registry
            .register(Arc::new(MockProvider {
                name: "provider_a".to_string(),
                media_type: MediaType::Music,
                enabled: true,
            }))
            .await;
        registry
            .register(Arc::new(MockProvider {
                name: "provider_b".to_string(),
                media_type: MediaType::Music,
                enabled: true,
            }))
            .await;

        let query = SearchQuery {
            title: Some("Test Title".to_string()),
            ..Default::default()
        };
        let results = registry.search(&query).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_provider_not_found() {
        let registry = ProviderRegistry::new();
        let result = registry
            .search_provider("nonexistent", &SearchQuery::default())
            .await;
        assert!(matches!(result, Err(ProviderError::NotSupported(_))));
    }
}
