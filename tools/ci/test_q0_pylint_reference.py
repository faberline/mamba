"""Pure Pylint shard controls. Mock originals; only harmless Python may run."""
from __future__ import annotations

import copy
import importlib.util
import io
import json
import os
import py_compile
from pathlib import Path
import sys
import tempfile
import time
import types
import unittest
from unittest.mock import patch

import q0_pylint_reference as runner


def case(index=0):
    files = {"main.py": "x = 1\n", "nested/extra.py": "y = 2\n", "native.so": "not a library\n",
             "native.pyd": "pyd\n", "native.dylib": "dylib\n", "notes.txt": "text\n", "module.pyi": "x: int\n"}
    return {"index": index, "name": f"control-{index}", "files": files,
            "source_sha256": {name: runner.digest(source.encode()) for name, source in files.items()}}


def expected(input_case, diagnostics=None, exit_code=0):
    return {**input_case, "expected": [diagnostics or []] * 3, "reference_exits": [exit_code] * 3}


def receipt(argv, project, exit_code=0):
    return {"pid": 42, "waited_pid": 42, "wait_completed": True, "actual_exit": exit_code,
            "timed_out": False, "launch_error": None, "passed": exit_code in runner.REFERENCE_EXITS,
            "argv": argv, "cwd": str(project)}


def environment_identity(minor=12):
    return {"identity": "control", "cache_tag": f"cpython-3{minor}",
            "magic_number_hex": importlib.util.MAGIC_NUMBER.hex()}


def fake_helper():
    helper = types.ModuleType("q0_pylint_inputs")
    def materialize(input_case, project):
        project.mkdir(parents=True, exist_ok=False)
        for name, source in input_case["files"].items():
            path = project / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(source.encode())
        return {"files": runner.inventory(project),
                "python_files": sorted(name for name in input_case["files"] if name.endswith(".py"))}
    helper.materialize_case = materialize
    helper.project_diagnostics = lambda raw, project: json.loads(raw)
    return helper


class Partition(unittest.TestCase):
    def test_global_indices_are_not_renumbered_and_cover_every_case(self):
        cases = [case(i) for i in range(11)]
        with patch.object(runner, "CASE_COUNT", 11):
            selected = [runner.select_shard(cases, i, 4) for i in range(4)]
        self.assertEqual([[c["index"] for c in rows] for rows in selected], [[0, 4, 8], [1, 5, 9], [2, 6, 10], [3, 7]])
        self.assertEqual(sorted(c["index"] for rows in selected for c in rows), list(range(11)))

    def test_fixed_partition_rejects_partial_duplicate_or_reordered_inputs(self):
        cases = [case(i) for i in range(4)]
        changed = [cases[:-1], [cases[0], cases[0], *cases[2:]], list(reversed(cases))]
        with patch.object(runner, "CASE_COUNT", 4):
            for rows in changed:
                with self.assertRaisesRegex(ValueError, "coverage differs"):
                    runner.select_shard(rows, 0, 4)
            for shard, count in [(-1, 4), (4, 4), (0, 3)]:
                with self.assertRaises(ValueError):
                    runner.select_shard(cases, shard, count)

    def test_argv_is_exact_and_has_only_sorted_relative_py_entries(self):
        result = runner.reference_argv(Path("/venv/bin/python"), ["a.py", "nested/b.py"], 13)
        self.assertEqual(result, ["/venv/bin/python", "-I", "-m", "pylint", "--rcfile=/dev/null", "--persistent=n",
                                  "--max-line-length=88", "--py-version=3.13", "--output-format=json2", "a.py", "nested/b.py"])
        for names in [[], ["b.py", "a.py"], ["a.py", "a.py"], ["/a.py"], ["../a.py"], ["a.pyi"]]:
            with self.assertRaises(ValueError):
                runner.reference_argv(Path("/python"), names, 13)

    def test_non_python_fixture_bytes_are_part_of_the_manifest(self):
        input_case = case()
        manifest = runner.source_manifest(input_case)
        self.assertEqual(set(manifest), set(input_case["files"]))
        self.assertEqual(len(manifest), 7)
        tampered = copy.deepcopy(input_case)
        tampered["files"]["native.so"] += "changed"
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            runner.source_manifest(tampered)


