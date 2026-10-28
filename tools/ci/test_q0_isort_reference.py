"""Pure controls for isort originals; no isort or Mamba execution."""
import ast
import json
from pathlib import Path
import sys
import tempfile
import unittest

from q0_isort_oracle import EXCLUDED_API_CODE, api_case, checker_receipt, child_path
from q0_isort_reference import PACKAGE_PROBE, checked_rows, terminal_summary, validate_dependencies


class OracleControls(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    @staticmethod
    def config(**kwargs):
        return kwargs

    def case(self, sorter):
        return api_case({"id": "example", "source": "import z\n"}, self.root / "case", 12,
                        sorter, self.config)

    def test_nonconvergence_is_retained(self):
        result = self.case(lambda source, **kwargs: source + "\n")
        self.assertFalse(result["reference_converges"])
        self.assertNotEqual(result["output"], result["repeat_output"])
        self.assertNotIn("reference_error", result)

    def test_original_exception_is_not_a_pass(self):
        def fail(*args, **kwargs):
            raise ValueError("original detail")
        result = self.case(fail)
        self.assertEqual(result["reference_error"], "ValueError: original detail")
        self.assertNotIn("reference_converges", result)

    def test_original_skip_is_distinct(self):
        FileSkipComment = type("FileSkipComment", (Exception,), {})
        def skip(*args, **kwargs):
            raise FileSkipComment("skip")
        result = self.case(skip)
        self.assertTrue(result["reference_skipped"])
        self.assertTrue(result["reference_converges"])
        self.assertEqual(result["output"], "import z\n")
        self.assertFalse(result["repeat_call_executed"])
        self.assertNotIn("repeat_output", result)

    def test_layout_cannot_escape(self):
        with self.assertRaisesRegex(ValueError, "stay within"):
            child_path(self.root, "../outside.py")
        with self.assertRaisesRegex(ValueError, "stay within"):
            child_path(self.root, "/outside.py")

    def test_batch_identity_and_count_are_required(self):
        output = self.root / "batch"
        output.mkdir()
        (output / "stdout.raw").write_text(json.dumps([{"id": "wrong"}]))
        with self.assertRaisesRegex(ValueError, "identity or order"):
            checked_rows(output, [{"id": "expected"}])

    def receipt(self, code, timeout=5, stdin=None):
        source = self.root / "main.py"
        source.write_bytes(b"fixture bytes\n")
        project = self.root / "pyproject.toml"
        project.write_bytes(b"[project]\n")
        return checker_receipt([sys.executable, "-I", "-B", "-c", code], self.root,
                               source, project, timeout=timeout, stdin=stdin)

    def test_checker_records_real_error_exit_and_streams(self):
        record = self.receipt("import sys; print('check'); print('error',file=sys.stderr); sys.exit(1)")
        self.assertEqual(record["actual_exit"], 1)
        self.assertEqual(record["pid"], record["waited_pid"])
        self.assertTrue(record["wait_completed"])
        self.assertTrue(record["source_unchanged"])
        self.assertEqual((self.root / "stdout.raw").read_bytes(), b"check\n")
        self.assertEqual((self.root / "stderr.raw").read_bytes(), b"error\n")

    def test_checker_timeout_is_waited_and_not_hidden(self):
        record = self.receipt("import time; time.sleep(10)", timeout=0.1)
        self.assertTrue(record["timed_out"])
        self.assertTrue(record["wait_completed"])
        self.assertLess(record["actual_exit"], 0)

    def test_stdin_bytes_are_retained_and_delivered(self):
        payload = b'{"source":"input bytes"}\n'
        record = self.receipt("import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())", stdin=payload)
        self.assertEqual(record["actual_exit"], 0)
        self.assertEqual((self.root / "stdin.raw").read_bytes(), payload)
        self.assertEqual((self.root / "stdout.raw").read_bytes(), payload)

    def test_checker_deletion_still_keeps_terminal_receipt(self):
        record = self.receipt("from pathlib import Path; Path('main.py').unlink()")
        self.assertEqual(record["actual_exit"], 0)
        self.assertTrue(record["wait_completed"])
        self.assertFalse(record["source_unchanged"])
        self.assertEqual(record["source_after_error"]["type"], "FileNotFoundError")
        self.assertTrue((self.root / "receipt.json").is_file())

    def test_missing_checker_and_failed_target_cannot_pass(self):
        targets = {minor: {"status": "passed"} for minor in (12, 13, 14)}
        self.assertFalse(terminal_summary(targets, None)["execution_passed"])
        self.assertFalse(terminal_summary(targets, {"reference_comparisons_passed": True})["execution_passed"])
        self.assertFalse(terminal_summary(targets, {"reference_comparisons_passed": True},
                                          {"checker_comparisons_passed": True})["execution_passed"])
        targets[14] = {"status": "failed"}
        summary = terminal_summary(targets, {"reference_comparisons_passed": True},
                                   {"checker_comparisons_passed": True})
        self.assertFalse(summary["execution_passed"])
        self.assertFalse(summary["issue_accepted"])
        self.assertFalse(summary["whole_suite_accepted"])

    def test_hosted_child_programs_have_valid_syntax(self):
        ast.parse(PACKAGE_PROBE)
        ast.parse(EXCLUDED_API_CODE)


class DependencyControls(unittest.TestCase):
    def setUp(self):
        self.pins = [{"name": "mypy_extensions", "version": "1.1.0"}]
        self.installed = {
            "isort": {"version": "9.0.2", "requires_dist": [
                "mypy-extensions>=1.1.0", "colorama; extra == 'colors'"]},
            "mypy-extensions": {"version": "1.1.0", "requires_dist": []},
        }

    def test_pinned_requirement_and_inactive_extra(self):
        result = validate_dependencies(self.pins, self.installed)
        self.assertTrue(result["dependencies_verified"])
        self.assertEqual(result["pinned_versions"]["mypy-extensions"], "1.1.0")
        self.assertEqual(len(result["active_requirements"]), 1)
        self.assertEqual(result["inactive_requirements"][0]["name"], "colorama")

    def test_missing_pinned_dependency_fails(self):
        del self.installed["mypy-extensions"]
        with self.assertRaisesRegex(ValueError, "is missing"):
            validate_dependencies(self.pins, self.installed)

    def test_installed_version_must_match_exact_pin(self):
        self.installed["mypy-extensions"]["version"] = "1.2.0"
        with self.assertRaisesRegex(ValueError, "differs from pin"):
            validate_dependencies(self.pins, self.installed)

    def test_pin_must_satisfy_active_constraint(self):
        self.pins[0]["version"] = "1.0.0"
        self.installed["mypy-extensions"]["version"] = "1.0.0"
        with self.assertRaisesRegex(ValueError, "constraint is not satisfied"):
            validate_dependencies(self.pins, self.installed)

    def test_unpinned_transitive_dependency_fails(self):
        self.installed["mypy-extensions"]["requires_dist"] = ["unlocked>=2"]
        with self.assertRaisesRegex(ValueError, "is not pinned"):
            validate_dependencies(self.pins, self.installed)

    def test_unsupported_active_dependency_source_fails(self):
        self.installed["isort"]["requires_dist"] = [
            "mypy-extensions @ https://example.invalid/mypy.whl"]
        with self.assertRaisesRegex(ValueError, "unsupported active"):
            validate_dependencies(self.pins, self.installed)

    def test_duplicate_alias_pins_fail(self):
        self.pins.append({"name": "mypy-extensions", "version": "1.1.0"})
        with self.assertRaisesRegex(ValueError, "duplicate dependency pin"):
            validate_dependencies(self.pins, self.installed)


if __name__ == "__main__":
    unittest.main()
