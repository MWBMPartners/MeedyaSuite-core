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
# 2. The ISO 639-2 code list as published by Debian's iso-codes project:
#    https://salsa.debian.org/iso-codes-team/iso-codes/-/raw/main/data/iso_639-2.json
#    iso-codes copies the list from the ISO 639-2 Registration Authority
#    (the Library of Congress) and gives, for each code, its terminology
#    ("T") form, its bibliographic ("B") form where different, and its
#    two-letter ISO 639-1 form. Containers such as MP4 and older MKV store
#    these three-letter codes ("eng", "ger"); BCP 47 requires the shortest
#    code ("en", "de"), so this list is how old container values are read
#    and how the old three-letter fields are written.
#
#    Why not the Library of Congress directly: its site refuses automated
#    downloads (HTTP 403, checked 28 Sept 2026).
#    Why not the SIL ISO 639-3 table (the first version of this script used
#    it): SIL lists individual languages and macrolanguages only, so the
#    ISO 639-2 *group* codes ("afa", Afro-Asiatic languages) had to be
#    guessed from the IANA registry's "Scope: collection" records — and that
#    guess was wrong. The registry also holds 50 group codes that exist only
#    in ISO 639-5 (added 2009-07-29, e.g. "alv"), which are NOT ISO 639-2
#    codes and must never be written into an MP4 or Matroska three-letter
#    field. The first version included them; an independent review caught
#    it (28 Sept 2026). The Debian list is the actual ISO 639-2 list, so no
#    guess is needed.
#
# The IANA URL always serves the newest registry, and old versions are not
# kept there, so this script does not download anything: regenerating the
# data is a deliberate act (a data-version bump — see the policy's
# "Changing this policy"), not something a build does. The sources section
# of the output records each file's date or commit and checksum.
#
# WHAT IT CANNOT DO
# -----------------
# It knows only what the two sources say. The four withdrawn codes below are
# the only hand-written data. A three-letter code in neither source is left
# out on purpose: callers report it as unrecognised rather than guess
# (policy rule LANG-002).
#
# Usage:
#   python3 scripts/media-lang/generate_language_data.py \
#       --registry path/to/language-subtag-registry \
#       --iso639-2 path/to/iso_639-2.json \
#       --iso639-2-commit <iso-codes git commit the file came from> \
#       --out docs/standards/data/bcp47-language-data-v1.json

import argparse
import hashlib
import json
import re
import sys

POLICY_ID = "MWBM-MEDIA-LANG"
POLICY_VERSION = "1.0.0"
# The data file is versioned on its own, because a registry refresh changes
# the data without changing any rule (see the policy's "Changing this
# policy" for what a data change requires).
DATA_VERSION = "1.0.0"

# ISO 639-2 codes that were WITHDRAWN, so the current list no longer has
# them, but which old media files still carry (an MKV muxed before 2008 can
# say "scc" for Serbian). Each maps to the code that replaced it, per the
# ISO 639-2 Registration Authority's published change history
# (https://www.loc.gov/standards/iso639-2/php/code_changes.php — that page
# could not be fetched automatically; these four are from the author's
# reading of it and are not independently checked by any test):
#   scc -> srp (sr)  and  scr -> hrv (hr): the B codes for Serbian and
#          Croatian, withdrawn in 2008 in favour of single B/T codes.
#   mol -> rum/ron (ro): Moldavian, withdrawn in 2008.
#   jaw -> jav (jv): Javanese, changed in 2001.
# Kept deliberately short: only codes that were once valid ISO 639-2 and
# have a single clear successor. Anything else stays unrecognised. These
# are for READING only; they never appear in the writing table.
WITHDRAWN_ISO639_2 = {"scc": "sr", "scr": "hr", "mol": "ro", "jaw": "jv"}

REGISTRY_URL = "https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry"
ISO639_2_URL = "https://salsa.debian.org/iso-codes-team/iso-codes/-/raw/main/data/iso_639-2.json"

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



