// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// meedya-db — Shared database schema, models, and MeedyaDB API client
// ====================================================================
//
// Provides:
// - Core media record types (Track, Album, Artist)
// - MeedyaDB API client (search, match, lookup)
// - Database export trait for multiple backends
//
// Extracted from MeedyaConverter MetadataProviders.swift (MeedyaDBClient)
// and MeedyaManager mm-export/ (schema, traits, DB backends).

pub mod models;
pub mod client;
pub mod export;
mod error;

pub use models::{Track, Album, Artist, MediaRecord};
pub use client::MeedyaDbClient;
pub use export::DbExporter;
pub use error::DbError;
