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

    # --- a review in a consuming repository (28 Sept 2026) -------------------
    # Deleting the lock line for the PHP conformance runner switched off the
    # check on it (its name is not "distinctive", so no unlisted-copy search
    # finds it): the reviewer then made the runner exit(0) and the checker
    # still said every copy matched. The PHP files are now all-or-none.

    def init_with_php_files(self):
        """Re-initialise the consumer with the PHP implementation's three
        files as well as the required ones."""
        args = ["--init", COMMIT]
        for master in cc.REQUIRED_MASTER_FILES:
            args += ["--file", f"copies/{os.path.basename(master)}={master}"]
        for master in cc.PHP_BINDING_MASTER_FILES:
            args += ["--file", f"php/{master}={master}"]
        self.assertEqual(self.run_checker(args), 0, self.err)

    def test_lock_with_all_three_php_files_passes(self):
        self.init_with_php_files()
        self.assertEqual(self.run_checker(), 0, self.err)
        self.assertIn(f"{len(cc.REQUIRED_MASTER_FILES) + 3} copies match", self.out)

    def test_deleting_the_php_runner_lock_line_fails(self):
        # The reviewer's exact steps: delete the runner's lock line, then
        # neuter the runner. Both steps must not leave a passing check.
        self.init_with_php_files()
        runner = "bindings/php/media-language/tests/run-conformance.php"
        self.write_lock_text("".join(l for l in self.lock_text().splitlines(True)
                                     if not l.rstrip().endswith(" " + runner)))
        with open(f"php/{runner}", "w") as f:
            f.write("<?php exit(0);\n")
        self.assertEqual(self.run_checker(["--offline"]), 1)
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("names some of the PHP implementation's files but not all", self.err)
        self.assertIn(runner, self.err)

    def test_deleting_the_php_readme_lock_line_fails(self):
        self.init_with_php_files()
        readme = "bindings/php/media-language/README.md"
        self.write_lock_text("".join(l for l in self.lock_text().splitlines(True)
                                     if not l.rstrip().endswith(" " + readme)))
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("but not all", self.err)

    def test_init_with_some_php_files_but_not_all_fails(self):
        args = ["--init", COMMIT, "--lock", "partial/MWBM-MEDIA-LANG.lock"]
        for master in cc.REQUIRED_MASTER_FILES:
            args += ["--file", f"partial/{os.path.basename(master)}={master}"]
        args += ["--file", f"partial/MediaLanguagePolicy.php={cc.PHP_BINDING_MASTER_FILES[0]}"]
        self.assertEqual(self.run_checker(args), 1)
        self.assertIn("names some of the PHP implementation's files but not all", self.err)
        self.assertFalse(os.path.exists("partial/MWBM-MEDIA-LANG.lock"))

    # --- Codex's review r7: the all-three rule held across the whole lock ---
    # With two copies of the PHP implementation (app1/ and app2/), app1's
    # three lines satisfied a rule that only counted master paths, so
    # app2's runner line could be deleted and the runner neutered. The rule
    # now holds per copy, and runner- and README-shaped files laid out like
    # a copy are searched for.

    def init_with_two_php_copies(self):
        """Re-initialise the consumer with two complete copies of the PHP
        implementation, laid out as the master lays it out, under app1/ and
        app2/."""
        args = ["--init", COMMIT]
        for master in cc.REQUIRED_MASTER_FILES:
            args += ["--file", f"copies/{os.path.basename(master)}={master}"]
        for app in ("app1", "app2"):
            for master in cc.PHP_BINDING_MASTER_FILES:
                # The master's own layout, below bindings/php/media-language/.
                place = master.split("bindings/php/media-language/", 1)[1]
                args += ["--file", f"{app}/php/{place}={master}"]
        self.assertEqual(self.run_checker(args), 0, self.err)

    def test_two_complete_php_copies_pass(self):
        self.init_with_two_php_copies()
        self.assertEqual(self.run_checker(), 0, self.err)
        self.assertIn(f"{len(cc.REQUIRED_MASTER_FILES) + 6} copies match", self.out)

    def test_deleting_one_copys_runner_line_fails_codex_app1_app2(self):
        # Codex's exact case: delete only app2's runner line, then neuter
        # app2's runner. app1 still lists all three master paths.
        self.init_with_two_php_copies()
        local = "app2/php/tests/run-conformance.php"
        self.write_lock_text("".join(l for l in self.lock_text().splitlines(True)
                                     if f" {local} " not in l))
        with open(local, "w") as f:
            f.write("<?php exit(0);\n")
        self.assertEqual(self.run_checker(["--offline"]), 1)
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("for the copy in app2/php", self.err)
        self.assertIn("bindings/php/media-language/tests/run-conformance.php is missing", self.err)

    def test_deleting_the_other_copys_readme_line_fails(self):
        # The same for README.md, in the FIRST copy this time: app2 still
        # lists the README master path, which used to satisfy the rule.
        self.init_with_two_php_copies()
        self.write_lock_text("".join(l for l in self.lock_text().splitlines(True)
                                     if " app1/php/README.md " not in l))
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("for the copy in app1/php", self.err)
        self.assertIn("bindings/php/media-language/README.md is missing", self.err)

    def test_a_php_copy_laid_out_differently_fails(self):
        # The runner loads ../MediaLanguagePolicy.php, so a copy must keep
        # the master's layout - and its files could not be grouped
        # otherwise.
        args = ["--init", COMMIT, "--lock", "odd/MWBM-MEDIA-LANG.lock"]
        for master in cc.REQUIRED_MASTER_FILES:
            args += ["--file", f"odd/{os.path.basename(master)}={master}"]
        for master in cc.PHP_BINDING_MASTER_FILES:
            args += ["--file", f"odd/php/{os.path.basename(master)}={master}"]
        self.assertEqual(self.run_checker(args), 1)
        self.assertIn("must keep the master's layout", self.err)
        self.assertIn("tests/run-conformance.php", self.err)
        self.assertFalse(os.path.exists("odd/MWBM-MEDIA-LANG.lock"))

    def test_an_unlisted_runner_laid_out_like_a_copy_fails(self):
        # A runner no lock line names, in the layout CI would run - such as
        # one left behind after all of a copy's lines and its library file
        # were removed - is refused by where it sits, not by its name.
        self.init_with_php_files()
        os.makedirs("app3/tests")
        with open("app3/tests/run-conformance.php", "w") as f:
            f.write("<?php exit(0);\n")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("app3/tests/run-conformance.php is laid out like a copy of the PHP "
                      "conformance runner", self.err)

    def test_an_unlisted_readme_beside_a_php_library_file_is_reported(self):
        self.init_with_php_files()
        os.makedirs("app3")
        for name in ("MediaLanguagePolicy.php", "README.md"):
            with open(f"app3/{name}", "w") as f:
                f.write("unlisted\n")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("app3/MediaLanguagePolicy.php has the name of a master file", self.err)
        self.assertIn("app3/README.md sits where a copy of the PHP implementation keeps its "
                      "README.md", self.err)

    def test_other_readmes_and_runners_are_not_mistaken_for_copies(self):
        # README.md elsewhere is an ordinary file, and with no PHP copy in
        # the lock a runner-shaped file is none of this checker's business.
        os.makedirs("notes")
        os.makedirs("tools/tests")
        for path in ("notes/README.md", "README.md", "tools/tests/run-conformance.php"):
            with open(path, "w") as f:
                f.write("not a copy\n")
        self.assertEqual(self.run_checker(), 0, self.err)
        # With a PHP copy in the lock, a README.md away from any copy is
        # still ordinary.
        self.init_with_php_files()
        os.unlink("tools/tests/run-conformance.php")
        self.assertEqual(self.run_checker(), 0, self.err)

    def test_an_unlisted_runner_is_found_in_a_git_checkout_too(self):
        import subprocess
        self.init_with_php_files()
        subprocess.run(["git", "init", "-q"], check=True)
        os.makedirs("app3/tests")
        with open("app3/tests/run-conformance.php", "w") as f:
            f.write("<?php exit(0);\n")
        subprocess.run(["git", "add", "-A"], check=True)
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("app3/tests/run-conformance.php is laid out like a copy", self.err)

    # --- second review (Codex, 28 Sept 2026) --------------------------------

    def test_lock_outside_the_repository_fails(self):
        # A VALID lock placed outside the repository (the case Codex found):
        # without the check, --update would rewrite it and exit 0.
        outside = tempfile.mkdtemp()
        outside_lock = os.path.join(outside, "OUTSIDE.lock")
        with open(cc.DEFAULT_LOCK) as f:
            before = f.read()
        with open(outside_lock, "w") as f:
            f.write(before)
        self.assertEqual(self.run_checker(["--update", "e" * 40, "--lock", outside_lock]), 1)
        with open(outside_lock) as f:
            self.assertEqual(f.read(), before)

    def test_lock_that_is_a_symbolic_link_fails(self):
        outside = tempfile.mkdtemp()
        target = os.path.join(outside, "real.lock")
        os.replace(cc.DEFAULT_LOCK, target)
        os.symlink(target, cc.DEFAULT_LOCK)
        with open(target) as f:
            before = f.read()
        self.assertEqual(self.run_checker(["--update", COMMIT]), 1)
        with open(target) as f:
            self.assertEqual(f.read(), before)

    def test_hard_linked_copy_is_not_written_through(self):
        outside = tempfile.mkdtemp()
        outside_file = os.path.join(outside, "outside-policy.md")
        with open(outside_file, "wb") as f:
            f.write(b"someone else's file\n")
        copy = "copies/media-language-bcp47-policy.md"
        os.unlink(copy)
        os.link(outside_file, copy)
        self.assertEqual(self.run_checker(["--update", COMMIT]), 0, self.err)
        with open(outside_file, "rb") as f:
            self.assertEqual(f.read(), b"someone else's file\n")
        with open(copy, "rb") as f:
            self.assertEqual(f.read(), POLICY)

    @unittest.skipIf(hasattr(os, "geteuid") and os.geteuid() == 0, "root can read anything")
    def test_unreadable_folder_fails_the_check(self):
        os.makedirs("hidden")
        os.chmod("hidden", 0)
        try:
            self.assertEqual(self.run_checker(), 1)
            self.assertIn("could not read", self.err)
        finally:
            os.chmod("hidden", 0o755)

    def test_in_a_git_checkout_only_tracked_files_are_searched(self):
        import subprocess
        subprocess.run(["git", "init", "-q"], check=True)
        subprocess.run(["git", "add", "-A"], check=True)
        # Build output git does not track (SwiftPM copies bundled data into
        # .build) is not a hand-made copy.
        os.makedirs(".build/res")
        with open(".build/res/bcp47-language-data-v1.json", "w") as f:
            f.write("{}")
        with open(".gitignore", "w") as f:
            f.write(".build/\n")
        self.assertEqual(self.run_checker(), 0, self.err)
        # A tracked copy that is not in the lock is still caught.
        os.makedirs("elsewhere")
        with open("elsewhere/bcp47-language-data-v1.json", "w") as f:
            f.write("{}")
        subprocess.run(["git", "add", "elsewhere"], check=True)
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("not in the lock", self.err)

    # --- third review (Codex, 28 Sept 2026) ---------------------------------

    def test_lock_path_through_a_link_and_dot_dot_fails(self):
        outside = tempfile.mkdtemp()
        os.makedirs(os.path.join(outside, "child"))
        os.symlink(os.path.join(outside, "child"), "jump")
        with open(cc.DEFAULT_LOCK) as f:
            lock = f.read()
        with open(os.path.join(outside, "escaped.lock"), "w") as f:
            f.write(lock)
        self.assertEqual(self.run_checker(["--update", "f" * 40, "--lock", "jump/../escaped.lock"]), 1)
        with open(os.path.join(outside, "escaped.lock")) as f:
            self.assertEqual(f.read(), lock)

    def test_linked_folder_outside_git_fails(self):
        other = tempfile.mkdtemp()
        with open(os.path.join(other, "bcp47-language-data-v1.json"), "w") as f:
            f.write("altered")
        os.symlink(other, "translations")
        self.assertEqual(self.run_checker(), 1)
        self.assertIn("link to a folder", self.err)

    def test_update_keeps_file_permissions(self):
        checker = "copies/check_copies.py"
        os.chmod(checker, 0o755)
        os.chmod("copies/bcp47-language-data-v1.json", 0o644)
        self.assertEqual(self.run_checker(["--update", COMMIT]), 0, self.err)
        self.assertEqual(os.stat(checker).st_mode & 0o777, 0o755)
        self.assertEqual(os.stat("copies/bcp47-language-data-v1.json").st_mode & 0o777, 0o644)

    def test_new_copies_are_not_owner_only(self):
        umask = os.umask(0o022)
        try:
            args = ["--init", COMMIT, "--lock", "fresh/MWBM-MEDIA-LANG.lock"]
            for master in cc.REQUIRED_MASTER_FILES:
                args += ["--file", f"fresh/{os.path.basename(master)}={master}"]
            self.assertEqual(self.run_checker(args), 0, self.err)
            self.assertEqual(os.stat("fresh/media-language-bcp47-policy.md").st_mode & 0o777, 0o644)
        finally:
            os.umask(umask)


if __name__ == "__main__":
    unittest.main()
