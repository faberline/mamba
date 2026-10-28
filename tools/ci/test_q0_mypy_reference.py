"""Pure controls for receipts and gates; never import or run original mypy."""
from __future__ import annotations

import copy
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_mypy_reference as runner


def receipt() -> dict:
    return {"pid": 42, "waited_pid": 42, "wait_completed": True, "actual_exit": 1,
            "timed_out": False, "launch_error": None, "passed": True}


def package(name: str, version: str, requirements: list[str] | None = None) -> dict:
    return {"name": name, "version": version, "requires_dist": requirements or [],
            "distribution_files": {"module.py": {"bytes": 1, "sha256": "a" * 64}},
            "missing_distribution_files": []}


class Dependencies(unittest.TestCase):
    def setUp(self):
        self.pins = [{"name": "root", "version": "2.0"}, {"name": "dep", "version": "1.2"}]
        self.packages = {"root": package("root", "2.0", ["dep>=1.1,<2"]),
                         "dep": package("dep", "1.2")}

    def test_exact_pins_and_transitive_constraints(self):
        result = runner.validate_dependencies(self.pins, self.packages)
        self.assertTrue(result["dependencies_verified"])
        self.assertEqual(result["active_requirements"][0]["version"], "1.2")

    def test_missing_dependency(self):
        del self.packages["dep"]
        with self.assertRaisesRegex(ValueError, "missing"):
            runner.validate_dependencies(self.pins, self.packages)

    def test_wrong_installed_version(self):
        self.packages["dep"]["version"] = "1.3"
        with self.assertRaisesRegex(ValueError, "differs from pin"):
            runner.validate_dependencies(self.pins, self.packages)

    def test_constraint_does_not_match_pin(self):
        self.packages["root"]["requires_dist"] = ["dep>=2"]
        with self.assertRaisesRegex(ValueError, "constraint"):
            runner.validate_dependencies(self.pins, self.packages)

    def test_active_unpinned_requirement(self):
        self.packages["dep"]["requires_dist"] = ["third>=1"]
        with self.assertRaisesRegex(ValueError, "not pinned"):
            runner.validate_dependencies(self.pins, self.packages)

    def test_inactive_extra_is_not_required(self):
        self.packages["root"]["requires_dist"] += ['colors>=1; extra == "colors"']
        result = runner.validate_dependencies(self.pins, self.packages)
        self.assertEqual(len(result["inactive_requirements"]), 1)

    def test_target_markers_select_active_requirement(self):
        self.packages["root"]["requires_dist"] = ['unknown; python_version >= "3.14"']
        runner.validate_dependencies(self.pins, self.packages, {"python_version": "3.12"})
        with self.assertRaisesRegex(ValueError, "not pinned"):
            runner.validate_dependencies(self.pins, self.packages, {"python_version": "3.14"})

    def test_active_extra_and_url_are_unsupported(self):
        for requirement in ["dep[feature]", "dep @ https://example.invalid/dep.whl"]:
            with self.subTest(requirement=requirement):
                self.packages["root"]["requires_dist"] = [requirement]
                with self.assertRaisesRegex(ValueError, "unsupported"):
                    runner.validate_dependencies(self.pins, self.packages)

    def test_duplicate_normalized_pin(self):
        pins = [{"name": "some_dep", "version": "1"}, {"name": "some-dep", "version": "1"}]
        with self.assertRaisesRegex(ValueError, "duplicate"):
            runner.validate_dependencies(pins, {})

    def test_payload_empty_or_missing(self):
        for key, value in [("distribution_files", {}), ("missing_distribution_files", ["lost.py"])]:
            with self.subTest(key=key):
                packages = copy.deepcopy(self.packages)
                packages["dep"][key] = value
                with self.assertRaisesRegex(ValueError, "payload is incomplete"):
                    runner.validate_dependencies(self.pins, packages)


