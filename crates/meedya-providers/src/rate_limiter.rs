use governor::{Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Per-provider rate limiter using a token-bucket algorithm.
pub struct ProviderRateLimiter {
    limiter: RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
    provider_name: String,
    rpm: u32,
}

impl ProviderRateLimiter {
    /// Create a rate limiter with the given requests-per-minute limit.
    pub fn new(provider_name: impl Into<String>, rpm: u32) -> Self {
        let rpm = rpm.max(1);
        let quota = Quota::per_minute(NonZeroU32::new(rpm).unwrap());
        Self {
            limiter: RateLimiter::direct(quota),
            provider_name: provider_name.into(),
            rpm,
        }
    }

    /// Non-blocking check. Returns `true` if a request is allowed right now.
    pub fn check(&self) -> bool {
        self.limiter.check().is_ok()
    }

    /// Async wait until a request is allowed.
    pub async fn wait_until_ready(&self) {
        self.limiter.until_ready().await;
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn rpm(&self) -> u32 {
        self.rpm
    }
}

/// Registry managing rate limiters for all providers.
pub struct RateLimiterRegistry {
    limiters: RwLock<HashMap<String, Arc<ProviderRateLimiter>>>,
}

impl RateLimiterRegistry {
    pub fn new() -> Self {
        Self {
            limiters: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default RPM limits for well-known providers.
    pub fn with_defaults() -> Self {
        let defaults = [
            ("musicbrainz", 50),
            ("spotify", 100),
            ("apple_music", 60),
            ("deezer", 50),
            ("tmdb", 40),
            ("thetvdb", 30),
            ("omdb", 10),
            ("apple_tv", 60),
            ("itunes_store", 20),
            ("apple_podcasts", 20),
            ("isrc", 10),
            ("eidr", 10),
            ("iswc", 10),
        ];

        let mut map = HashMap::new();
        for (name, rpm) in defaults {
            map.insert(
                name.to_string(),
                Arc::new(ProviderRateLimiter::new(name, rpm)),
            );
        }

        Self {
            limiters: RwLock::new(map),
        }
    }

    /// Get or create a rate limiter for a provider.
    pub async fn get_or_create(&self, provider_name: &str, rpm: u32) -> Arc<ProviderRateLimiter> {
        // Check read first
        if let Some(limiter) = self.limiters.read().await.get(provider_name) {
            return Arc::clone(limiter);
        }

        // Create and insert
        let mut limiters = self.limiters.write().await;
        limiters
            .entry(provider_name.to_string())
            .or_insert_with(|| Arc::new(ProviderRateLimiter::new(provider_name, rpm)))
            .clone()
    }

    /// Get an existing limiter.
    pub async fn get(&self, provider_name: &str) -> Option<Arc<ProviderRateLimiter>> {
        self.limiters.read().await.get(provider_name).cloned()
    }
}

impl Default for RateLimiterRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_requests() {
        let limiter = ProviderRateLimiter::new("test", 100);
        assert!(limiter.check());
        assert_eq!(limiter.provider_name(), "test");
        assert_eq!(limiter.rpm(), 100);
    }

    #[test]
    fn minimum_rpm_is_one() {
        let limiter = ProviderRateLimiter::new("test", 0);
        assert_eq!(limiter.rpm(), 1);
    }

    #[tokio::test]
    async fn registry_get_or_create() {
        let registry = RateLimiterRegistry::new();
        let limiter = registry.get_or_create("spotify", 100).await;
        assert_eq!(limiter.provider_name(), "spotify");

        // Same instance returned on second call
        let limiter2 = registry.get_or_create("spotify", 200).await;
        assert_eq!(limiter2.rpm(), 100); // First creation wins
    }

    #[tokio::test]
    async fn registry_defaults() {
        let registry = RateLimiterRegistry::with_defaults();
        let mb = registry.get("musicbrainz").await;
        assert!(mb.is_some());
        assert_eq!(mb.unwrap().rpm(), 50);
    }
}