class PackageIdentity(unittest.TestCase):
    def identity(self, venv):
        pins = [{"name": name, "version": version} for name, version in runner.PINNED_VERSIONS.items()]
        distributions = {name: {"name": name, "version": version, "requires_dist": [],
                               "distribution_files": {"payload": {"bytes": 1, "sha256": "a" * 64}},
                               "missing_distribution_files": []}
                         for name, version in runner.PINNED_VERSIONS.items()}
        distributions["pylint"]["requires_dist"] = ["astroid>=4.3.3,<4.4", "isort>=9", "dill>=0.4"]
        distributions["isort"]["requires_dist"] = ["mypy-extensions>=1.1", 'colorama; extra == "colors"']
        markers = {"python_full_version": "3.12.11", "python_version": "3.12", "sys_platform": "darwin",
                   "platform_system": "Darwin", "platform_machine": "arm64"}
        modules = {}
        for name in ["pylint", "astroid"]:
            path = venv / (name + ".bin")
            path.write_bytes(b"harmless identity payload")
            modules[name] = {"path": str(path), "sha256": runner.digest(path.read_bytes())}
        rules = [f"C{i:04}" for i in range(383)]
        identity = {"python": [3, 12, 11], "isolated": 1, "pylint": "4.1.2", "astroid": "4.3.3",
                    "cache_tag": "cpython-312", "magic_number_hex": importlib.util.MAGIC_NUMBER.hex(),
                    "rules": rules, "modules": modules, "distributions": distributions,
                    "dependency_verification": runner.validate_dependencies(pins, distributions, markers)}
        return identity, pins, rules

    def test_exact_identity_and_inactive_extras(self):
        with tempfile.TemporaryDirectory() as temporary:
            venv = Path(temporary)
            identity, pins, rules = self.identity(venv)
            runner.require_package_identity(identity, "3.12.11", pins, rules, venv)
            self.assertEqual(len(identity["dependency_verification"]["inactive_requirements"]), 1)

    def test_fixture_import_pin_is_required_when_core_marker_is_inactive(self):
        with tempfile.TemporaryDirectory() as temporary:
            venv = Path(temporary)
            identity, pins, rules = self.identity(venv)
            identity["distributions"]["pylint"]["requires_dist"].append('typing-extensions>=4.10; python_version < "3.11"')
            markers = identity["dependency_verification"]["marker_environment"]
            identity["dependency_verification"] = runner.validate_dependencies(pins, identity["distributions"], markers)
            self.assertTrue(any(row["name"] == "typing-extensions" for row in identity["dependency_verification"]["inactive_requirements"]))
            runner.require_package_identity(identity, "3.12.11", pins, rules, venv)
            omitted = [pin for pin in pins if pin["name"] != "typing_extensions"]
            # Core constraints alone would pass. The full producer environment must not.
            runner.validate_dependencies(omitted, identity["distributions"], markers)
            with self.assertRaisesRegex(ValueError, "fixture-import pins differ"):
                runner.require_package_identity(identity, "3.12.11", omitted, rules, venv)
            for change in ["missing", "version", "payload"]:
                altered = copy.deepcopy(identity)
                if change == "missing":
                    del altered["distributions"]["typing_extensions"]
                elif change == "version":
                    altered["distributions"]["typing_extensions"]["version"] = "4.15.0"
                else:
                    altered["distributions"]["typing_extensions"]["missing_distribution_files"] = ["typing_extensions.py"]
                with self.assertRaises(ValueError):
                    runner.require_package_identity(altered, "3.12.11", pins, rules, venv)

    def test_wrong_patch_isolation_target_marker_or_rule_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            venv = Path(temporary)
            identity, pins, rules = self.identity(venv)
            for change in ["patch", "isolation", "marker", "rule", "cache_tag", "magic_number_hex",
                           "sys_platform", "platform_system", "platform_machine"]:
                altered = copy.deepcopy(identity)
                if change == "patch":
                    altered["python"][-1] = 10
                elif change == "isolation":
                    altered["isolated"] = 0
                elif change == "marker":
                    altered["dependency_verification"]["marker_environment"]["python_full_version"] = "3.13.5"
                elif change in {"cache_tag", "magic_number_hex"}:
                    altered[change] = "incorrect"
                elif change in {"sys_platform", "platform_system", "platform_machine"}:
                    altered["dependency_verification"]["marker_environment"][change] = {
                        "sys_platform": "linux", "platform_system": "Linux", "platform_machine": "x86_64"}[change]
                else:
                    altered["rules"][-1] = "unapproved"
                with self.assertRaises(ValueError):
                    runner.require_package_identity(altered, "3.12.11", pins, rules, venv)

    def test_missing_wrong_version_or_partial_dependency_payload_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            venv = Path(temporary)
            identity, pins, rules = self.identity(venv)
            for change in ["missing", "version", "payload"]:
                altered = copy.deepcopy(identity)
                if change == "missing":
                    del altered["distributions"]["mypy_extensions"]
                elif change == "version":
                    altered["distributions"]["astroid"]["version"] = "4.3.2"
                else:
                    altered["distributions"]["dill"]["missing_distribution_files"] = ["lost.py"]
                with self.assertRaises(ValueError):
                    runner.require_package_identity(altered, "3.12.11", pins, rules, venv)

    def test_installed_module_hash_tampering_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            venv = Path(temporary)
            identity, pins, rules = self.identity(venv)
            Path(identity["modules"]["pylint"]["path"]).write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "module identity"):
                runner.require_package_identity(identity, "3.12.11", pins, rules, venv)


