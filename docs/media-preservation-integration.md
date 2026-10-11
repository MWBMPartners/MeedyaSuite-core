# MeedyaSuite-core: media preservation integration requirements

Status: proposed development/discovery requirements, 2026-10-11. No implemented support is claimed and existing milestones remain unchanged.

Use the [common media preservation and presentation contract](https://github.com/MeedyaSuite/MeedyaSuite-specs/blob/docs/cross-app-media-contract/standards/CROSS-APP-MEDIA-CONTRACT.md) alongside the [Converter research notes](https://github.com/MWBMPartners/MeedyaConverter/blob/docs/mp4-remux-preservation/docs/MP4-Remux-Preservation-Plan.md). These links reference draft working branches. Adopt a reviewed, commit-pinned contract before implementation/conformance claims; app-specific deviations need an explicit decision and capability report.

## Shared-model investigation

Reuse the existing codec, metadata, fingerprint and import surfaces after auditing their actual public APIs; do not claim they already implement this contract. Keep app UI/playback choices out of core, respect feature-gated heavy dependencies and intentional separate tag-I/O surfaces.

Proposed reusable contracts include stable source/presentation/track/service IDs; rational timeline transforms and priming/preroll/edits; native/translated/archival flags; stereo/multiview/HDR evidence; original plus normalized metadata; native 608/708 carriage and service identity; typed relationship edges; capability/evidence coverage; duplicate comparison and ranking policy; and versioned handoff/job reports. Each model needs unknown/unverified states and provenance rather than a blanket supported boolean.

Metadata mappings should prefer standard fields, then clearly namespaced custom fields where needed. Exact Kodi XML bytes/digests and unknown data coexist with searchable normalized fields. Custom relationships must not masquerade as native container references. Preserve multiple caption standards and services without a text conversion.

Fingerprint similarity is candidate evidence only. Full encoded/decoded comparisons need configuration and coverage; check timeline/role/relationship equivalence before opt-in consolidation. A higher nominal bitrate/sample rate is not a universal quality ranking.

Public API proposals need consumer and licensing review, versioned serialization, source identity validation and migrations. Update API docs and bindings together only when an actual API changes; these notes add no new crate, dependency, FFI or implementation.

## Discovery and acceptance

Define schemas/fixtures collaboratively with Player/Subtitler/Converter before building adapters. Exercise unknown-field retention, native caption services, rational timelines, reference remapping, conflicting metadata, caption exclusion policy, stale cache signatures and old handoff versions. Recognition, preservation, decode/render and playback evidence remain separate. Keep test/media-processing implementation in this repository, specifications in the central specs repository.

## Planning integration

Carry these cases into future implementation issues and regression fixtures. Inventory/preview, safe edits and evidence contracts come first; edition-aware synchronization and incremental edits are bounded investigations. Do not infer support from a successful mux, codec label, progress message or an archived tag. Structural/sample checks, exhaustive preservation and target playback are different verification levels.
