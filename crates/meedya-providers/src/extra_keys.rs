// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Standard metadata keys for fields not natively on `ProviderResult`.
//
// Provider implementations use these constants when populating the
// `result.metadata: HashMap<String, serde_json::Value>` blob, so downstream
// consumers can rely on a consistent set of well-known keys rather than each
// provider inventing its own naming.
//
// These keys carry data the upstream `ProviderResult` struct does not model
// directly: things like album-artist (distinct from track artist), the total
// number of tracks on an album, ISWC / EIDR identifiers, content-advisory
// flags, duration in seconds, BPM, and the provider's internal item ID.
//
// Ported from MeedyaManager `mm-providers` under issue #136 /
// MeedyaSuite-core#12 (the bare upstream names drop the `META_` prefix the
// MM constants used).
//
// Identifier-shaped keys (ISWC, EIDR) are also canonical slugs in the
// cross-repo `meedya_metadata::identifier_types` registry (#65) — the
// `identifier_extra_keys_match_registry_slugs` test below is the in-repo
// mechanism that keeps them from drifting apart (e.g. the `mm_iswc` vs
// `iswc` class of bug MeedyaManager hit).

/// Album artist (when different from the track artist). Stored as `Value::String`.
pub const ALBUM_ARTIST: &str = "album_artist";

/// Total tracks on the album. Stored as `Value::Number(u32)`.
pub const TRACK_TOTAL: &str = "track_total";

/// ISWC (composition identifier). Stored as `Value::String`.
pub const ISWC: &str = "iswc";

/// EIDR (video identifier). Stored as `Value::String`.
pub const EIDR: &str = "eidr";

/// Content advisory label ("explicit", "clean", "PG-13", etc.). Stored as `Value::String`.
pub const CONTENT_ADVISORY: &str = "content_advisory";

/// Duration in seconds. Stored as `Value::Number(f64)`.
pub const DURATION_SECS: &str = "duration_secs";

/// Beats per minute. Stored as `Value::Number(f64)`.
pub const BPM: &str = "bpm";

/// Provider-specific item identifier. Stored as `Value::String`.
pub const PROVIDER_ID: &str = "provider_id";

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier-shaped extra_keys constants ARE registry slugs (#65).
    /// This is the in-repo mechanism (not a comment) preventing the
    /// `mm_iswc`-vs-`iswc` class of key drift MeedyaManager hit.
    #[test]
    fn identifier_extra_keys_match_registry_slugs() {
        for key in [ISWC, EIDR] {
            let t = meedya_metadata::identifier_type(key)
                .unwrap_or_else(|| panic!("extra_keys::{key:?} is not a registry slug"));
            assert_eq!(t.status, meedya_metadata::IdentifierStatus::Active);
        }
    }
}
