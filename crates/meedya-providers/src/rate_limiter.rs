// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Provider rate limiting.
//
// Every provider in `providers/` throttles itself through this module: each
// holds an `Arc<ProviderRateLimiter>` handed out by `default_limiter_for` and
// awaits it immediately before each outbound request. Limiters are keyed by
// *upstream host budget* rather than provider name, and the table is
// process-global, so throttling holds across provider instances — see
// `default_limiter_for` and MeedyaSuite-core#94.

use governor::{Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// Per-provider rate limiter using a token-bucket algorithm.
///
/// There are two constructors and the difference between them matters:
/// `governor` treats a quota's cell count as its **burst capacity** as well as
/// its replenishment rate, so [`ProviderRateLimiter::new`] with `rpm = 50`
/// admits 50 requests back-to-back before throttling anything. That is the
/// right shape for a service publishing a per-minute allowance and the wrong
/// shape for one policing an average rate per second — MusicBrainz answers a
/// 50-request burst with 503s. Use [`ProviderRateLimiter::per_second`] there.
pub struct ProviderRateLimiter {
    limiter: RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
    provider_name: String,
    rpm: u32,
    burst: u32,
}

impl ProviderRateLimiter {
    /// Create a rate limiter with the given requests-per-minute limit.
    ///
    /// Burst capacity equals `rpm` (see the type-level note). For a service
    /// that polices a per-second average, use [`Self::per_second`].
    pub fn new(provider_name: impl Into<String>, rpm: u32) -> Self {
        let rpm = rpm.max(1);
        let quota = Quota::per_minute(NonZeroU32::new(rpm).unwrap());
        Self {
            limiter: RateLimiter::direct(quota),
            provider_name: provider_name.into(),
            rpm,
            burst: rpm,
        }
    }

    /// Create a rate limiter with the given requests-per-second limit.
    ///
    /// Burst capacity is `rps`, so `per_second(1)` releases requests strictly
    /// one at a time rather than letting a batch loop fire a minute's worth at
    /// once. [`Self::rpm`] reports the equivalent sustained rate (`rps * 60`);
    /// [`Self::burst`] is what distinguishes this from `new(name, rps * 60)`.
    pub fn per_second(provider_name: impl Into<String>, rps: u32) -> Self {
        let rps = rps.max(1);
        let quota = Quota::per_second(NonZeroU32::new(rps).unwrap());
        Self {
            limiter: RateLimiter::direct(quota),
            provider_name: provider_name.into(),
            rpm: rps.saturating_mul(60),
            burst: rps,
        }
    }

    /// Non-blocking check. Returns `true` if a request is allowed right now,
    /// **consuming** the cell it just admitted; `false` leaves the bucket
    /// untouched. For fail-fast callers that would rather skip a provider than
    /// queue behind it — the providers themselves use [`Self::wait_until_ready`].
    pub fn check(&self) -> bool {
        self.limiter.check().is_ok()
    }

    /// Async wait until a request is allowed.
    pub async fn wait_until_ready(&self) {
        self.limiter.until_ready().await;
    }

    /// Name of the budget this limiter governs. For the process defaults this
    /// is the upstream host (e.g. `"musicbrainz.org"`), not a provider id,
    /// because several providers share one host budget.
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Sustained rate in requests per minute.
    pub fn rpm(&self) -> u32 {
        self.rpm
    }

    /// How many requests may be issued back-to-back before throttling starts.
    pub fn burst(&self) -> u32 {
        self.burst
    }
}

/// Requests-per-minute granted to a provider id absent from the default
/// table. Deliberately conservative: an unrecognised id means nobody has
/// checked that service's published limit, so under-asking is the safe error.
const FALLBACK_RPM: u32 = 30;

/// Build the process-global default limiters, keyed by provider id.
///
/// Several ids deliberately map to the *same* `Arc`: the budget belongs to the
/// upstream host, not to the provider struct pointed at it. Four Apple
/// providers each holding their own 20 RPM limiter would issue 80 RPM against
/// one iTunes budget from one IP.
///
/// Every number carries its source. Where a service publishes no limit the
/// figure is a conservative guess and is labelled as one.
fn build_default_limiters() -> HashMap<&'static str, Arc<ProviderRateLimiter>> {
    let mut map: HashMap<&'static str, Arc<ProviderRateLimiter>> = HashMap::new();

    // musicbrainz.org — the web service documents "no more than one request
    // per second on average" for anonymous clients, enforced per source IP.
    // per_second(1), not per_minute(60): a 60-request burst is exactly what
    // the published rule forbids, and MusicBrainz answers it with 503s.
    // Shared by all three MusicBrainz-backed providers.
    let musicbrainz = Arc::new(ProviderRateLimiter::per_second("musicbrainz.org", 1));
    for id in ["musicbrainz", "isrc", "iswc"] {
        map.insert(id, Arc::clone(&musicbrainz));
    }

    // itunes.apple.com — Apple's iTunes Search API documentation states the
    // search endpoint is limited to approximately 20 calls per minute, again
    // per IP. Shared by every provider that queries that one endpoint.
    let itunes = Arc::new(ProviderRateLimiter::new("itunes.apple.com", 20));
    for id in ["apple_music", "apple_tv", "itunes_store", "apple_podcasts"] {
        map.insert(id, Arc::clone(&itunes));
    }

    // One host, one provider — no sharing to arrange.
    let solo: [(&'static str, &'static str, u32); 6] = [
        // Spotify publishes no figure (its limit is computed over a rolling
        // 30-second window and varies by app). Inherited MeedyaManager value.
        ("spotify", "api.spotify.com", 100),
        // Deezer documents 50 requests per 5 seconds per IP; 50 RPM sits an
        // order of magnitude under that.
        ("deezer", "api.deezer.com", 50),
        // TMDb's published limit was 40 requests / 10 seconds before TMDb
        // withdrew the documented number; it still throttles. 40 RPM stays
        // well inside the old figure.
        ("tmdb", "api.themoviedb.org", 40),
        // TheTVDB publishes no per-minute limit — conservative guess.
        ("thetvdb", "api4.thetvdb.com", 30),
        // OMDb's free tier is 1,000 requests/day with no per-minute figure;
        // 10 RPM stops a batch run burning the daily budget in two minutes.
        ("omdb", "www.omdbapi.com", 10),
        // EIDR is a paid registry API publishing no per-minute limit —
        // conservative guess.
        ("eidr", "id.eidr.org", 10),
    ];
    for (id, host, rpm) in solo {
        map.insert(id, Arc::new(ProviderRateLimiter::new(host, rpm)));
    }

    map
}

