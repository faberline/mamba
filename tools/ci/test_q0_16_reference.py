"""Mocked capture and metadata controls; never import or run target packages."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_16_reference as reference
from q0_16_packet import digest, validate_packet
from test_q0_16_packet import PROFILE, case, packet


def receipt(exit_code=0, **changes):
    result = {"pid": 123, "waited_pid": 123, "wait_completed": True, "actual_exit": exit_code,
              "timed_out": False, "launch_error": None, "passed": True,
              "stdout_sha256": digest(b""), "stderr_sha256": digest(b"")}
    result.update(changes)
    return result


def mypy_row(name="main.py", rule="misc", severity="error"):
    return {"file": name, "code": rule, "message": "retained exact message", "hint": None,
            "severity": severity, "line": 1, "column": 0, "end_line": 1, "end_column": -1}


class ReferenceControls(unittest.TestCase):
    def test_control_imports_do_not_load_reference_packages(self):
        targets = {"mypy", "pylint", "astroid", "pydantic", "pydantic_core", "librt", "ast_serialize"}
        self.assertFalse({name.split(".")[0] for name in sys.modules} & targets)

    def test_mypy_projection_keeps_global_stream_order_and_end_columns(self):
        raw = b"".join((json.dumps(item) + "\n").encode() for item in
                       [mypy_row("a.py", "one"), mypy_row("b.py", "two"), mypy_row("a.py", "three")])
        rows = reference.ordered_mypy_diagnostics(raw, {"a.py", "b.py"}, Path("/isolated"))
        self.assertEqual([item["rule"] for item in rows], ["one", "two", "three"])
        self.assertEqual(rows[0]["location"]["column"], 1)
        self.assertEqual(rows[0]["location"]["end_column"], -1)
        self.assertEqual(rows[0]["message"], "retained exact message")

    def test_mypy_projection_rejects_unknown_and_external_paths(self):
        for filename in ("other.py", "/outside/main.py"):
            with self.subTest(filename=filename), self.assertRaises(ValueError):
                reference.ordered_mypy_diagnostics((json.dumps(mypy_row(filename)) + "\n").encode(),
                                                   {"main.py"}, Path("/isolated"))

    def test_exit_rules_use_all_diagnostics_and_do_not_accept_usage_status(self):
        self.assertEqual(reference.expected_exit("mypy", [{"severity": "note"}]), 0)
        self.assertEqual(reference.expected_exit("mypy", [{"severity": "error"}]), 1)
        messages = [{"rule": "F0001", "severity": "fatal"}, {"rule": "E0602", "severity": "error"},
                    {"rule": "W0001", "severity": "warning"}, {"rule": "R0001", "severity": "refactor"},
                    {"rule": "C0001", "severity": "convention"}, {"rule": "I0001", "severity": "info"}]
        self.assertEqual(reference.expected_exit("pylint", messages), 31)
        with self.assertRaises(ValueError):
            reference.expected_exit("pylint", [{"rule": "E0602", "severity": "warning"}])
        with self.assertRaises(ValueError):
            reference.require_receipt(receipt(32), tuple(range(32)))

    def test_pylint_suppression_changes_only_its_declared_argument(self):
        value = case("pylint")
        baseline = reference.argv_for(Path("/venv/python"), "pylint", value, Path("/work"), 12, [])
        self.assertIn("-B", baseline)
        self.assertFalse(any(item.startswith(("--enable", "--disable")) for item in baseline))
        value["disabled_rules"] = ["E0602"]
        controlled = reference.argv_for(Path("/venv/python"), "pylint", value, Path("/other"), 12, [])
        controlled.remove("--disable=E0602")
        self.assertEqual(controlled, baseline)

    def capture_case(self, temporary, *, stdout=b"", stderr=b"", actual_exit=0, changes=None, mutate=False):
        value = case(role="non_trigger")
        proof = validate_packet(packet(cases=[value]), PROFILE)["cases"][0]
        root = Path(temporary).resolve() / "case"
        def fake_capture(argv, cwd, output, **options):
            output.mkdir()
            (output / "stdout.raw").write_bytes(stdout)
            (output / "stderr.raw").write_bytes(stderr)
            observed = receipt(actual_exit, stdout_sha256=digest(stdout), stderr_sha256=digest(stderr), **(changes or {}))
            (output / "receipt.json").write_text(json.dumps(observed))
            if mutate:
                (cwd / "main.py").write_text("# changed after capture\n")
            return observed
        with patch.object(reference, "run_command", side_effect=fake_capture):
            result = reference.run_case("mypy", value, proof, {"sha256": "packet"}, Path("/not-executed"),
                                        {"identity_sha256": "environment"}, root, 12, [])
        return root, result

    def test_absence_predicate_requires_a_real_terminal_receipt(self):
        for changes in ({"wait_completed": False}, {"timed_out": True}, {"waited_pid": 124},
                        {"pid": None}, {"actual_exit": True}, {"launch_error": {"type": "OSError"}}):
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary:
                root, row = self.capture_case(temporary, changes=changes)
                self.assertEqual(row["status"], "failed")
                self.assertFalse(row["observation_complete"])
                self.assertTrue((root / "command/receipt.json").is_file())

    def test_successful_absence_capture_stays_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, row = self.capture_case(temporary)
            self.assertEqual(row["status"], "passed")
            self.assertTrue(row["observation_complete"])
            self.assertFalse(row["issue_accepted"])
            self.assertEqual(row["native_parity"], "not_run")

    def test_actual_exit_cannot_contradict_diagnostic_severity(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, row = self.capture_case(temporary, stdout=(json.dumps(mypy_row()) + "\n").encode(), actual_exit=0)
            self.assertEqual(row["status"], "failed")
            self.assertEqual(row["expected_exit_from_observed_diagnostics"], 1)

    def test_absence_only_control_cannot_pass_an_unrelated_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, row = self.capture_case(temporary,
                stdout=(json.dumps(mypy_row(rule="syntax")) + "\n").encode(), actual_exit=1)
            self.assertTrue(row["observation_complete"])
            self.assertTrue(all(item["passed"] for item in row["predicate_results"]))
            self.assertEqual(row["status"], "failed")
            self.assertEqual(row["clean_absence_control"], {"required": True, "passed": False})
            self.assertEqual(json.loads((root / "command/receipt.json").read_bytes())["actual_exit"], 1)

    def test_stderr_and_changed_inputs_remain_failures_with_raw_facts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, row = self.capture_case(temporary, stderr=b"unexpected warning\n")
            self.assertEqual(row["status"], "failed")
            self.assertEqual((root / "command/stderr.raw").read_bytes(), b"unexpected warning\n")
        with tempfile.TemporaryDirectory() as temporary:
            root, row = self.capture_case(temporary, mutate=True)
            self.assertEqual(row["status"], "failed")
            self.assertFalse(row["source_files_unchanged"])
            self.assertNotEqual(json.loads((root / "source-after.json").read_bytes()),
                                json.loads((root / "source-before.json").read_bytes()))

    def test_only_known_venv_trees_are_excluded_from_artifact_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            excluded = root / "environments/mypy/venv"
            excluded.mkdir(parents=True)
            (excluded / "excluded.py").write_text("# excluded package bytes\n")
            included = root / "cases/example/work/venv"
            included.mkdir(parents=True)
            (included / "kept.py").write_text("# declared case input\n")
            link = root / "captured-link"
            link.symlink_to(included, target_is_directory=True)
            result = reference.artifact_inventory(root, [excluded])
            self.assertNotIn("environments/mypy/venv/excluded.py", result)
            self.assertIn("cases/example/work/venv/kept.py", result)
            self.assertEqual(result["captured-link"]["kind"], "symlink")
            self.assertNotIn("captured-link/kept.py", result)

    def test_installed_record_raw_bytes_are_kept_even_after_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            metadata = root / "venv/lib/python3.12/site-packages/example-1.dist-info"
            metadata.mkdir(parents=True)
            record = metadata / "RECORD"
            original = b"example.py,sha256=example,12\nexample-1.dist-info/RECORD,,\n"
            record.write_bytes(original)
            identity = {"distributions": {"example": {"metadata_path": str(metadata),
                "distribution_files": {"example-1.dist-info/RECORD": reference.fact(record)}}}}
            before = reference.capture_installed_records(root, identity, "mypy", "before")
            self.assertTrue(before["all_sources_match"])
            self.assertEqual((root / "installed-records-before/example.RECORD").read_bytes(), original)
            record.write_bytes(original + b"changed.py,,\n")
            after = reference.capture_installed_records(root, identity, "mypy", "after")
            self.assertFalse(after["all_sources_match"])
            self.assertEqual((root / "installed-records-after/example.RECORD").read_bytes(), record.read_bytes())
            with self.assertRaises(FileExistsError):
                reference.capture_installed_records(root, identity, "mypy", "after")

    def test_pylint_record_capture_uses_site_when_probe_has_no_metadata_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            site = root / "venv/lib/python3.12/site-packages"
            module = site / "pylint/__init__.py"
            module.parent.mkdir(parents=True)
            module.write_text("# never imported\n")
            record = site / "pylint-4.1.2.dist-info/RECORD"
            record.parent.mkdir()
            record.write_bytes(b"pylint-4.1.2.dist-info/RECORD,,\n")
            identity = {"modules": {"pylint": {"path": str(module)}},
                        "distributions": {"pylint": {"distribution_files": {
                            "pylint-4.1.2.dist-info/RECORD": reference.fact(record)}}}}
            captured = reference.capture_installed_records(root, identity, "pylint", "before")
            self.assertTrue(captured["all_sources_match"])
            self.assertEqual(captured["copied_record_count"], 1)
            self.assertEqual((root / "installed-records-before/pylint.RECORD").read_bytes(), record.read_bytes())

    def test_same_input_control_does_not_pass_a_failed_hit_partner(self):
        hit, control = case("pylint", ident="hit"), case("pylint", "non_trigger", "control")
        control["disabled_rules"] = ["E0602"]
        value = packet("pylint", [hit, control])
        proof = validate_packet(value, PROFILE)
        inputs = {"files": {"main.py": {"sha256": "same"}}, "entrypoints": ["main.py"]}
        baseline = {"tool": "pylint", "id": "hit", "status": "failed", "argv": ["python", "main.py"],
                    "input_proof": inputs, "source_files_unchanged": True}
        negative = {"tool": "pylint", "id": "control", "status": "passed", "argv": ["python", "--disable=E0602", "main.py"],
                    "input_proof": inputs, "source_files_unchanged": True, "errors": []}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve()
            for row in (baseline, negative):
                folder = output / "cases/pylint" / row["id"]
                folder.mkdir(parents=True)
                reference.write_json(folder / "result.json", row)
            initial = output / "cases/pylint/control/result.json"
            initial_bytes = initial.read_bytes()
            rows = [baseline, negative]
            reference.publish_case_results(rows, [{"packet": value, "proof": proof}], output)
            self.assertEqual(negative["status"], "failed")
            self.assertFalse(negative["paired_control"]["paired_control_passed"])
            self.assertEqual(initial.read_bytes(), initial_bytes)
            self.assertEqual(json.loads(initial_bytes)["status"], "passed")
            self.assertEqual(negative["initial_result_pointer"], "cases/pylint/control/result.json")
            self.assertEqual(negative["result_pointer"], "cases/pylint/control/final-result.json")
            self.assertEqual(negative["final_result_pointer"], negative["result_pointer"])
            self.assertEqual(json.loads((output / negative["result_pointer"]).read_bytes()), negative)
            self.assertEqual(json.loads((output / "case-results.json").read_bytes()), rows)
            self.assertEqual([json.loads(line) for line in (output / "raw-index.jsonl").read_bytes().splitlines()], rows)

    def test_preparation_failure_remains_not_run_in_paired_control_index(self):
        hit, control = case("pylint", ident="hit"), case("pylint", "non_trigger", "control")
        control["disabled_rules"] = ["E0602"]
        value = packet("pylint", [hit, control])
        proof = validate_packet(value, PROFILE)
        baseline = reference.not_run_row("pylint", hit, proof["cases"][0], {}, 12, "preparation failed")
        negative = reference.not_run_row("pylint", control, proof["cases"][1], {}, 12, "preparation failed")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve()
            rows = [baseline, negative]
            reference.publish_case_results(rows, [{"packet": value, "proof": proof}], output)
            self.assertEqual(negative["status"], "not_run")
            self.assertFalse(negative["process_launched"])
            self.assertFalse(negative["paired_control"]["paired_control_passed"])
            for row in rows:
                self.assertIsNone(row["initial_result_pointer"])
                self.assertIsNone(row["initial_result_sha256"])
                self.assertEqual(json.loads((output / row["final_result_pointer"]).read_bytes()), row)
                self.assertEqual(row["status"], "not_run")
            self.assertEqual(json.loads((output / "case-results.json").read_bytes()), rows)
            self.assertEqual([json.loads(line) for line in (output / "raw-index.jsonl").read_bytes().splitlines()], rows)

    def test_final_results_never_overwrite_initial_or_prior_final_bytes(self):
        row = {"tool": "mypy", "id": "one", "status": "passed"}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve()
            folder = output / "cases/mypy/one"
            folder.mkdir(parents=True)
            initial = folder / "result.json"
            reference.write_json(initial, row)
            initial_bytes = initial.read_bytes()
            reference.write_final_results([row], output)
            final = output / row["final_result_pointer"]
            final_bytes = final.read_bytes()
            row["status"] = "failed"
            with self.assertRaises(FileExistsError):
                reference.write_final_results([row], output)
            self.assertEqual(initial.read_bytes(), initial_bytes)
            self.assertEqual(final.read_bytes(), final_bytes)

    def test_nonhosted_execution_stops_before_any_download_or_preparation(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve() / "run"
            with patch.object(reference.pylint_reference, "require_host_context", side_effect=ValueError("not hosted")), \
                    patch.object(reference, "download") as download, \
                    patch.object(reference.pydantic_reference, "prepare") as prepare:
                code = reference.run(Path("/not-read"), output, Path("/not-executed"), "3.12.11")
            self.assertEqual(code, 1)
            download.assert_not_called()
            prepare.assert_not_called()
            summary = json.loads((output / "summary.json").read_bytes())
            self.assertFalse(summary["supplement_capture_passed"])
            self.assertEqual(summary["planned_case_count"], 0)
            self.assertFalse(summary["issue_accepted"])


if __name__ == "__main__":
    unittest.main()
