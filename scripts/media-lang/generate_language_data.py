#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# scripts/media-lang/generate_language_data.py
#
# Builds docs/standards/data/bcp47-language-data-v1.json — the reference
# data every implementation of the MWBM-MEDIA-LANG policy reads (the Rust
# crate `meedya-lang`, the PHP copy under bindings/php/media-language/, and
# the Swift copy in MeedyaConverter).
#
# WHY A GENERATED FILE AND NOT THE RAW SOURCES
# --------------------------------------------
# Three languages (Rust, PHP, Swift) have to agree, byte for byte, on which
# old codes are replaced by which new ones. If each implementation parsed
# the IANA registry itself, three parsers would each get the continuation
# lines, ranges and odd records slightly differently, and the conformance
# tests would catch the disagreement only for the handful of codes they
# happen to mention. One small JSON file that all three read removes that
# whole class of disagreement.
#
# SOURCES (both public, both downloaded by hand and passed in as files)
# ----------------------------------------------------------------------
# 1. The IANA Language Subtag Registry (RFC 5646's registry):
#    https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry
#    Gives every registered subtag, which ones are deprecated, and what
#    replaces them ("Preferred-Value").
# 2. The SIL ISO 639-3 code table:
#    https://iso639-3.sil.org/sites/iso639-3/files/downloads/iso-639-3.tab
#    Gives, for each three-letter code, its ISO 639-2 "B" and "T" forms and
#    its two-letter ISO 639-1 form. Containers such as MP4 and older MKV
#    store three-letter codes ("eng", "ger"); BCP 47 requires the shortest
#    code ("en", "de"), so this table is how old container values are read.
#    The Library of Congress publishes the same mapping, but its site refuses
#    automated downloads (HTTP 403, checked 28 Sept 2026); SIL is the
#    official registration authority for ISO 639-3 and carries the same
#    Part 2B / Part 2T / Part 1 columns.
#
# The IANA URL always serves the newest registry, and old versions are not
# kept there, so this script does not download anything: regenerating the
# data is a deliberate act (a data-version bump), not something a build does.
#
# WHAT IT CANNOT DO
# -----------------
# ISO 639-2 also has about sixty "collective" codes (for example "afa",
# Afro-Asiatic languages) that ISO 639-3 does not list. They are still
# registered in the IANA registry as language subtags, so they are covered
# by rule 2 in build_iso639_2_map(); nothing is lost. A three-letter code
# that is in neither source is left out of the map on purpose — callers must
# report it as unrecognised rather than guess (policy rule LANG-002).
#
# Usage:
#   python3 scripts/media-lang/generate_language_data.py \
#       --registry path/to/language-subtag-registry \
#       --iso639-3 path/to/iso-639-3.tab \
#       --retrieved 2026-09-28 \
#       --out docs/standards/data/bcp47-language-data-v1.json

import argparse
import hashlib
import json
import sys

POLICY_ID = "MWBM-MEDIA-LANG"
POLICY_VERSION = "1.0.0"
# The data file is versioned on its own, because a registry refresh changes
# the data without changing any rule. The policy document says which data
# versions a given policy version accepts.
DATA_VERSION = "1.0.0"

# ISO 639-2 codes that were WITHDRAWN, so neither source above lists them
# any more, but which old media files still carry (an MKV muxed before 2008
# can say "scc" for Serbian). Each maps to the code that replaced it, per the
# ISO 639-2 Registration Authority's published change history
# (https://www.loc.gov/standards/iso639-2/php/code_changes.php):
#   scc -> srp (sr)  and  scr -> hrv (hr): the B codes for Serbian and
#          Croatian, withdrawn 2008-06-28 in favour of the single B/T codes.
#   mol -> rum/ron (ro): Moldavian, withdrawn 2008-11-03.
#   jaw -> jav (jv): Javanese, changed 2001-01-03.
# Kept deliberately short: only codes that were once valid ISO 639-2 and
# have a single clear successor. Anything else stays unrecognised.
WITHDRAWN_ISO639_2 = {"scc": "sr", "scr": "hr", "mol": "ro", "jaw": "jv"}

REGISTRY_URL ="https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry"
SIL_URL = "https://iso639-3.sil.org/sites/iso639-3/files/downloads/iso-639-3.tab"


