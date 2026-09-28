#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# scripts/media-lang/check_copies.py
#
# Keeps a repository's copies of the MWBM-MEDIA-LANG policy files identical
# to the master copies in MWBMPartners/MeedyaSuite-core.
#
# THIS FILE IS ITSELF COPIED VERBATIM into every repository that uses the
# policy (and listed in that repository's lock, so an edited copy of the
# checker is caught too). Change it in MeedyaSuite-core only.
#
# WHY COPIES AT ALL
# -----------------
# The policy must be readable from a plain checkout of each repository, by a
# person or by any coding assistant, with no network and no submodule. So
# each repository holds real copies. Copies drift unless something checks
# them — this script is that something, run in each repository's own CI.
#
# THE LOCK FILE (default: docs/standards/MWBM-MEDIA-LANG.lock)
# -----------------------------------------------------------
# Plain text, written by --update, never by hand:
#
#   # comment lines start with '#'
#   policy MWBM-MEDIA-LANG 1.0.0
#   source MWBMPartners/MeedyaSuite-core <40-character commit>
#   file <sha256> <path in this repo> <path in MeedyaSuite-core>
#   file ...
#
# Paths may not contain spaces (checked), so the three-column form is safe.
#
# WHAT A NORMAL RUN CHECKS (both must pass; CI runs this mode)
# ------------------------------------------------------------
# 1. Every local copy still has the checksum recorded in the lock — so a
#    hand edit to a copy fails the build.
# 2. The recorded checksums match the master files at the recorded commit,
#    downloaded from GitHub. MeedyaSuite-core is public, so no token is
#    needed. If the download fails for ANY reason the run fails: a check
#    that could not run is never reported as a pass.
#
# --offline skips step 2 and says so loudly. It is for working without a
# network; CI must not use it.
#
# WHAT IT CANNOT DO
# -----------------
# It does not tell you a newer policy version exists — moving to one is a
# deliberate change (the policy's section 8.3). It does not check that the
# repository's code follows the policy; the conformance test cases do that.
#
# Usage:
#   python3 scripts/media-lang/check_copies.py                 # verify (CI)
#   python3 scripts/media-lang/check_copies.py --offline       # local only
#   python3 scripts/media-lang/check_copies.py --update <commit>
#       re-download every file listed in the lock at <commit>, overwrite the
#       copies and rewrite the lock
#   python3 scripts/media-lang/check_copies.py --init <commit> \
#       --file <local path>=<master path> [--file ...]
#       first-time setup: fetch the named files and write a new lock

import argparse
import hashlib
import os
import re
import sys
import urllib.error
import urllib.request

MASTER_REPO = "MWBMPartners/MeedyaSuite-core"
RAW_URL = "https://raw.githubusercontent.com/{repo}/{commit}/{path}"
DEFAULT_LOCK = "docs/standards/MWBM-MEDIA-LANG.lock"
POLICY_MASTER_PATH = "docs/standards/media-language-bcp47-policy.md"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
# A full commit is required, never a branch name: a branch moves, so a lock
# naming one would check against a different master tomorrow.


def fail(message):
    print(f"MWBM-MEDIA-LANG copy check FAILED: {message}", file=sys.stderr)
    sys.exit(1)


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    with open(path, "rb") as f:
        return sha256_bytes(f.read())


def download(commit, master_path):
    """Fetch one master file at one commit. Any failure is fatal: the caller
    must never treat "could not download" as "matches"."""
    url = RAW_URL.format(repo=MASTER_REPO, commit=commit, path=master_path)
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            return response.read()
    except (urllib.error.URLError, OSError) as exc:
        fail(f"could not download {url} ({exc}). The check cannot run without it, "
             "so it fails rather than passing. Use --offline only when working "
             "without a network, never in CI.")


def read_policy_version(text):
    """The version printed in the policy document's header table."""
    match = re.search(r"\|\s*\*\*Version\*\*\s*\|\s*`([0-9]+\.[0-9]+\.[0-9]+)`", text)
    return match.group(1) if match else None