class CaseExecution(unittest.TestCase):
    def run_control(self, root, input_case, frozen, *, diagnostics=None, exit_code=0, mutate=False, timeout=False, cache=False):
        comparator = runner.PylintComparator(Path(__file__).resolve().parents[2])
        def execute(argv, project, output, **options):
            output.mkdir(parents=True)
            (output / "stdout.raw").write_bytes(json.dumps(diagnostics or []).encode())
            (output / "stderr.raw").write_bytes(b"raw stderr control")
            if mutate:
                (project / "native.so").write_bytes(b"changed")
            if cache:
                destination = project / "nested/__pycache__/extra.cpython-312.pyc"
                destination.parent.mkdir(parents=True)
                py_compile.compile(str(project / "nested/extra.py"), cfile=str(destination), doraise=True)
            record = receipt(argv, project, exit_code)
            if timeout:
                record.update(timed_out=True, actual_exit=-9, passed=False)
            return record
        with (patch.dict(sys.modules, {"q0_pylint_inputs": fake_helper()}), patch.object(runner, "run_command", side_effect=execute) as command):
            result = runner.run_case(Path("/venv/bin/python"), root, input_case, frozen, 12, comparator, environment_identity())
        return result, command

    def test_valid_interpreter_cache_is_retained_without_source_change(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "results").mkdir()
            input_case = case()
            frozen = expected(input_case)
            result, _ = self.run_control(root, input_case, frozen, cache=True)
            cache_name = "nested/__pycache__/extra.cpython-312.pyc"
            self.assertEqual(result["status"], "passed")
            self.assertTrue(result["fixture_files_unchanged"])
            self.assertTrue(result["source_unchanged"])
            self.assertFalse(result["project_tree_unchanged"])
            self.assertIn(cache_name, result["source_after"])
            self.assertNotIn(cache_name, result["source_before"])
            self.assertEqual(result["generated_cache_files"][cache_name]["source"], "nested/extra.py")
            self.assertEqual(result["generated_cache_files"][cache_name]["magic_number_hex"], importlib.util.MAGIC_NUMBER.hex())
            self.assertEqual((root / "case_0000" / cache_name).read_bytes()[:4], importlib.util.MAGIC_NUMBER)
            self.assertIn("case_0000/" + cache_name, runner.inventory(root))
            runner.require_shard_results([input_case], [result], [frozen], 12, Path("/venv/bin/python"), root, environment_identity())

    def test_unknown_or_invalid_cache_and_changed_or_missing_fixture_fail(self):
        for change in ["tag", "magic", "nonsource", "arbitrary", "changed", "removed"]:
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                project = Path(temporary) / "case_0000"
                input_case = case()
                fake_helper().materialize_case(input_case, project)
                before = runner.inventory(project)
                cache_name = {"tag": "__pycache__/main.cpython-313.pyc",
                              "magic": "__pycache__/main.cpython-312.pyc",
                              "nonsource": "__pycache__/unknown.cpython-312.pyc",
                              "arbitrary": "new.txt"}.get(change)
                if cache_name:
                    path = project / cache_name
                    path.parent.mkdir(exist_ok=True)
                    magic = b"wrong" if change == "magic" else importlib.util.MAGIC_NUMBER
                    path.write_bytes(magic + b"harmless cache control")
                elif change == "changed":
                    (project / "native.so").write_bytes(b"changed")
                else:
                    (project / "native.so").unlink()
                state = runner.fixture_file_state(input_case, before, runner.inventory(project), project, environment_identity(), 12)
                self.assertFalse(state["project_files_acceptable"])
                if change in {"changed", "removed"}:
                    self.assertFalse(state["fixture_files_unchanged"])
                else:
                    self.assertTrue(state["fixture_files_unchanged"])
                    self.assertFalse(state["project_tree_unchanged"])

    def test_one_case_one_fresh_command_and_all_fixture_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "results").mkdir()
            input_case = case(8)
            result, command = self.run_control(root, input_case, expected(input_case))
            self.assertEqual(command.call_count, 1)
            self.assertEqual(result["status"], "passed")
            self.assertEqual(Path(result["project"]).name, "case_0008")
            self.assertEqual(set(result["source_after"]), set(input_case["files"]))
            self.assertFalse((root / "case_0008/pyproject.toml").exists())
            args = command.call_args
            self.assertEqual(args.args[2], root / "commands/case_0008")
            self.assertEqual(args.kwargs["timeout"], 120)
            self.assertEqual(args.args[0][-2:], ["main.py", "nested/extra.py"])

    def test_expected_fatal_is_a_comparison_pass_with_nonzero_program_exit(self):
        diagnostic = {"rule": "F0010", "message": "fatal control", "hint": None, "path": "main.py", "severity": "fatal",
                      "location": {"line": 1, "column": 0, "end_line": None, "end_column": None}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "results").mkdir()
            input_case = case()
            result, _ = self.run_control(root, input_case, expected(input_case, [diagnostic], 1), diagnostics=[diagnostic], exit_code=1)
            self.assertEqual(result["status"], "passed")
            self.assertEqual(result["reference_outcome"], "fatal")
            self.assertFalse(result["program_success"])

    def test_non_python_source_tampering_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "results").mkdir()
            input_case = case()
            result, _ = self.run_control(root, input_case, expected(input_case), mutate=True)
            self.assertEqual(result["status"], "failed")
            self.assertFalse(result["source_unchanged"])
            self.assertIn("SourceChanged", [failure["error_class"] for failure in result["failures"]])

    def test_timeout_or_usage_exit_cannot_be_a_comparison_pass(self):
        for exit_code, timeout in [(0, True), (32, False), (-9, False)]:
            with self.subTest(exit_code=exit_code, timeout=timeout), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "results").mkdir()
                input_case = case()
                result, _ = self.run_control(root, input_case, expected(input_case), exit_code=exit_code, timeout=timeout)
                self.assertEqual(result["status"], "failed")
                self.assertIsNone(result["comparison"])
                self.assertTrue(result["source_unchanged"])
                self.assertEqual((root / "commands/case_0000/stderr.raw").read_bytes(), b"raw stderr control")

    def test_actual_exit_must_equal_frozen_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "results").mkdir()
            input_case = case()
            result, _ = self.run_control(root, input_case, expected(input_case), exit_code=1)
            self.assertEqual(result["receipt"]["actual_exit"], 1)
            self.assertEqual(result["status"], "failed")
            self.assertFalse(result["comparison"]["matched"])

    def test_worker_failure_does_not_stop_other_cases_and_output_order_is_global(self):
        selected = [case(i) for i in [0, 4, 8]]
        called = []
        def execute(executable, root, input_case, frozen, minor, comparator, identity):
            called.append(input_case["index"])
            if input_case["index"] == 4:
                raise ValueError("first worker failure")
            if input_case["index"] == 0:
                time.sleep(0.02)
            return {"index": input_case["index"], "name": input_case["name"], "status": "passed"}
        with tempfile.TemporaryDirectory() as temporary, patch.object(runner, "run_case", side_effect=execute):
            rows = runner.execute_cases(Path("/python"), Path(temporary), selected, [None] * 9, 12, None, {})
        self.assertEqual(sorted(called), [0, 4, 8])
        self.assertEqual([row["index"] for row in rows], [0, 4, 8])
        self.assertEqual(rows[1]["failures"][0]["message"], "first worker failure")
        self.assertEqual(rows[-1]["status"], "passed")