def sha256_of(path):
    """Checksum of a source file, recorded so a reader can tell exactly
    which download the data was built from."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_registry(path):
    """Read the IANA registry into a list of records (dict of field -> list
    of values). Records are separated by '%%'. A line starting with two
    spaces continues the previous field (the registry wraps long
    Description and Comments lines that way). Returns (file_date, records)."""
    records, current, last_field = [], {}, None
    file_date = None
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if line == "%%":
                if current:
                    records.append(current)
                current, last_field = {}, None
                continue
            if line.startswith("  ") and last_field:
                current[last_field][-1] += " " + line.strip()
                continue
            if ":" not in line:
                continue
            field, value = line.split(":", 1)
            field, value = field.strip(), value.strip()
            if field == "File-Date":
                file_date = value
                continue
            current.setdefault(field, []).append(value)
            last_field = field
    if current:
        records.append(current)
    if not file_date:
        sys.exit("registry has no File-Date line — is this the right file?")
    return file_date, records


def first(record, field):
    values = record.get(field)
    return values[0] if values else None


def build_registry_sections(records):
    """Turn registry records into the lookup sections the policy needs."""
    languages, scripts, regions, variants = set(), set(), set(), set()
    language_ranges, script_ranges, region_ranges = [], [], []
    extlangs = {}
    preferred = {"language": {}, "extlang": {}, "script": {}, "region": {}, "variant": {}}
    deprecated_no_replacement = {"language": [], "script": [], "region": [], "variant": []}
    grandfathered, redundant_preferred = {}, {}

    for r in records:
        kind = first(r, "Type")
        if kind in ("grandfathered", "redundant"):
            tag = first(r, "Tag")
            pv = first(r, "Preferred-Value")
            if kind == "grandfathered":
                # Every grandfathered tag is recorded, with or without a
                # replacement: the parser needs the full list, because most
                # of them ("i-default", "en-GB-oed") do not fit the normal
                # tag grammar and would otherwise be rejected as malformed.
                # Key is lower case (matching is case-insensitive); the value
                # keeps the registry's own casing, which is canonical.
                grandfathered[tag.lower()] = {"tag": tag, "preferred": pv}
            elif pv:
                # Redundant tags without a replacement need no entry: they
                # already fit the grammar and are kept exactly as written.
                redundant_preferred[tag.lower()] = pv
            continue

        subtag = first(r, "Subtag")
        if not subtag:
            continue
        pv = first(r, "Preferred-Value")
        deprecated = "Deprecated" in r

        if ".." in subtag:
            lo, hi = subtag.split("..")
            {"language": language_ranges, "script": script_ranges,
             "region": region_ranges}[kind].append([lo, hi])
            continue

        if kind == "language":
            languages.add(subtag)
            if pv:
                preferred["language"][subtag] = pv
            elif deprecated:
                deprecated_no_replacement["language"].append(subtag)
        elif kind == "extlang":
            # An extlang ("zh-yue") is always replaced in canonical form by
            # its Preferred-Value, which the registry sets to the extlang
            # itself ("yue"). The prefix list is kept so a reader can check
            # the extlang was used after the right language.
            extlangs[subtag] = r.get("Prefix", [])
            if pv:
                preferred["extlang"][subtag] = pv
        elif kind == "script":
            scripts.add(subtag)
            if pv:
                preferred["script"][subtag] = pv
            elif deprecated:
                deprecated_no_replacement["script"].append(subtag)
        elif kind == "region":
            regions.add(subtag)
            if pv:
                preferred["region"][subtag] = pv
            elif deprecated:
                # For example "CS" and "YU": the country split, so there is
                # no single replacement. The policy keeps such a tag as
                # written and reports it, rather than guessing a successor.
                deprecated_no_replacement["region"].append(subtag)
        elif kind == "variant":
            variants.add(subtag)
            if pv:
                preferred["variant"][subtag] = pv
            elif deprecated:
                deprecated_no_replacement["variant"].append(subtag)

    for key in deprecated_no_replacement:
        deprecated_no_replacement[key].sort()
    return {
        "languages": sorted(languages),
        "language_ranges": language_ranges,
        "extlangs": dict(sorted(extlangs.items())),
        "scripts": sorted(scripts),
        "script_ranges": script_ranges,
        "regions": sorted(regions),
        "region_ranges": region_ranges,
        "variants": sorted(variants),
        "preferred": {k: dict(sorted(v.items())) for k, v in preferred.items()},
        "deprecated_without_replacement": deprecated_no_replacement,
        "grandfathered": dict(sorted(grandfathered.items())),
        "redundant_preferred": dict(sorted(redundant_preferred.items())),
    }


def build_iso639_2_writing_table(sil_path, collectives):
    """For WRITING an old three-letter field: map each BCP 47 language
    subtag that has an ISO 639-2 code to both forms of that code.

    Formats disagree on which form they want. Matroska's old `Language`
    element uses the bibliographic (B) form — "ger", "fre", "chi" — per
    RFC 9559 section 12. MP4/MOV's media header uses the terminology (T)
    form — "deu", "fra", "zho" — per ISO/IEC 14496-12. For most languages
    the two are the same; for twenty they differ. Storing both means no
    implementation ever has to guess which one a format wants.
    """
    table = {}
    with open(sil_path, encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        col = {name: i for i, name in enumerate(header)}
        for line in f:
            cells = line.rstrip("\n").split("\t")
            part2b, part2t, part1 = cells[col["Part2b"]], cells[col["Part2t"]], cells[col["Part1"]]
            if not (part2b or part2t):
                continue
            subtag = part1 or part2t or part2b
            table[subtag] = {"b": part2b or part2t, "t": part2t or part2b}
    for code in collectives:
        table.setdefault(code, {"b": code, "t": code})
    return dict(sorted(table.items()))


def build_iso639_2_map(sil_path, registry_preferred):
    """Map every ISO 639-2 three-letter code in the SIL table (both B and T
    forms) to the BCP 47 language subtag that means the same language.

    Rule 1 (here) — the code is in the SIL table as Part 2B or Part 2T: use
             its two-letter Part 1 code when there is one ("ger" and "deu"
             -> "de"), otherwise its Part 2T code, which is then itself the
             registered subtag.
    Rule 2 (in main(), via iso639_2_collectives_and_specials) — the ISO
             639-2 codes the SIL table does not list (collections such as
             "afa", and the special codes "mis", "mul", "und", "zxx") are
             registered IANA language subtags and map to themselves.
    A deprecated result is replaced by its registry Preferred-Value, so the
    map always yields a canonical subtag.
    """
    mapping = {}
    with open(sil_path, encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        col = {name: i for i, name in enumerate(header)}
        for needed in ("Id", "Part2b", "Part2t", "Part1"):
            if needed not in col:
                sys.exit(f"SIL table has no {needed} column — format changed?")
        for line in f:
            cells = line.rstrip("\n").split("\t")
            part2b, part2t, part1 = cells[col["Part2b"]], cells[col["Part2t"]], cells[col["Part1"]]
            if not (part2b or part2t):
                continue
            target = part1 or part2t or part2b
            for code in (part2b, part2t):
                if code:
                    mapping[code] = target
    for code, target in list(mapping.items()):
        mapping[code] = registry_preferred.get(target, target)
    return dict(sorted(mapping.items()))


def iso639_2_collectives_and_specials(records):
    """The registered three-letter subtags that ISO 639-2 defines but ISO
    639-3 does not: collections ("afa") and the four special codes.

    Only these are added, deliberately not every three-letter subtag in the
    registry: the registry also holds all ~7,800 ISO 639-3 codes, which are
    not ISO 639-2 codes, and admitting them would let a typo in an old
    container field ("enn") pass as a valid legacy code. The registry marks
    exactly the ISO 639-2 collections with Scope: collection and the four
    specials with Scope: special."""
    out = []
    for r in records:
        if first(r, "Type") != "language":
            continue
        subtag = first(r, "Subtag")
        scope = first(r, "Scope")
        if subtag and len(subtag) == 3 and ".." not in subtag and scope in ("collection", "special"):
            out.append(subtag)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--registry", required=True)
    ap.add_argument("--iso639-3", required=True, dest="iso639_3")
    ap.add_argument("--retrieved", required=True, help="date the SIL table was downloaded, YYYY-MM-DD")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    file_date, records = parse_registry(args.registry)
    sections = build_registry_sections(records)
    iso = build_iso639_2_map(args.iso639_3, sections["preferred"]["language"])
    for code in iso639_2_collectives_and_specials(records):
        iso.setdefault(code, sections["preferred"]["language"].get(code, code))
    for code, target in WITHDRAWN_ISO639_2.items():
        if code in iso:
            sys.exit(f"{code} is listed as withdrawn but a source still defines it — review the table")
        iso[code] = target
    iso = dict(sorted(iso.items()))
    writing = build_iso639_2_writing_table(
        args.iso639_3, iso639_2_collectives_and_specials(records))

    data = {
        "$schema": "./bcp47-language-data-v1.schema.json",
        "policy": POLICY_ID,
        "policy_version": POLICY_VERSION,
        "data_version": DATA_VERSION,
        "sources": {
            "iana_language_subtag_registry": {
                "url": REGISTRY_URL,
                "file_date": file_date,
                "sha256": sha256_of(args.registry),
            },
            "sil_iso_639_3": {
                "url": SIL_URL,
                "retrieved": args.retrieved,
                "sha256": sha256_of(args.iso639_3),
            },
        },
        "special_languages": {
            "mul": "multiple languages",
            "mis": "uncoded language",
            "und": "undetermined language",
            "zxx": "no linguistic content",
        },
        **sections,
        "iso639_2": iso,
        "iso639_2_for_language": writing,
    }
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        # Compact separators keep the file small enough to embed in an app;
        # sort_keys keeps regeneration byte-stable, so a registry refresh
        # shows up as a readable diff of what actually changed.
        json.dump(data, f, ensure_ascii=False, sort_keys=True, indent=1)
        f.write("\n")
    print(f"wrote {args.out}: registry {file_date}, "
          f"{len(sections['languages'])} languages, {len(iso)} ISO 639-2 codes")


if __name__ == "__main__":
    main()