/// The process-global default table, built once.
fn default_limiters() -> &'static HashMap<&'static str, Arc<ProviderRateLimiter>> {
    static DEFAULT_LIMITERS: OnceLock<HashMap<&'static str, Arc<ProviderRateLimiter>>> =
        OnceLock::new();
    DEFAULT_LIMITERS.get_or_init(build_default_limiters)
}

/// The default rate limiter for a provider id, shared process-wide.
///
/// This is what every provider's constructor calls, and the sharing is the
/// point. A limiter owned per provider instance would throttle nothing useful:
/// batch callers construct a provider per task, so N instances would mean N
/// independent budgets pointed at one upstream. Two calls with the same id —
/// or with two ids that share a host budget, e.g. `"musicbrainz"` and
/// `"isrc"` — return the same `Arc`.
///
/// Ids absent from the table share one conservative fallback limiter
/// (30 RPM). Apps needing a different budget build their own limiter and
/// install it with each provider's `with_rate_limiter`, or manage a set
/// through [`RateLimiterRegistry`].
pub fn default_limiter_for(provider_id: &str) -> Arc<ProviderRateLimiter> {
    static FALLBACK: OnceLock<Arc<ProviderRateLimiter>> = OnceLock::new();

    if let Some(limiter) = default_limiters().get(provider_id) {
        return Arc::clone(limiter);
    }
    Arc::clone(
        FALLBACK
            .get_or_init(|| Arc::new(ProviderRateLimiter::new("unknown-provider", FALLBACK_RPM))),
    )
}

/// Registry managing rate limiters for all providers.
///
/// This is the app-level mechanism for *custom* budgets — a self-hosted
/// MusicBrainz mirror with no 1 RPS rule, a paid API tier, a shared budget
/// across several apps. Provider defaults do not come from here; they come
/// from [`default_limiter_for`]. To make a provider use a registry entry,
/// hand it over explicitly with that provider's `with_rate_limiter`.
pub struct RateLimiterRegistry {
    limiters: RwLock<HashMap<String, Arc<ProviderRateLimiter>>>,
}

