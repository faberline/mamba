"""Negative controls for hosted-reference receipts; no product is executed."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

import q0_ruff_reference as reference


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def command(self, code, name="result", **options):
        return reference.run_command([sys.executable, "-I", "-c", code], self.root,
                                     self.root / name, **options)

    def test_expected_error_exit_is_retained_with_actual_streams(self):
        result = self.command("import sys; print('diagnostic'); print('detail', file=sys.stderr); sys.exit(1)",
                              expected_exits=(0, 1))
        self.assertTrue(result["passed"])
        self.assertEqual(result["actual_exit"], 1)
        self.assertEqual(result["pid"], result["waited_pid"])
        self.assertTrue(result["wait_completed"])
        self.assertEqual((self.root / "result/stdout.raw").read_bytes(), b"diagnostic\n")
        self.assertEqual((self.root / "result/stderr.raw").read_bytes(), b"detail\n")
        saved = json.loads((self.root / "result/receipt.json").read_bytes())
        self.assertEqual(saved["stdout_sha256"], hashlib.sha256(b"diagnostic\n").hexdigest())

    def test_unexpected_exit_cannot_pass(self):
        result = self.command("raise SystemExit(2)")
        self.assertFalse(result["passed"])
        self.assertEqual(result["actual_exit"], 2)

    def test_timeout_kills_and_waits_for_child(self):
        result = self.command("import time; time.sleep(10)", timeout=0.1)
        self.assertTrue(result["timed_out"])
        self.assertTrue(result["wait_completed"])
        self.assertEqual(result["pid"], result["waited_pid"])
        self.assertIsNotNone(result["actual_exit"])
        self.assertFalse(result["passed"])

    def test_failed_launch_still_has_terminal_receipt(self):
        result = reference.run_command([str(self.root / "missing-program")], self.root,
                                        self.root / "missing")
        self.assertFalse(result["passed"])
        self.assertIsNone(result["pid"])
        self.assertFalse(result["wait_completed"])
        self.assertIsNotNone(result["launch_error"])
        self.assertTrue((self.root / "missing/receipt.json").is_file())

    def test_hash_mismatch_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            reference.checked_bytes(b"changed", hashlib.sha256(b"original").hexdigest())

    def test_existing_receipt_is_never_overwritten(self):
        self.command("print('first')")
        with self.assertRaises(FileExistsError):
            self.command("print('second')")
        self.assertEqual((self.root / "result/stdout.raw").read_bytes(), b"first\n")

    def test_missing_target_and_not_run_target_cannot_pass(self):
        missing = reference.terminal_summary({12: {"status": "passed"}, 13: {"status": "passed"}})
        self.assertFalse(missing["all_targets_terminal"])
        self.assertFalse(missing["execution_passed"])
        stopped = reference.terminal_summary({12: {"status": "passed"}, 13: {"status": "passed"},
                                               14: {"status": "not_run"}})
        self.assertTrue(stopped["all_targets_terminal"])
        self.assertFalse(stopped["execution_passed"])
        self.assertFalse(stopped["issue_accepted"])


if __name__ == "__main__":
    unittest.main()
