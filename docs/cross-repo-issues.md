# Cross-Project GitHub Issues for MeedyaSuite-core Integration

This document contains pre-drafted GitHub issues for MeedyaDL, MeedyaConverter,
and MeedyaManager to integrate with shared functionality from MeedyaSuite-core.

These issues could not be created via the GitHub MCP tools in the current session
because the session scope is restricted to `mwbmpartners/meedyasuite-core`.

**To create these**: Start a new Claude Code session with all repos connected,
or create them manually using the content below.

---

## MeedyaDL Issues

### Issue DL-1: Replace codec_registry.rs with meedya-codecs dependency

**Title**: `feat: replace local codec_registry with meedya-codecs from MeedyaSuite-core`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`src-tauri/src/models/codec_registry.rs` and `src-tauri/codecs.toml` define audio/video
codec types, per-service flag mappings, and meta-codec resolution locally. This is now
duplicated with `meedya-codecs` in MeedyaSuite-core, which provides a superset of
these types with richer metadata (FFmpeg names, lossless flags, spatial detection, HDR
support, codec-container compatibility).

#### Changes Required

1. Add `meedya-codecs` as a git dependency in `src-tauri/Cargo.toml`:
   ```toml
   meedya-codecs = { git = "https://github.com/MWBMPartners/MeedyaSuite-core" }
   ```
2. Replace `use crate::models::codec_registry::*` with `use meedya_codecs::*`
3. Update `codecs.toml` to use `CodecRegistry::from_toml()` from the shared crate
4. Remove `src-tauri/src/models/codec_registry.rs`
5. Update `mediainfo_service.rs` to use `AudioCodec` enum variants
6. Update all downstream consumers (gamdl_options.rs, settings.rs, etc.)
7. Run `cargo test` to verify

#### Branch

`feature/MeedyaDL_MeedyaSuite-core_integration`

---

### Issue DL-2: Replace tag_registry.rs with meedya-metadata dependency

**Title**: `feat: replace local tag_registry with meedya-metadata from MeedyaSuite-core`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`src-tauri/src/models/tag_registry.rs` and `src-tauri/tags.toml` define tag schemas,
JSON path extraction, and value conversion locally. This is now provided by
`meedya-metadata` in MeedyaSuite-core with identical types plus cross-format field
mapping (iTunes atoms, Vorbis Comments, ID3v2).

#### Changes Required

1. Add `meedya-metadata` as a git dependency
2. Replace local `TagRegistry`, `TagDefinition`, `TagValueType`, `AtomTarget` with
   `meedya_metadata::*` equivalents
3. Replace `extract_json_value()` and `value_to_string()` with shared versions
4. Move `tags.toml` to use the shared crate's loading mechanism
5. Update `metadata_tag_service.rs` to use `meedya_metadata::write_tags()` and
   `meedya_metadata::write_registry_tags()` instead of direct `mp4ameta` calls
6. Remove `src-tauri/src/models/tag_registry.rs`
7. Run `cargo test` to verify

#### Branch

`feature/MeedyaDL_MeedyaSuite-core_integration`

---

### Issue DL-3: Replace acoustid/replaygain services with meedya-fingerprint dependency

**Title**: `feat: replace local AcoustID/ReplayGain with meedya-fingerprint from MeedyaSuite-core`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`src-tauri/src/services/acoustid_service.rs` and `replaygain_service.rs` implement
fingerprinting and loudness analysis locally. These are now provided by
`meedya-fingerprint` (analysis) + `meedya-metadata` (tag writing) in MeedyaSuite-core.

#### Changes Required

1. Add `meedya-fingerprint` and update `meedya-metadata` dependencies
2. Replace local `AcoustIdClient` usage with `meedya_fingerprint::AcoustIdClient`
3. Replace local ReplayGain analysis with `meedya_fingerprint::ReplayGainAnalyzer`
4. Replace manual tag writing with `meedya_metadata::write_replaygain_tags()` and
   `meedya_metadata::write_acoustid_tags()` one-liners
5. Remove `src-tauri/src/services/acoustid_service.rs`
6. Remove `src-tauri/src/services/replaygain_service.rs`
7. Keep app-specific concerns: settings integration, progress events, opt-in flags
8. Run `cargo test` to verify

