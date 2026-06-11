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
