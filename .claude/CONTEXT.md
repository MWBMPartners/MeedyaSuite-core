# MeedyaSuite-core — Project Context

> Snapshot maintained for Claude Code sessions. Reflects the actual state of `main`, not aspirational state.
> Last updated: 2026-09-28 — `meedya-lyrics` and `meedya-metadata` brought into line with policy
> **MWBM-MEDIA-LANG** (the shared `meedya-lang` crate's LANG-002/LANG-003/TRACK-070 rules):
> `embed::DEFAULT_LANGUAGE` no longer guesses English, `xml:lang` is read through the shared
> reader, and `CommonTag::Language` is written per format instead of passed through unchanged.
> Workspace and per-crate test counts re-measured. All of this is on branch
> `feature/bcp47-language-policy` — not yet merged into `feature/work-in-progress`, so treat the
> counts here as "workspace state including that branch", not as `feature/work-in-progress`
> itself, until the two are reconciled. Previous note: 2026-09-28 (earlier the same day) — the
> workspace and per-crate test counts and the crate table below first came to include the
> **`meedya-lang`** crate itself (shared implementation of the policy). Earlier still: 2026-09-23
> — standing rules re-issued (see CLAUDE.md); no code change since 2026-09-09,
> when the **`meedya-audio-analysis`** crate (tempo + key, #16) landed. Earlier: 2026-09-01
> branch-consolidation pass — four WIP branches merged into `feature/work-in-progress`. See
> [HANDOFF.md](HANDOFF.md) §0 for in-flight state and [HISTORY.md](HISTORY.md) for the
> narrative.

## What this repo is

`MeedyaSuite-core` is the shared Rust workspace consumed by the MeedyaSuite app family:

- **MeedyaConverter** (Swift, macOS) — audio/video conversion + tagging
- **MeedyaManager** (Rust + Swift/C#/GTK4) — local library management + tagging
- **MeedyaDL** (Rust/Tauri + React/TS) — store downloads (Apple Music etc.)
- **MeedyaPlayer** (planned) — MeedyaSuite-native media player
- **MeedyaDB** (Swift, planned)

Apps consume this via direct Cargo git dependency (Rust apps) or C FFI / WASM bindings (Swift/web — bindings not yet scaffolded). No app-specific logic lives in this workspace.

## Workspace state (feature/work-in-progress plus `meedya-lang` from `feature/bcp47-language-policy`, not yet reconciled; PR target main)

| Crate | Purpose | Status | Tests |
|---|---|---|---|
| [meedya-codecs](../crates/meedya-codecs/) | Audio/video/subtitle codecs, container formats, HDR, spatial audio, classification, FFprobe + MediaInfo integration | **Implemented** | 47 |
| [meedya-metadata](../crates/meedya-metadata/) | Two coexisting tag I/O surfaces: `lofty`-backed (multi-format) and `mp4ameta`-backed (sandbox-safe). Tag registry, JSON path extraction, codec ID tags, playback bounds, cross-repo `identifier_types` registry (#65). | **Implemented** | 177 |
| [meedya-tags-extended](../crates/meedya-tags-extended/) | Multi-format DJ metadata (lofty). `ExtendedTags`/`MusicalKey`/`CuePoint`/`LoopPoint`/`BeatGrid`. Standard BPM+key+comment + Mixed In Key reader (`mik`). Other proprietary readers pending. | **Implemented (foundation + MIK)** | 180 |
| [meedya-library-import](../crates/meedya-library-import/) | External library ingestion: iTunes XML, CUE sheets. Emits normalized `LibraryEntry` records. | **Implemented** | 30 |
| [meedya-lyrics](../crates/meedya-lyrics/) | LRCLIB client, LRC parser/writer, sidecar I/O, plain-text and SYLT tag-embed. | **Implemented** | 157 |
| [meedya-providers](../crates/meedya-providers/) | Provider framework: traits, capabilities, rate limiting, credentials, cover art, fuzzy match scoring, Lucene/Solr query escaping (`lucene`). In-repo `MetadataProvider` impls (feature-gated): MusicBrainz, Spotify, Apple Music, Deezer, TMDB, TheTVDB, OMDb, Apple TV, iTunes Store, Apple Podcasts, ISRC, EIDR, ISWC. | **Implemented** | 59 (201 all-features) |
| [meedya-audio-analysis](../crates/meedya-audio-analysis/) | Tempo (BPM) and musical key detection from the audio itself. Refuses to answer rather than guess when it is not sure. Shares one decode pass between the two, then takes two different frequency analyses because tempo needs fine timing and key needs fine pitch. | **Implemented** | 67 |
| [meedya-fingerprint](../crates/meedya-fingerprint/) | AcoustID client + ReplayGain EBU R128 analyser (bounded FFmpeg subprocess). Pure-Rust Chromaprint fingerprint generation (no fpcalc) behind the non-default `chromaprint` feature. | **Implemented** | 12 (17 all-features) |
| [meedya-lang](../crates/meedya-lang/) | Shared implementation of the Media Language & BCP 47 Policy (`MWBM-MEDIA-LANG`): canonical tag parsing, stored order, presentation order, role ordering, preference matching (never guessing which of two same-ID tracks was meant — refuses with an error instead), sidecar naming. Conformance-tested against a 290-case fixture shared with the PHP implementation; the test runner also refuses a damaged case file, including a wrong type, a forbidden null or a list where an object belongs anywhere (21 tests prove it, three of them running tables of damaged copies). No feature flags — same test count either way. | **Implemented** | 120 |
| [meedya-db](../crates/meedya-db/) | MeedyaDB API client + `Track`/`Album`/`Artist` models + `DbExporter` trait. | **Implemented** | 4 |
| [meedya-core](../crates/meedya-core/) | Facade re-exporting all implemented crates behind feature flags. | **Implemented** | — |

**Total: 853 tests with default features, 1000 with `--all-features`** (the CI configuration) on `feature/bcp47-language-policy`. All passing, 0 failing.

> **Measured, not carried forward.** From `cargo test --workspace [--all-features] --locked` run on 2026-09-28, after the fixes for Codex's review of revisions 5–7 of policy MWBM-MEDIA-LANG (revision 8: `meedya-metadata` +15 tests, `meedya-lyrics` +1). Earlier measurements the same day: 837 / 984 (after the stand-in review of revision 6), 812 / 959 (after the stand-in review of revision 5), 796 / 943 (after Codex's review r7), 780 / 927 (after policy revision 4), 736 / 883 (after bringing `meedya-lyrics` and `meedya-metadata` into line with the policy), 716 / 863 (after adding `meedya-lang`); before `meedya-lang`, 644 / 791. Earlier revisions accumulated a narrative of incremental deltas (466 → 511 → 533 → 546 → 664) that had drifted from reality. Doc-count drift is this repo's chronic failure mode: **only ever write a number you just measured.** CI guarding is tracked in issue #71.

Per-crate, `--all-features`: `meedya-audio-analysis` 67 · `meedya-codecs` 47 · `meedya-core` 0 · `meedya-db` 4 · `meedya-fingerprint` 17 · `meedya-lang` 120 · `meedya-library-import` 30 · `meedya-lyrics` 157 · `meedya-metadata` 177 · `meedya-providers` 201 · `meedya-tags-extended` 180 (sum 1000). `meedya-providers` measures 59 with default features (provider impls are feature-gated), `meedya-fingerprint` 12; with default features the per-crate figures sum to 853.

> **Public API specification for partner apps**: see [`docs/API.md`](../docs/API.md). Keep that file in sync with public API changes — see the standing task in [CLAUDE.md](CLAUDE.md#standing-tasks).

## Module-level detail

### meedya-codecs

Public surface: `AudioCodec` (42+ variants), `VideoCodec` (21+), `ContainerFormat` (36+), `ChannelConfig`, `HdrFormat`, `SpatialAudioFormat`, `SpatialType`, `SubtitleCodec`, `CodecRegistry`, `MediaClassification`. Modules: `audio_codec`, `video_codec`, `container`, `channel_config`, `classify`, `ffprobe`, `mediainfo`, `hdr`, `spatial`, `spatial_type`, `subtitle_codec`, `registry`, `tool_path`.

### meedya-metadata

Two surfaces coexist by design:

- **`lofty`-backed**: `common_tags` (CommonTag enum — `#[non_exhaustive]` as of #65/0.2.0, STANDARD_NAMESPACES), `tag_io` (read_tags, write_tags, write_registry_tags, write_acoustid_tags, write_replaygain_tags, TagMap), `tag_registry` (TagDefinition, TagRegistry, TagScope, TagValueType, AtomTarget), `json_path`. **`CommonTag::Language` follows policy MWBM-MEDIA-LANG's TRACK-070** (feature/bcp47-language-policy): `write_tags` reads the caller's value with `meedya-lang`'s LANG-002 reader first — ID3v2 (no full-tag field) gets the ISO 639-2 terminology code or `und`; every other format gets the canonical BCP 47 tag or the original text, unchanged, when unrecognised. `read_tags` is unchanged (raw text); a reader MUST pass it through `meedya_lang::from_legacy_three_letter` before treating it as a language.
- **`template`** (#47) — filename template engine (`Template`, `TemplateError`, `TagSource` trait), root re-exported. `{name}` placeholders, `|`-piped transforms (sanitize/ascii/lower/upper/title/trim/round/fallback:VAR/max:N), `:NN` width specifiers.
- **`identifier_types`** (#65) — cross-repo identifier-type registry loaded from [identifier_types.toml](../crates/meedya-metadata/identifier_types.toml) (scope→slug→validation vocabulary; DATA, not an enum). `IdentifierType`/`IdentifierScope`/`IdentifierStatus`/`IdentifierValidation`; `identifier_types()`/`identifier_type()`/`active_identifier_slugs()`; raw artifact re-exported as `IDENTIFIER_TYPES_TOML`. Guard-held: `crates/meedya-metadata/tests/identifier_registry_guard.rs` declares the expected active/reserved slug sets and fails CI if the artifact drifts from that declaration or from `CommonTag::identifier_slug()`.
- **`mp4ameta`-backed (sandbox-safe)**: `registry` (TAG_REGISTRY static loaded from [tags.toml](../crates/meedya-metadata/tags.toml)), `writer` (`write_tags_from_registry`, `write_local_tags`, `extract_isrc_from_vendor`), `codec_tags` (CodecKind enum + per-codec writers), `playback_bounds` (`set_playback_start/stop`, `get_playback_*_ms`, `clear_*`).

**Adding a new tag**: edit `tags.toml`, zero Rust changes (PROMPTS.md has the template). **Adding a new identifier type**: edit `identifier_types.toml` + update the expected-slug guard, zero other Rust changes.

### meedya-tags-extended

- [src/io.rs](../crates/meedya-tags-extended/src/io.rs) — `TagFile`: lofty-based open/edit/save with foreign-frame pass-through.
- [src/model.rs](../crates/meedya-tags-extended/src/model.rs) — `ExtendedTags`, `Source` enum, `CuePoint`, `LoopPoint`, `BeatGrid`, `Rgb`, `MusicalKey` (Camelot/Open Key/traditional round-tripping).
- [src/standard.rs](../crates/meedya-tags-extended/src/standard.rs) — BPM/key/comment read+write across all lofty-supported formats.
- [src/mik.rs](../crates/meedya-tags-extended/src/mik.rs) — Mixed In Key reader. `read_mik(tag) -> MikAnalysis` scans every documented MIK write location (standard fields, artist/title prefixes+suffixes, comment, grouping, label) and recovers key/energy/tempo. `normalise_to_standards(tag, &analysis)` writes the canonical values to standard tag fields (only Energy falls back to `MeedyaMeta:Energy` because no standard exists). Source fields are read-only — user data preserved.

**Pending** (one session each, fixture-driven): Serato (Markers2/Autotags/BeatGrid), Rekordbox (ID3 PRIV + XML sidecar), Traktor (cue frames + collection.nml), Virtual DJ (.vdj sidecar + embedded markers).

### meedya-library-import

- [src/itunes_xml.rs](../crates/meedya-library-import/src/itunes_xml.rs) — iTunes / Music.app XML parser; cross-platform `file://` URL decoding.
- [src/cuesheet.rs](../crates/meedya-library-import/src/cuesheet.rs) — Full CUE parser at CD-frame precision; rich `CueSheet { catalog, performer, title, rems, files }` model. `import()` adapter emits LibraryEntries only for narrow trim cases.

`LibraryEntry { locator: Path|PersistentId, start_ms, stop_ms }` is the normalized output. Filesystem matching is the consuming app's job.

### meedya-lyrics

- [src/provider/](../crates/meedya-lyrics/src/provider/) — `LyricsProvider` trait + `LrclibProvider`.
- [src/lrc.rs](../crates/meedya-lyrics/src/lrc.rs) — LRC parser/writer (`[mm:ss.xx]`).
- [src/sidecar.rs](../crates/meedya-lyrics/src/sidecar.rs) — `.lrc` sidecar writes.
- [src/embed.rs](../crates/meedya-lyrics/src/embed.rs) — Two embed paths: `embed()` writes plain text via `meedya-metadata::CommonTag::Lyrics` (USLT/©lyr/LYRICS); `embed_synced()` writes ID3v2 SYLT frames (errors on non-ID3v2 containers). UTF-16 BOM, MS timestamp format, lyrics content type. `DEFAULT_LANGUAGE` is `*b"XXX"` (ID3's "language not known" marker, policy MWBM-MEDIA-LANG's LANG-003) — it used to be `*b"eng"`, silently guessing English. `id3_language(value)` turns a real BCP 47 tag or old three-letter code into the right bytes via `meedya-lang`'s LANG-002 reader.
- [src/lyricsfile_ttml.rs](../crates/meedya-lyrics/src/lyricsfile_ttml.rs) — `xml:lang`/`lang` on `<tt>` is read through `meedya_lang::from_legacy_three_letter` (LANG-002): a recognised value is stored canonicalised (`EN-gb` → `en-GB`), an unrecognised one is kept exactly as found (never replaced with `und` or a guess), and an absent attribute stays absent.

### meedya-providers

Provider framework. Re-exports: `MetadataProvider`, `ProviderCapabilities`, `ProviderError`, `SearchQuery`, `ProviderResult`, `MediaType`, `CoverArtInfo`, `CoverArtSize`, `CredentialStore`, `CredentialSource`, `ResolvedCredential`, `MatchScorer`, `ScoringWeights`, `ProviderRateLimiter`, `RateLimiterRegistry`, `escape_lucene`, `quote_phrase`, `phrase_clause`. Modules: `traits`, `types`, `cover_art`, `credentials`, `match_scoring`, `rate_limiter`, `lucene` (Lucene/Solr query escaping — always compiled, no feature gate), `providers` (feature-gated concrete `MetadataProvider` impls: `musicbrainz`, `isrc`, `iswc`, `spotify`, `apple_music`, `deezer`, `tmdb`, `thetvdb`, `omdb`, `apple_tv`, `itunes_store`, `apple_podcasts`, `eidr`).

**MusicBrainz Solr 9→10 search hardening (2026-09-01)**: audited the announced breaking tickets (SEARCH-444/642/666/752/764) against `musicbrainz`/`isrc`/`iswc` — none hit us (we never search `area`/`url`/`cdstub`/`tag`, never read relationship `target` or release `quality`, and all response parsers are serde-derive structs that ignore unknown fields). The real risk was our own unescaped Lucene query construction under the stricter Solr 10 parser; `lucene::{escape_lucene, quote_phrase, phrase_clause}` now hardens every user-supplied value going into a query, `MusicBrainzProvider::build_lucene_query` replaced the old dead `search_term` fallback, and ISRC/ISWC queries are normalised before being embedded.

The three `lucene` helpers implement **two different escaping regimes and are not interchangeable**: inside a quoted phrase only `\` and `"` are structurally significant (escaping the full special set there would embed literal backslashes into the phrase and kill the match), while a bare term needs the full 19-character set escaped. Both prior branches had conflated these.

**Identifier query forms were settled by live probing**, not documentation — MusicBrainz documents neither: ISRC matches compact (`isrc:GBAYE0601498`) and misses hyphenated; ISWC matches only in the stored dotted display form (`iswc:"T-304.031.869-8"`), while the compact form returns 0 results and the hyphen-only form is a parse error. Hence the deliberate asymmetry between `normalise_isrc` and `format_iswc_dotted`. Forward-compat parse fixtures (Solr-10-shaped response JSON with `relations`/`quality`/`release-group`/`genres` noise) prove the parsers are unaffected. Genre search (SEARCH-681) deferred — see [HISTORY.md](HISTORY.md).

### meedya-fingerprint

- `acoustid` — `AcoustIdClient`, `AcoustIdResult`. Rate-limited AcoustID lookup client.
- `chromaprint` (feature-gated, `default = []`) — pure-Rust fingerprint generation (`generate_fingerprint`), no fpcalc binary. Opt-in via the `chromaprint` Cargo feature (pulls `rusty-chromaprint` + `symphonia` + `base64`); `meedya-core` does not forward this feature.
- `replaygain` — `ReplayGainAnalyzer`, `ReplayGainResult`, `AlbumGainResult`, `DEFAULT_REFERENCE_LEVEL` (-18 LUFS), `DEFAULT_ANALYSIS_TIMEOUT` (600s).

### meedya-lang

Shared implementation of policy **MWBM-MEDIA-LANG** — normative rules in
[`docs/standards/media-language-bcp47-policy.md`](../docs/standards/media-language-bcp47-policy.md).
Embeds a byte-for-byte copy of `docs/standards/data/bcp47-language-data-v1.json` (a unit test
enforces this — refresh with `cp docs/standards/data/bcp47-language-data-v1.json
crates/meedya-lang/data/`). Eight modules, kept apart on purpose (the policy's section 9): `tag`
(`canonicalise`, `from_legacy_three_letter`, `from_posix_locale`, `iso639_2_code`/
`iso639_2_write`, `LanguageTag`, `TagKind`, `TagNote`); `canonical` (Part A's stored-order
comparator, `sort_canonical`, `LanguageItem`); `roles` (`TrackType`, `Role`, `role_rank`);
`tracks` (the role-aware variant, `sort_tracks`, `TrackItem`); `presentation` (Part B's
menu-order comparator — a genuinely different algorithm from Part A, never the same
comparison function — `sort_for_presentation`, `subtitle_menu`, `label`, `PresentationItem`);
`matching` (`match_tags`, `MatchLevel`, `TagMatch`); `select` (`select_audio`/`select_subtitle`,
`SelectableTrack`, `SubtitleMode`); `sidecar` (`build_sidecar_name`/`parse_sidecar_name`,
added in the policy's first revision, TEXT-030). Zero dependencies beyond `serde` +
`serde_json` (needed only to parse the embedded data); no I/O, no network. Conformance-tested
in `tests/conformance.rs` against `tests/fixtures/bcp47-language-policy-v1.json` — every case
in every section, collecting all failures before asserting, plus a stability check (every
`canonicalise` case's non-null answer must canonicalise to itself unchanged) and a section-skip
guard (the number of cases run must equal the number in the file). The same fixture file is
what the PHP implementation (`bindings/php/media-language/`) runs against, so the two can never
quietly disagree.

### meedya-db

`MeedyaDbClient` (api.meedya.tv/v1), `DbExporter` trait, `MediaRecord`/`Track`/`Album`/`Artist` models.

### meedya-core

Facade with feature flags (`metadata` / `codecs` / `fingerprint` / `lyrics` / `providers` / `tags-extended` / `library-import` / `db` / `keyring` / `full`). All implemented crates re-exported as top-level modules. `meedya_core::prelude` re-exports common types: `CommonTag`, `IdentifierType`, `MetadataError`, `TagRegistry`, `AudioCodec`, `ChannelConfig`, `CodecRegistry`, `ContainerFormat`, `SpatialType`, `MetadataProvider`, `ProviderCapabilities`, `CredentialStore`, `ProviderRateLimiter`, `ProviderResult`, `SearchQuery`, `Lyrics`, `LyricsProvider`, `SyncedLine`, `TrackQuery`, `TagFile`, `ExtendedTags`, `MusicalKey`, `KeyMode`, `Note`, `CuePoint`, `LoopPoint`, `BeatGrid`, `Source`, `LibraryEntry`, `EntryLocator`, `ImportReport`, `SourceInfo`.

## Key design decisions

0. **Standards-first** (project-wide policy). Use standard metadata tags (ID3v2 / Vorbis / MP4 ilst spec fields) wherever they exist. Fall back to `MeedyaMeta:*` freeform atoms only when no standard equivalent exists — e.g., DJ energy ratings, playback bounds, audit trails. See [CLAUDE.md → Key design principles](CLAUDE.md#key-design-principles).
1. **Two tag-I/O foundations coexist.** `mp4ameta` for sandbox-safe Apple Music flow; `lofty` for multi-format DJ-metadata and general pass-through. Not unified — they serve different code paths.

2. **Pass-through preservation.** `meedya-tags-extended::TagFile` round-trips unknown frames automatically (lofty design). MeedyaConverter re-encodes don't strip Serato/Rekordbox/Traktor blobs even when we don't model them.

3. **Config-driven where possible.** [tags.toml](../crates/meedya-metadata/tags.toml) declarative; no Rust changes to add a tag.

4. **Library importers don't match files.** Normalized records with `EntryLocator::{ Path | PersistentId }`; consuming apps handle filesystem resolution.

5. **MeedyaMeta atom namespace** is for MeedyaSuite-only fields without standard equivalents (playback bounds, custom cue points). `com.apple.iTunes` namespace is used when the field has player compatibility precedent.

6. **Results only, not side effects.** Crates return data; consumers handle I/O. `meedya-fingerprint` exemplifies this — it produces `AcoustIdResult` / `ReplayGainResult`, and `meedya-metadata::tag_io::write_acoustid_tags` / `write_replaygain_tags` handles file writes.

7. **Fixture-based testing for proprietary parsers.** Won't write Serato/etc parsers from memory — every format needs validation against real DJ-tagged sample files. See [PROMPTS.md → Implementing a proprietary DJ reader](PROMPTS.md#implementing-a-proprietary-dj-reader).

8. **Identifier vocabulary is data, not an enum** (#65). `identifier_types.toml` is the cross-repo scope→slug→validation registry; adding an identifier type is a TOML edit plus one line in the guard test's expected-slug declaration — never a new Rust type. `CommonTag` is `#[non_exhaustive]` — new variants are reserved for tags with a genuine per-container frame mapping (ID3v2/Vorbis/MP4 ilst); a bare external identifier with no container frame belongs in the registry instead.

## Build / test

```bash
cargo build --workspace          # all 11 crates
cargo test  --workspace          # 853 tests
cargo test  --workspace --all-features   # 1000 tests (the CI configuration)
cargo test  -p meedya-metadata   # single crate
cargo doc   --workspace --no-deps --open  # exhaustive auto-generated reference
```

Workspace uses Rust edition 2021, MIT license, copyright header `// Copyright (c) 2026 MeedyaSuite` on every source file.

## Cross-repo coordination

Each downstream app (MeedyaConverter, MeedyaManager, MeedyaDL) has or will have a `claude/core-integration` branch where it adopts this workspace as a dependency. Pre-drafted GitHub issues per app are in [`docs/cross-repo-issues.md`](../docs/cross-repo-issues.md). See auto-memory `project_core_integration.md` for the integration kickoff context.

## What NOT to assume

- Don't conflate `meedya-metadata` (mp4ameta + lofty surfaces) with `meedya-tags-extended` (lofty-only DJ-aware reader/writer). They're separate by design.
- Don't push proprietary DJ reader implementations into `meedya-tags-extended` from memory — every Serato/Rekordbox/Traktor format needs validation against real fixture files.
- Don't add features beyond what the task requires. Trait abstractions, optional fields, unused error variants — drop them.
- Don't update `docs/API.md` as a follow-up commit when the public API changes — partner apps consume it as the integration reference; stale spec produces silent integration bugs. Update it in the same commit as the code change.
