#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# scripts/media-lang/test_check_copies.py
#
# Tests for check_copies.py. Each "must fail" test reproduces one way an
# independent review (28 Sept 2026) made the first version of the checker
# report a PASS when copies did not match the master, or write outside the
# repository. They exist so that none of those holes can quietly reopen.
#
# The network is replaced by stand-ins (FakeMaster below), so these run
# offline and never touch GitHub. What they cannot show: that GitHub's real
# compare lookup answers the way commit_is_on_approved_branch() expects —
# that was checked by hand against the live API when the checker was
# written, and is re-checked every time a consumer's CI runs it for real.
#
# Run: python3 -m unittest scripts/media-lang/test_check_copies.py

import contextlib
import importlib.util
import io
import os
import tempfile
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("check_copies", os.path.join(HERE, "check_copies.py"))
cc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cc)

COMMIT = "a" * 40
POLICY = b"# Media Language & BCP 47 Policy\n\n| **Version** | `1.0.0` |\n"


class FakeMaster:
    """Stands in for GitHub: a fixed set of master files at COMMIT, and a
    switch for whether COMMIT counts as part of the approved history."""

    def __init__(self, approved=True):
        self.approved = approved
        self.files = {path: (POLICY if path == cc.POLICY_MASTER_PATH else f"master {path}\n".encode())
                      for path in cc.MASTER_FILES}

    def download(self, commit, path):
        if path not in cc.MASTER_FILES:
            raise cc.CheckFailed("not a master file")
        return self.files[path]

    def approved_check(self, commit):
        return self.approved


def read_all(folder):
    """Every file in a folder, by name, read and closed."""
    out = {}
    for name in os.listdir(folder):
        with open(os.path.join(folder, name), "rb") as f:
            out[name] = f.read()
    return out


class CheckCopiesTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.old_cwd = os.getcwd()
        os.chdir(self.root)
        self.master = FakeMaster()
        self.patches = [
            mock.patch.object(cc, "download", side_effect=self.master.download),
            mock.patch.object(cc, "commit_is_on_approved_branch", side_effect=self.master.approved_check),
            mock.patch.dict(os.environ, {"CI": "", "GITHUB_ACTIONS": ""}),
        ]
        for p in self.patches:
            p.start()
        # A consumer with every required file, laid out under copies/.
        args = ["--init", COMMIT]
        for master in cc.REQUIRED_MASTER_FILES:
            args += ["--file", f"copies/{os.path.basename(master)}={master}"]
        self.assertEqual(self.run_checker(args), 0, self.err)

    def tearDown(self):
        for p in self.patches:
            p.stop()
        os.chdir(self.old_cwd)
        self.tmp.cleanup()

    def run_checker(self, args=()):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cc.main(list(args))
        self.out, self.err = out.getvalue(), err.getvalue()
        return code

    def lock_text(self):
        with open(cc.DEFAULT_LOCK, encoding="utf-8") as f:
            return f.read()

    def write_lock_text(self, text):
        with open(cc.DEFAULT_LOCK, "w", encoding="utf-8") as f:
            f.write(text)

    # --- the good path -----------------------------------------------------

    def test_clean_copies_pass(self):
        self.assertEqual(self.run_checker(), 0, self.err)
        self.assertIn("match the master", self.out)

    # --- review finding A1: a master path pointing somewhere else -----------

    def test_master_path_outside_the_list_fails(self):
        text = self.lock_text().replace(
            "tests/fixtures/bcp47-language-policy-v1.json",
            "../../../pallets/markupsafe/main/README.md", 1)
        self.write_lock_text(text)
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("is not a master file", self.err)

    def test_master_path_with_dot_segment_fails(self):
        text = self.lock_text().replace(
            " docs/standards/media-language-bcp47-policy.md\n",
            " docs/standards/./media-language-bcp47-policy.md\n", 1)
        self.write_lock_text(text)
        self.assertEqual(self.run_checker(), 1)

    # --- review finding A2: a commit from a fork ----------------------------

    def test_commit_not_on_an_approved_branch_fails(self):
        self.master.approved = False
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("not part of", self.err)

    def test_update_from_an_unapproved_commit_changes_nothing(self):
        before = self.lock_text()
        self.master.approved = False
        self.assertEqual(self.run_checker(["--update", "b" * 40]), 1)
        self.assertEqual(self.lock_text(), before)

    # --- review finding A3: deleting a lock line ------------------------------

    def test_lock_missing_a_required_file_fails(self):
        lines = [l for l in self.lock_text().splitlines(True)
                 if "bcp47-language-policy-v1.json" not in l or "schema" in l]
        self.write_lock_text("".join(lines))
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("leaves out files every copy must have", self.err)

    def test_copy_not_listed_in_the_lock_fails(self):
        os.makedirs("elsewhere")
        with open("elsewhere/bcp47-language-policy-v1.json", "w") as f:
            f.write("an unchecked copy")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("not in the lock", self.err)

    # --- review finding A4: writing outside the repository ------------------

    def test_local_path_leaving_the_repository_fails(self):
        text = self.lock_text().replace("copies/check_copies.py", "../OUTSIDE.py", 1)
        self.write_lock_text(text)
        self.assertEqual(self.run_checker(["--update", COMMIT]), 1)
        self.assertFalse(os.path.exists(os.path.join(self.root, "..", "OUTSIDE.py")))

    def test_local_path_inside_git_fails(self):
        text = self.lock_text().replace("copies/check_copies.py", ".git/hooks/pre-commit", 1)
        self.write_lock_text(text)
        self.assertEqual(self.run_checker(["--update", COMMIT]), 1)

    def test_local_path_through_a_symbolic_link_fails(self):
        outside = tempfile.mkdtemp()
        os.symlink(outside, "linked")
        text = self.lock_text().replace("copies/check_copies.py", "linked/check_copies.py", 1)
        self.write_lock_text(text)
        self.assertEqual(self.run_checker(["--update", COMMIT]), 1)
        self.assertEqual(os.listdir(outside), [])

    # --- review finding A5: --offline in CI ---------------------------------

    def test_offline_is_refused_in_ci(self):
        with mock.patch.dict(os.environ, {"CI": "true"}):
            self.assertEqual(self.run_checker(["--offline"]), 1)
        self.assertIn("refused in CI", self.err)

    def test_offline_outside_ci_says_it_did_not_check_the_master(self):
        self.assertEqual(self.run_checker(["--offline"]), 0)
        self.assertIn("NOT CHECKED", self.out)

    # --- edits, line endings, duplicates, part-way failures -----------------

    def test_edited_copy_fails(self):
        with open("copies/bcp47-language-data-v1.json", "a") as f:
            f.write("edited")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("has been changed", self.err)

    def test_line_ending_conversion_is_named(self):
        path = "copies/media-language-bcp47-policy.md"
        with open(path, "rb") as f:
            data = f.read()
        with open(path, "wb") as f:
            f.write(data.replace(b"\n", b"\r\n"))
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("line endings", self.err)

    def test_second_source_line_fails(self):
        self.write_lock_text(self.lock_text() + f"source {cc.MASTER_REPO} {'c' * 40}\n")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("second source line", self.err)

    def test_failed_download_during_update_writes_nothing(self):
        before = read_all("copies")
        calls = {"n": 0}

        def flaky(commit, path):
            calls["n"] += 1
            if calls["n"] == 3:
                raise cc.CheckFailed("network went away")
            return b"new content"
        with mock.patch.object(cc, "download", side_effect=flaky):
            self.assertEqual(self.run_checker(["--update", "d" * 40]), 1)
        after = read_all("copies")
        self.assertEqual(before, after)

    def test_failed_master_download_fails_the_check(self):
        with mock.patch.object(cc, "download", side_effect=cc.CheckFailed("no network")):
            self.assertEqual(self.run_checker(), 1)

    def test_init_without_every_required_file_fails(self):
        self.assertEqual(self.run_checker(
            ["--init", COMMIT, "--file", f"x/policy.md={cc.POLICY_MASTER_PATH}"]), 1)
        self.assertIn("must include every required file", self.err)


if __name__ == "__main__":
    unittest.main()
