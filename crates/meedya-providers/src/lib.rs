// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// meedya-providers — Shared metadata provider framework
// ======================================================
//
// Centralised provider traits, registry, rate limiting, credential
// management, and shared implementations for metadata lookup services
// (MusicBrainz, TMDB, TheTVDB, AcoustID, Discogs, etc.) used across
// all MeedyaSuite applications.
//
// Placeholder — implementation follows meedya-codecs and meedya-metadata.

pub mod traits;

pub use traits::MetadataProvider;