class Receipts(unittest.TestCase):
    def test_python_version_and_isolation_are_required(self):
        runner.require_python_identity({"python": [3, 12, 11], "isolated": 1}, "3.12.11")
        for identity in [{"python": [3, 12, 11]}, {"python": [3, 12, 11], "isolated": 0},
                         {"python": [3, 12, 10], "isolated": 1}]:
            with self.assertRaisesRegex(ValueError, "isolated Python"):
                runner.require_python_identity(identity, "3.12.11")

    def test_terminal_receipt_rejects_missing_or_wrong_fields(self):
        runner.require_receipt(receipt(), (0, 1))
        changes = {"pid": None, "waited_pid": 43, "wait_completed": False, "actual_exit": None,
                   "timed_out": True, "launch_error": {"message": "failed"}, "passed": False}
        for field, value in changes.items():
            with self.subTest(field=field):
                record = receipt()
                record[field] = value
                with self.assertRaisesRegex(ValueError, "terminal receipt"):
                    runner.require_receipt(record, (0, 1))

    def test_exit_one_is_reference_only(self):
        with self.assertRaises(ValueError):
            runner.require_receipt(receipt())

    def test_actual_subprocess_exit_and_raw_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = runner.run_command([sys.executable, "-I", "-c",
                                         "import sys; print('raw'); sys.stderr.write('detail'); sys.exit(1)"],
                                        root, root / "process", expected_exits=(0, 1), inputs={"input": "control"})
            runner.require_receipt(result, (0, 1))
            self.assertEqual((root / "process/stdout.raw").read_bytes(), b"raw\n")
            self.assertEqual((root / "process/stderr.raw").read_bytes(), b"detail")
            self.assertEqual(result["inputs_sha256"], runner.digest((root / "process/inputs.json").read_bytes()))
            self.assertEqual(result["stdout_sha256"], runner.digest(b"raw\n"))

    def test_timeout_keeps_first_output_and_real_wait(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = runner.run_command([sys.executable, "-I", "-c",
                                         "import time; print('before timeout', flush=True); time.sleep(30)"],
                                        root, root / "process", timeout=0.5)
            self.assertTrue(result["timed_out"])
            self.assertTrue(result["wait_completed"])
            self.assertEqual(result["pid"], result["waited_pid"])
            self.assertNotEqual(result["actual_exit"], 0)
            self.assertEqual((root / "process/stdout.raw").read_bytes(), b"before timeout\n")
            with self.assertRaises(ValueError):
                runner.require_receipt(result, (0, 1))
            with self.assertRaises(FileExistsError):
                runner.run_command([sys.executable, "-c", "pass"], root, root / "process")
            self.assertTrue(json.loads((root / "process/receipt.json").read_bytes())["timed_out"])

    def test_launch_failure_is_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = runner.run_command([str(root / "missing-python")], root, root / "process")
            self.assertIsNone(result["pid"])
            self.assertFalse(result["wait_completed"])
            self.assertEqual(result["launch_error"]["type"], "FileNotFoundError")
            with self.assertRaises(ValueError):
                runner.require_receipt(result)


class Gates(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = [{"name": str(i), "filename": f"case_{i:04}.py", "source_sha256": "a" * 64}
                     for i in range(runner.CASE_COUNT)]

    def comparison(self):
        rows = [{**case, "matches_expected": True, "diagnostics": [], "diagnostic_count": 0}
                for case in self.cases]
        rows[0].update(diagnostics=[{"control": True}] * 3, diagnostic_count=3)
        return {"case_count": runner.CASE_COUNT, "matching_case_count": runner.CASE_COUNT,
                "diagnostic_count": 3, "comparisons_passed": True, "cases": rows}

    def test_complete_all_case_comparison(self):
        runner.require_comparison(self.comparison(), self.cases, 3)

    def test_zero_and_partial_rows_fail(self):
        for count in [0, runner.CASE_COUNT - 1]:
            with self.subTest(count=count):
                result = self.comparison()
                result["cases"] = result["cases"][:count]
                with self.assertRaisesRegex(ValueError, "incomplete"):
                    runner.require_comparison(result, self.cases, 3)

    def test_wrong_order_or_one_mismatch_fails(self):
        result = self.comparison()
        result["cases"][0], result["cases"][1] = result["cases"][1], result["cases"][0]
        with self.assertRaisesRegex(ValueError, "identity"):
            runner.require_comparison(result, self.cases, 3)
        result = self.comparison()
        result["cases"][-1]["matches_expected"] = False
        with self.assertRaisesRegex(ValueError, "match"):
            runner.require_comparison(result, self.cases, 3)

    def test_diagnostic_count_cannot_hide_truncated_rows(self):
        result = self.comparison()
        result["cases"][0]["diagnostics"] = []
        with self.assertRaisesRegex(ValueError, "diagnostic row count"):
            runner.require_comparison(result, self.cases, 3)

    def test_project_tampering_fails(self):
        before = {"main.py": {"sha256": "a"}}
        runner.require_unchanged(before, before.copy())
        for after in [{}, {"main.py": {"sha256": "b"}}, {**before, "extra": {"sha256": "c"}}]:
            with self.assertRaises(ValueError):
                runner.require_unchanged(before, after)

    def test_exact_reference_argv_and_relative_input_order(self):
        names = [case["filename"] for case in self.cases]
        result = runner.reference_argv(Path("/venv/bin/python"), Path("/project"), names, 14)
        self.assertEqual(result[:13], ["/venv/bin/python", "-I", "-m", "mypy", "--strict",
                                     "--config-file", "/project/mypy.ini", "--python-version", "3.14",
                                     "--cache-dir=/dev/null", "--no-incremental", "--output=json", names[0]])
        self.assertEqual(result[12:], names)
        self.assertNotIn("-B", result)
        self.assertFalse(any("platform" in arg for arg in result))
        with self.assertRaises(ValueError):
            runner.reference_argv(Path("/python"), Path("/project"), names[:-1], 14)

    def test_summary_requires_every_target_and_no_acceptance(self):
        targets = {minor: {"status": "passed", "case_count": runner.CASE_COUNT,
                           "matching_case_count": runner.CASE_COUNT, "source_unchanged": True,
                           "diagnostic_count": runner.EXPECTED_COUNTS[minor], "receipt": receipt()}
                   for minor in runner.VERSIONS}
        good = runner.terminal_summary(targets)
        self.assertTrue(good["execution_passed"])
        self.assertFalse(good["issue_accepted"])
        self.assertFalse(good["whole_suite_accepted"])
        self.assertFalse(good["historical_original_evidence_recovered"])
        for mutation in ["missing", "timeout", "count"]:
            changed = copy.deepcopy(targets)
            if mutation == "missing":
                del changed[14]
            elif mutation == "timeout":
                changed[14]["receipt"]["timed_out"] = True
            else:
                changed[14]["case_count"] -= 1
            self.assertFalse(runner.terminal_summary(changed)["execution_passed"])

    def test_main_attempts_remaining_targets_and_retains_first_failure(self):
        repository = Path(__file__).resolve().parents[2]
        proof = {"case_count": runner.CASE_COUNT,
                 "original_input_sha256": "34a18754668b79baa71a16314d4eac07209c9a03111850546a78cc29ff711637",
                 "published_sources": [{}] * 729}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "fresh"
            argv = ["runner", "--repository", str(repository), "--output", str(output),
                    "--python312", "/python312", "--python313", "/python313", "--python314", "/python314"]
            with (patch.object(sys, "argv", argv), patch.object(sys, "platform", "linux"),
                  patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ARCH": "X64"}),
                  patch("q0_mypy_inputs.prepare_inputs", return_value=(self.cases, proof)),
                  patch.object(runner, "download"),
                  patch.object(runner, "prepare", side_effect=[ValueError("first real failure"), Path("/p13"), Path("/p14")]) as prepare,
                  patch.object(runner, "run_target", return_value={"status": "failed", "reason": "control"}) as target,
                  patch("sys.stdout", new_callable=io.StringIO)):
                self.assertEqual(runner.main(), 1)
            self.assertEqual(prepare.call_count, 3)
            self.assertEqual(target.call_count, 2)
            first = json.loads((output / "python312-failure.json").read_bytes())
            self.assertEqual(first["message"], "first real failure")
            self.assertFalse(json.loads((output / "summary.json").read_bytes())["execution_passed"])


if __name__ == "__main__":
    unittest.main()