class Coverage(unittest.TestCase):
    def test_only_exact_host_and_lock_context_are_permitted(self):
        environment = {"GITHUB_ACTIONS": "true", "RUNNER_OS": "macOS", "RUNNER_ARCH": "ARM64"}
        with patch.object(sys, "platform", "darwin"), patch.dict(os.environ, environment):
            runner.require_host_context(runner.HOST_CONTEXT)
            for key, value in [("runner_label", "macos-15-intel"), ("runner_arch", "X64"), ("runner_os", "Linux")]:
                changed = {**runner.HOST_CONTEXT, key: value}
                with self.assertRaisesRegex(ValueError, "locked host context differs"):
                    runner.require_host_context(changed)
        for key, value in [("GITHUB_ACTIONS", "false"), ("RUNNER_OS", "Linux"), ("RUNNER_ARCH", "X64")]:
            with patch.object(sys, "platform", "darwin"), patch.dict(os.environ, {**environment, key: value}):
                with self.assertRaisesRegex(ValueError, "macOS ARM64"):
                    runner.require_host_context(runner.HOST_CONTEXT)
        with patch.object(sys, "platform", "linux"), patch.dict(os.environ, environment):
            with self.assertRaisesRegex(ValueError, "macOS ARM64"):
                runner.require_host_context(runner.HOST_CONTEXT)

    def test_non_hosted_run_fails_before_download_and_claims_no_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "fresh"
            argv = ["runner", "--repository", str(Path(__file__).resolve().parents[2]), "--output", str(root),
                    "--python", sys.executable, "--python-version", "3.12.11", "--shard-index", "0", "--shard-count", "4"]
            with (patch.object(sys, "argv", argv), patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}),
                  patch.object(runner, "download", side_effect=AssertionError("must not download")) as download,
                  patch("sys.stdout", new_callable=io.StringIO)):
                self.assertEqual(runner.main(), 1)
            download.assert_not_called()
            summary = json.loads((root / "summary.json").read_bytes())
            self.assertFalse(summary["shard_execution_passed"])
            for key in ["full_reference_accepted", "issue_accepted", "whole_suite_accepted", "historical_original_evidence_recovered"]:
                self.assertFalse(summary[key])

    def setup_rows(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / "python312"
        root.mkdir()
        executable = root / "venv/bin/python"
        selected = [case(0), case(4)]
        published = [expected(case(i)) for i in range(5)]
        rows = []
        for input_case in selected:
            project = root / f"case_{input_case['index']:04}"
            fake_helper().materialize_case(input_case, project)
            before = runner.inventory(project)
            row = {"index": input_case["index"], "name": input_case["name"], "source_sha256": input_case["source_sha256"],
                         "status": "passed", "failures": [], "source_unchanged": True,
                         "source_before": before, "source_after": before,
                         "environment_identity": environment_identity(),
                         "receipt": receipt(runner.reference_argv(executable, ["main.py", "nested/extra.py"], 12), project),
                         "comparison": {"matched": True, "matched_by": "canonical", "actual_exit": 0, "reference_exit": 0}}
            row.update(runner.fixture_file_state(input_case, before, before, project, environment_identity(), 12))
            rows.append(row)
        return selected, rows, published, executable, root

    def test_complete_shard_receipts_and_sources_are_required(self):
        selected, rows, published, executable, root = self.setup_rows()
        runner.require_shard_results(selected, rows, published, 12, executable, root, environment_identity())
        for altered in [[], rows[:-1], [rows[0], rows[0]], list(reversed(rows))]:
            with self.assertRaises(ValueError):
                runner.require_shard_results(selected, altered, published, 12, executable, root, environment_identity())

    def test_missing_wait_pid_exit_and_timeout_are_rejected(self):
        selected, rows, published, executable, root = self.setup_rows()
        for key, value in [("pid", None), ("wait_completed", False), ("actual_exit", None), ("timed_out", True), ("launch_error", {"error": "failed"})]:
            altered = copy.deepcopy(rows)
            altered[0]["receipt"][key] = value
            with self.assertRaises(ValueError):
                runner.require_shard_results(selected, altered, published, 12, executable, root, environment_identity())

    def test_wrong_command_cwd_source_and_comparison_are_rejected(self):
        selected, rows, published, executable, root = self.setup_rows()
        for mutation in ["argv", "cwd", "source", "comparison"]:
            altered = copy.deepcopy(rows)
            if mutation == "argv":
                altered[0]["receipt"]["argv"].append("--jobs=4")
            elif mutation == "cwd":
                altered[0]["receipt"]["cwd"] = "/other"
            elif mutation == "source":
                altered[0]["source_after"]["native.so"]["sha256"] = "wrong"
            else:
                altered[0]["comparison"]["matched"] = False
            with self.assertRaises(ValueError):
                runner.require_shard_results(selected, altered, published, 12, executable, root, environment_identity())

    def test_collector_rechecks_files_and_recorded_cache_metadata(self):
        selected, rows, published, executable, root = self.setup_rows()
        project = root / "case_0000"
        path = project / "__pycache__/main.cpython-312.pyc"
        path.parent.mkdir()
        py_compile.compile(str(project / "main.py"), cfile=str(path), doraise=True)
        rows[0]["source_after"] = runner.inventory(project)
        rows[0].update(runner.fixture_file_state(selected[0], rows[0]["source_before"], rows[0]["source_after"], project, environment_identity(), 12))
        runner.require_shard_results(selected, rows, published, 12, executable, root, environment_identity())
        for change in ["cache", "tree", "fixture", "environment"]:
            altered = copy.deepcopy(rows)
            if change == "cache":
                altered[0]["generated_cache_files"]["__pycache__/main.cpython-312.pyc"]["source"] = "other.py"
            elif change == "tree":
                altered[0]["project_tree_unchanged"] = True
            elif change == "fixture":
                altered[0]["fixture_files_unchanged"] = False
            else:
                altered[0]["environment_identity"]["magic_number_hex"] = "00000000"
            with self.assertRaises(ValueError):
                runner.require_shard_results(selected, altered, published, 12, executable, root, environment_identity())
        (project / "native.so").unlink()
        with self.assertRaises(ValueError):
            runner.require_shard_results(selected, rows, published, 12, executable, root, environment_identity())

    def test_benign_process_receipt_retains_raw_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = runner.run_command([sys.executable, "-I", "-c", "import sys; print('control'); sys.exit(4)"],
                                        root, root / "process", expected_exits=runner.REFERENCE_EXITS)
            runner.require_receipt(result, runner.REFERENCE_EXITS)
            self.assertEqual(result["actual_exit"], 4)
            self.assertEqual((root / "process/stdout.raw").read_bytes(), b"control\n")

    def test_benign_timeout_preserves_output_and_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = runner.run_command([sys.executable, "-I", "-c", "import time; print('first', flush=True); time.sleep(30)"],
                                        root, root / "process", expected_exits=runner.REFERENCE_EXITS, timeout=0.5)
            self.assertTrue(result["timed_out"])
            self.assertEqual((root / "process/stdout.raw").read_bytes(), b"first\n")
            with self.assertRaises(ValueError):
                runner.require_receipt(result, runner.REFERENCE_EXITS)
            with self.assertRaises(FileExistsError):
                runner.run_command([sys.executable, "-c", "pass"], root, root / "process")


if __name__ == "__main__":
    unittest.main()