#### Branch

`feature/MeedyaDL_MeedyaSuite-core_integration`

---

### Issue DL-4: Replace musicbrainz_service.rs with meedya-providers dependency

**Title**: `feat: replace local MusicBrainz service with meedya-providers from MeedyaSuite-core`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`src-tauri/src/services/musicbrainz_service.rs` implements MusicBrainz API integration
locally. This will be provided by `meedya-providers` in MeedyaSuite-core as a shared
provider implementation.

#### Changes Required

1. Add `meedya-providers` as a git dependency (once implemented — blocked by MeedyaSuite-core #2)
2. Replace local MusicBrainz client with shared provider
3. Remove `src-tauri/src/services/musicbrainz_service.rs`

#### Blocked By

MeedyaSuite-core #2 (meedya-providers implementation)

#### Branch

`feature/MeedyaDL_MeedyaSuite-core_integration`

---

## MeedyaConverter Issues

### Issue MC-1: Evaluate MeedyaSuite-core Swift bindings for codec/format types

**Title**: `feat: evaluate MeedyaSuite-core Swift bindings for shared codec/format types`
**Labels**: `enhancement`, `meedyasuite-core`

**Body**:

#### Problem

`Sources/ConverterEngine/Models/` contains Swift enum definitions for:
- `AudioCodec.swift` (42 cases)
- `VideoCodec.swift` (21 cases)
- `ContainerFormat.swift` (28 cases)
- `SubtitleFormat.swift` (15 cases)

Plus extended versions in `FFmpeg/Extended*.swift`. These are now canonically defined
in `meedya-codecs` (Rust) in MeedyaSuite-core with a superset of all variants.

#### Changes Required

1. **Blocked by**: MeedyaSuite-core #3 (Swift C FFI bindings)
2. Once Swift bindings exist, evaluate replacing local Swift enums with the
   generated Swift wrappers from MeedyaSuite-core
3. Map FFmpeg-specific properties (encoder/decoder names) through the shared types
4. Keep MeedyaConverter-specific logic (encoding profiles, hardware detection) local

#### Notes

MeedyaConverter is pure Swift — it cannot use Rust crates directly. Integration
requires the C FFI / XCFramework bindings scaffolded in MeedyaSuite-core `bindings/swift/`.

#### Branch

`feature/MeedyaConverter_MeedyaSuite-core_integration`

---

### Issue MC-2: Replace MeedyaDBClient with shared meedya-db client (via Swift bindings)

**Title**: `feat: replace local MeedyaDBClient with shared meedya-db client`
**Labels**: `enhancement`, `meedyasuite-core`

**Body**:

#### Problem

`Sources/ConverterEngine/Metadata/MetadataProviders.swift` contains `MeedyaDBClient`
with search, match-by-filename, and X-API-Key auth. This is now provided by
`meedya-db::MeedyaDbClient` in MeedyaSuite-core.

#### Changes Required

1. **Blocked by**: MeedyaSuite-core #3 (Swift C FFI bindings)
2. Replace local `MeedyaDBClient` Swift class with shared Rust client via FFI
3. Update `APIKeyManager.swift` to use shared credential management

#### Branch

`feature/MeedyaConverter_MeedyaSuite-core_integration`

---

### Issue MC-3: Replace metadata provider implementations with shared meedya-providers

**Title**: `feat: replace local metadata providers with shared meedya-providers`
**Labels**: `enhancement`, `meedyasuite-core`

**Body**:

#### Problem

`Sources/ConverterEngine/Metadata/MetadataLookup.swift` and `MetadataProviders.swift`
implement 7+ providers (TMDB, TheTVDB, MusicBrainz, Discogs, FanArt.tv, OpenSubtitles,
OMDb) independently. These will be centralised in `meedya-providers`.

#### Changes Required

1. **Blocked by**: MeedyaSuite-core #2 (meedya-providers) and #3 (Swift bindings)
2. Replace local provider implementations with shared versions via FFI
3. Keep MeedyaConverter-specific UI/orchestration logic local

#### Branch

`feature/MeedyaConverter_MeedyaSuite-core_integration`

---

## MeedyaManager Issues

### Issue MM-1: Replace mm-core classify/filetype_registry with meedya-codecs

**Title**: `feat: replace mm-core classify and filetype_registry with meedya-codecs`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`crates/mm-core/src/classify/mod.rs` and `crates/mm-core/src/filetype_registry.rs`
define a 4-level media classification system and 100+ file extension mappings locally.
This is now provided by `meedya-codecs` in MeedyaSuite-core with identical types.

#### Changes Required

1. Add `meedya-codecs` as a git dependency in workspace `Cargo.toml`
2. Replace `classify::MediaGroup/MediaFormat/MediaClass/MediaQuality` with
   `meedya_codecs::classify::*`
3. Replace `filetype_registry.rs` with `ContainerFormat::from_extension()`
4. Update `filetypes.json5` to use shared `containers.toml` format
5. Remove `crates/mm-core/src/classify/mod.rs`
6. Remove `crates/mm-core/src/filetype_registry.rs`
7. Update all consumers (companion, metadata, settings_bundle, etc.)
8. Run `cargo test` to verify

#### Branch

`feature/MeedyaManager_MeedyaSuite-core_integration`

---

### Issue MM-2: Replace mm-core metadata/tag_registry with meedya-metadata

**Title**: `feat: replace mm-core tag_registry with meedya-metadata`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`crates/mm-core/src/metadata/tag_registry.rs` and `config/tags.json5` define tag
schemas locally. This is now provided by `meedya-metadata` in MeedyaSuite-core.

#### Changes Required

1. Add `meedya-metadata` as a git dependency
2. Replace local `TagRegistry`/`TagDefinition`/`TagValueType` with shared types
3. Replace `tags.json5` with shared `tags.toml` loading via `TagRegistry::from_toml()`
4. Update `metadata/mod.rs` to use `meedya_metadata::write_tags()` for file I/O
5. Update `rule_engine/tag_registry.rs` to reference shared types
6. Update UniFFI bindings (`mm-ffi/uniffi_api.rs`) to expose shared types
7. Run `cargo test` to verify

#### Branch

`feature/MeedyaManager_MeedyaSuite-core_integration`

---

### Issue MM-3: Replace mm-export with meedya-db shared schema

**Title**: `feat: replace mm-export schema/traits with meedya-db`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`crates/mm-export/src/` defines `DbExporter` trait, schema, and 5 database backends
(SQLite, MySQL, MariaDB, PostgreSQL, SQL Server) locally. The trait and schema are now
in `meedya-db`.

#### Changes Required

1. Add `meedya-db` as a git dependency
2. Replace local `DbExporter` trait with `meedya_db::DbExporter`
3. Replace local schema definitions with `meedya_db::export::schema::*`
4. Replace local `Track`/`Album`/`Artist` models with `meedya_db::models::*`
5. Keep database backend implementations (sqlite.rs, mysql.rs, etc.) local —
   they implement the shared trait but contain backend-specific SQL
6. Run `cargo test` to verify

#### Branch

`feature/MeedyaManager_MeedyaSuite-core_integration`

---

### Issue MM-4: Replace mm-providers traits with meedya-providers

**Title**: `feat: replace mm-providers traits and shared providers with meedya-providers`
**Labels**: `enhancement`, `refactor`, `meedyasuite-core`

**Body**:

#### Problem

`crates/mm-providers/src/` defines `BaseProvider` trait, registry, rate limiter,
credential manager, and 15+ provider implementations. The framework layer should
be shared via `meedya-providers` so all apps use the same provider infrastructure.

#### Changes Required

1. **Blocked by**: MeedyaSuite-core #2 (meedya-providers implementation)
2. Replace local `BaseProvider` trait with `meedya_providers::MetadataProvider`
3. Replace local rate limiter with shared implementation
4. Replace local credential manager with shared implementation
5. Migrate shared providers (MusicBrainz, TMDB, TheTVDB, etc.) to shared crate
6. Keep MeedyaManager-specific providers local if needed

#### Branch

`feature/MeedyaManager_MeedyaSuite-core_integration`