def parse_lock(lock_path):
    if not os.path.isfile(lock_path):
        fail(f"no lock file at {lock_path}")
    policy_version = commit = None
    files = []
    with open(lock_path, encoding="utf-8") as f:
        for number, raw in enumerate(f, 1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if parts[0] == "policy" and len(parts) == 3 and parts[1] == "MWBM-MEDIA-LANG":
                policy_version = parts[2]
            elif parts[0] == "source" and len(parts) == 3 and parts[1] == MASTER_REPO:
                commit = parts[2]
            elif parts[0] == "file" and len(parts) == 4 and SHA_RE.match(parts[1]):
                files.append({"sha256": parts[1], "local": parts[2], "master": parts[3]})
            else:
                fail(f"{lock_path} line {number} is not understood: {line!r}")
    if not policy_version or not commit or not COMMIT_RE.match(commit) or not files:
        fail(f"{lock_path} must have a policy line, a source line with a full "
             "40-character commit, and at least one file line")
    return policy_version, commit, files


def write_lock(lock_path, policy_version, commit, files):
    for entry in files:
        for key in ("local", "master"):
            if re.search(r"\s", entry[key]):
                fail(f"path {entry[key]!r} contains a space; the lock format cannot hold it")
    os.makedirs(os.path.dirname(lock_path) or ".", exist_ok=True)
    with open(lock_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("# MWBM-MEDIA-LANG copy lock. Written by scripts/media-lang/check_copies.py\n")
        f.write("# --update; do not edit by hand. Master: MWBMPartners/MeedyaSuite-core.\n")
        f.write(f"policy MWBM-MEDIA-LANG {policy_version}\n")
        f.write(f"source {MASTER_REPO} {commit}\n")
        for entry in files:
            f.write(f"file {entry['sha256']} {entry['local']} {entry['master']}\n")


def fetch_into_place(commit, files):
    """Download each master file at the commit, write it to its local path,
    and return the policy version read from the policy document."""
    policy_version = None
    for entry in files:
        data = download(commit, entry["master"])
        os.makedirs(os.path.dirname(entry["local"]) or ".", exist_ok=True)
        with open(entry["local"], "wb") as f:
            f.write(data)
        entry["sha256"] = sha256_bytes(data)
        if entry["master"] == POLICY_MASTER_PATH:
            policy_version = read_policy_version(data.decode("utf-8"))
    if not policy_version:
        fail(f"the files must include the policy document ({POLICY_MASTER_PATH}), "
             "and its header must state a version")
    return policy_version


def verify(lock_path, offline):
    policy_version, commit, files = parse_lock(lock_path)
    problems = []
    for entry in files:
        if not os.path.isfile(entry["local"]):
            problems.append(f"{entry['local']} is missing")
            continue
        actual = sha256_file(entry["local"])
        if actual != entry["sha256"]:
            problems.append(f"{entry['local']} has been changed (checksum {actual[:12]}…, "
                            f"lock says {entry['sha256'][:12]}…). Copies must not be edited "
                            "here: change the master in MeedyaSuite-core and run --update.")
        if entry["master"] == POLICY_MASTER_PATH and os.path.isfile(entry["local"]):
            with open(entry["local"], encoding="utf-8") as f:
                stated = read_policy_version(f.read())
            if stated != policy_version:
                problems.append(f"{entry['local']} says version {stated}, lock says {policy_version}")
    if problems:
        fail("\n  " + "\n  ".join(problems))
    if offline:
        print(f"MWBM-MEDIA-LANG {policy_version}: {len(files)} local copies match the lock. "
              "NOT CHECKED against the master (--offline).")
        return
    for entry in files:
        master = sha256_bytes(download(commit, entry["master"]))
        if master != entry["sha256"]:
            problems.append(f"{entry['local']}: the lock's checksum does not match "
                            f"{entry['master']} at {commit[:12]} in {MASTER_REPO}")
    if problems:
        fail("\n  " + "\n  ".join(problems))
    print(f"MWBM-MEDIA-LANG {policy_version}: {len(files)} copies match the master "
          f"at {MASTER_REPO}@{commit[:12]}.")


def main():
    ap = argparse.ArgumentParser(description="Check or update copies of the MWBM-MEDIA-LANG policy files.")
    ap.add_argument("--lock", default=DEFAULT_LOCK, help=f"lock file path (default {DEFAULT_LOCK})")
    ap.add_argument("--offline", action="store_true", help="check local copies only (never in CI)")
    ap.add_argument("--update", metavar="COMMIT", help="re-fetch every locked file at COMMIT")
    ap.add_argument("--init", metavar="COMMIT", help="first-time setup at COMMIT (with --file)")
    ap.add_argument("--file", action="append", default=[], metavar="LOCAL=MASTER",
                    help="with --init: a file to copy, as local-path=master-path")
    args = ap.parse_args()

    if args.update or args.init:
        commit = args.update or args.init
        if not COMMIT_RE.match(commit):
            fail("give a full 40-character commit, not a branch or short hash")
        if args.init:
            if not args.file:
                fail("--init needs at least one --file local=master")
            files = []
            for spec in args.file:
                if "=" not in spec:
                    fail(f"--file {spec!r} must be local-path=master-path")
                local, master = spec.split("=", 1)
                files.append({"sha256": "", "local": local, "master": master})
        else:
            _, _, files = parse_lock(args.lock)
        policy_version = fetch_into_place(commit, files)
        write_lock(args.lock, policy_version, commit, files)
        print(f"Copied {len(files)} files at {commit[:12]}; lock written to {args.lock} "
              f"(policy {policy_version}). Now update code and tests as the policy changelog requires.")
        return
    verify(args.lock, args.offline)


if __name__ == "__main__":
    main()