impl RateLimiterRegistry {
    pub fn new() -> Self {
        Self {
            limiters: RwLock::new(HashMap::new()),
        }
    }

    /// Create pre-populated with the process-global defaults.
    ///
    /// The entries are the *same* `Arc`s [`default_limiter_for`] hands to
    /// providers, so a registry built this way observes and shares the budgets
    /// already in force rather than opening a parallel set of them.
    pub fn with_defaults() -> Self {
        let map = default_limiters()
            .iter()
            .map(|(id, limiter)| ((*id).to_string(), Arc::clone(limiter)))
            .collect();

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

    #[test]
    fn minimum_rps_is_one() {
        let limiter = ProviderRateLimiter::per_second("test", 0);
        assert_eq!(limiter.burst(), 1);
        assert_eq!(limiter.rpm(), 60);
    }

    // The burst tests below assert against a *locally owned* limiter, never a
    // process-global one, and never sleep: `check()` consumes a cell, so the
    // bucket's capacity is observable immediately. Replenishment is on a real
    // clock (a per_minute(3) cell takes 20s to come back), so a test that
    // waited for one would be a minute-long test.
    #[test]
    fn per_second_one_admits_a_single_request_then_throttles() {
        let limiter = ProviderRateLimiter::per_second("test", 1);
        assert!(limiter.check(), "first request must be admitted");
        assert!(
            !limiter.check(),
            "per_second(1) must not let a tight loop burst past its quota"
        );
    }

    #[test]
    fn per_minute_burst_capacity_equals_rpm() {
        // Documents the trap per_second exists to avoid: a per-minute quota
        // admits its whole minute's worth back-to-back.
        let limiter = ProviderRateLimiter::new("test", 3);
        assert_eq!(limiter.burst(), 3);
        for i in 0..3 {
            assert!(limiter.check(), "request {i} should be inside the burst");
        }
        assert!(!limiter.check(), "the 4th request must be throttled");
    }

    #[test]
    fn default_limiter_for_returns_one_shared_instance_per_id() {
        // The property the whole design rests on: providers constructed
        // independently must land on the same budget.
        let a = default_limiter_for("deezer");
        let b = default_limiter_for("deezer");
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn musicbrainz_backed_providers_share_one_host_budget() {
        let mb = default_limiter_for("musicbrainz");
        assert!(Arc::ptr_eq(&mb, &default_limiter_for("isrc")));
        assert!(Arc::ptr_eq(&mb, &default_limiter_for("iswc")));
        assert_eq!(mb.provider_name(), "musicbrainz.org");
    }

    #[test]
    fn itunes_backed_providers_share_one_host_budget() {
        let am = default_limiter_for("apple_music");
        for id in ["apple_tv", "itunes_store", "apple_podcasts"] {
            assert!(
                Arc::ptr_eq(&am, &default_limiter_for(id)),
                "{id} must share the itunes.apple.com budget"
            );
        }
        assert_eq!(am.provider_name(), "itunes.apple.com");
    }

    #[test]
    fn distinct_hosts_get_distinct_budgets() {
        assert!(!Arc::ptr_eq(
            &default_limiter_for("musicbrainz"),
            &default_limiter_for("deezer")
        ));
    }

    #[test]
    fn musicbrainz_default_is_one_request_per_second() {
        let mb = default_limiter_for("musicbrainz");
        assert_eq!(mb.burst(), 1, "a burst would draw 503s from MusicBrainz");
        assert_eq!(mb.rpm(), 60);
    }

    #[test]
    fn unknown_provider_ids_share_the_conservative_fallback() {
        let a = default_limiter_for("some-future-provider");
        let b = default_limiter_for("another-unknown");
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.rpm(), FALLBACK_RPM);
        assert!(!Arc::ptr_eq(&a, &default_limiter_for("tmdb")));
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
        // 1 req/sec, reported as its sustained per-minute equivalent.
        assert_eq!(mb.unwrap().rpm(), 60);
    }

    #[tokio::test]
    async fn registry_defaults_are_the_process_global_limiters() {
        let registry = RateLimiterRegistry::with_defaults();
        let from_registry = registry.get("tmdb").await.expect("tmdb is a default");
        assert!(Arc::ptr_eq(&from_registry, &default_limiter_for("tmdb")));
    }
}
