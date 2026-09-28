#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# scripts/media-lang/validate_json.py
#
# Checks the MWBM-MEDIA-LANG test cases and reference data against their
# JSON Schemas (Draft 2020-12, with format checking). Run by CI's
# `language-policy` job. Needs the `jsonschema` package (CI pins it).
#
# Why this exists: the schemas are the documentation other repositories
# read to understand the two files. If a file stops matching its schema,
# either the file or the documentation is now wrong, and both are copied
# into other repositories — so the build fails here, before the copy.

import json
import sys

from jsonschema import Draft202012Validator, FormatChecker

PAIRS = (
    ("tests/fixtures/bcp47-language-policy-v1.json",
     "tests/fixtures/bcp47-language-policy-v1.schema.json"),
    ("docs/standards/data/bcp47-language-data-v1.json",
     "docs/standards/data/bcp47-language-data-v1.schema.json"),
)


def main():
    failed = False
    for data_path, schema_path in PAIRS:
        with open(schema_path, encoding="utf-8") as f:
            schema = json.load(f)
        Draft202012Validator.check_schema(schema)
        with open(data_path, encoding="utf-8") as f:
            data = json.load(f)
        errors = sorted(Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(data),
                        key=lambda e: list(e.path))
        if errors:
            failed = True
            print(f"{data_path} does not match {schema_path}:")
            for e in errors[:20]:
                print(f"  at {'/'.join(map(str, e.path)) or '(top)'}: {e.message}")
        else:
            print(f"{data_path}: matches its schema")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