def read_iso639_2(path):
    """Read Debian iso-codes' ISO 639-2 list into (codes, local_use_range).

    Each code is a dict {"t": terminology code, "b": bibliographic code
    (same as t for all but twenty), "one": ISO 639-1 two-letter code or
    None}. The local-use range ("qaa-qtz") is returned separately: it is a
    range, not a code, and implementations test it by comparison."""
    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    rows = doc.get("639-2")
    if not isinstance(rows, list) or not rows:
        sys.exit("ISO 639-2 file has no '639-2' list — format changed?")
    codes, local_range = [], None
    for row in rows:
        t = row.get("alpha_3", "")
        if re.fullmatch(r"[a-z]{3}-[a-z]{3}", t):
            local_range = t.split("-")
            continue
        if not re.fullmatch(r"[a-z]{3}", t):
            sys.exit(f"unexpected ISO 639-2 code {t!r}")
        b = row.get("bibliographic", t)
        one = row.get("alpha_2")
        codes.append({"t": t, "b": b, "one": one})
    if local_range != ["qaa", "qtz"]:
        sys.exit(f"expected the local-use range qaa-qtz, found {local_range}")
    return codes, local_range


def build_iso639_2_tables(codes, registry_languages, registry_preferred):
    """Build the reading table (every B and T code -> BCP 47 subtag) and the
    writing table (BCP 47 subtag -> {"b", "t"}).

    The BCP 47 subtag for an ISO 639-2 code is its ISO 639-1 code when it
    has one, otherwise the T code itself (BCP 47 uses the shortest code).
    A deprecated result is replaced by its registry Preferred-Value, so the
    reading table always yields a canonical subtag. Every result must be a
    registered language subtag, or the sources disagree and the run stops.
    """
    reading, writing = {}, {}
    registered = set(registry_languages)
    for code in codes:
        subtag = code["one"] or code["t"]
        subtag = registry_preferred.get(subtag, subtag)
        if subtag not in registered:
            sys.exit(f"ISO 639-2 {code['t']} maps to {subtag}, which the IANA registry does not list")
        reading[code["t"]] = subtag
        reading[code["b"]] = subtag
        if subtag in writing and writing[subtag] != {"b": code["b"], "t": code["t"]}:
            sys.exit(f"two ISO 639-2 codes map to {subtag}: {writing[subtag]} and {code}")
        writing[subtag] = {"b": code["b"], "t": code["t"]}
    for code, target in WITHDRAWN_ISO639_2.items():
        if code in reading:
            sys.exit(f"{code} is listed as withdrawn but the current list still defines it — review the table")
        reading[code] = target
    return dict(sorted(reading.items())), dict(sorted(writing.items()))


def main():
    ap = argparse.ArgumentParser(description="Generate the MWBM-MEDIA-LANG reference data.")
    ap.add_argument("--registry", required=True)
    ap.add_argument("--iso639-2", required=True, dest="iso639_2")
    ap.add_argument("--iso639-2-commit", required=True, dest="iso639_2_commit",
                    help="the iso-codes git commit the ISO 639-2 file was taken from")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.iso639_2_commit):
        sys.exit("--iso639-2-commit must be a full 40-character commit")

    file_date, records = parse_registry(args.registry)
    sections = build_registry_sections(records)
    codes, local_range = read_iso639_2(args.iso639_2)
    reading, writing = build_iso639_2_tables(
        codes, sections["languages"], sections["preferred"]["language"])

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
            "iso_639_2": {
                "url": ISO639_2_URL,
                "iso_codes_commit": args.iso639_2_commit,
                "sha256": sha256_of(args.iso639_2),
            },
        },
        "special_languages": {
            "mul": "multiple languages",
            "mis": "uncoded language",
            "und": "undetermined language",
            "zxx": "no linguistic content",
        },
        **sections,
        "iso639_2": reading,
        "iso639_2_for_language": writing,
        "iso639_2_local_use": local_range,
    }
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        # indent=1 keeps the file readable in a diff while staying small
        # enough to embed; sort_keys keeps regeneration byte-stable, so a
        # registry refresh shows up as a readable diff of what changed.
        json.dump(data, f, ensure_ascii=False, sort_keys=True, indent=1)
        f.write("\n")
    print(f"wrote {args.out}: registry {file_date}, "
          f"{len(sections['languages'])} languages, {len(reading)} ISO 639-2 codes read, "
          f"{len(writing)} written")


if __name__ == "__main__":
    main()
