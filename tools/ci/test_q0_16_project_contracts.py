"""Pure named-project controls; original packages and case programs never run."""
from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_16_project_contracts as contracts

REPOSITORY = Path(__file__).resolve().parents[2]


def inputs():
    return contracts.load_inputs(REPOSITORY)


def raw_diagnostics(expected):
    return b"".join((json.dumps({"file": row["path"], "code": row["rule"], "message": row["message"],
        "hint": row["hint"], "severity": row["severity"], "line": row["location"]["line"],
        "column": row["location"]["column"] - 1, "end_line": row["location"]["end_line"],
        "end_column": row["location"]["end_column"]}) + "\n").encode() for row in expected) or b"\n"


def fake_command(stdout, actual_exit=1, mutation=None, saved="valid", stderr=b""):
    def capture(argv, cwd, output, **options):
        output.mkdir()
        contracts.write_json(output / "inputs.json", options["inputs"])
        (output / "stdin.raw").write_bytes(b"")
        (output / "stdout.raw").write_bytes(stdout); (output / "stderr.raw").write_bytes(stderr)
        record = {"argv": argv, "cwd": str(cwd), "expected_exits": options["expected_exits"],
            "started_utc": "2026-10-29T00:00:00+00:00", "pid": 123, "waited_pid": 123,
            "wait_completed": True, "actual_exit": actual_exit, "timed_out": False, "launch_error": None,
            "duration_seconds": 0.1, "stdout_sha256": contracts.digest(stdout), "stderr_sha256": contracts.digest(stderr),
            "stdin_sha256": contracts.digest(b""), "inputs_sha256": contracts.digest((output / "inputs.json").read_bytes()), "passed": True}
        if mutation: mutation(record, cwd)
        if saved == "valid": contracts.write_json(output / "receipt.json", record)
        elif saved == "corrupt": (output / "receipt.json").write_bytes(b"{invalid receipt")
        return record
    return capture


class ProjectContractControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.lock, cls.cases = inputs()

    def test_imports_do_not_load_target_packages(self):
        self.assertFalse({name.split(".")[0] for name in sys.modules} &
                         {"mypy", "pydantic", "pydantic_core", "pylint", "astroid", "ruff", "librt"})

    def test_all_575_source_and_1725_selected_target_values_bind_independently(self):
        self.assertEqual(len(self.cases), 575)
        self.assertEqual(sum(len(case["files"]) for case in self.cases), 1069)
        note_cells = 0
        for case in self.cases:
            binding = case["binding"]
            self.assertEqual(binding["input_reference"]["value_sha256"], contracts.value_digest(case["files"]))
            self.assertEqual(binding["source_files"], contracts.source_facts(case["files"]))
            for index, target in enumerate(binding["targets"]):
                self.assertEqual(target["python_version"], list(contracts.VERSIONS.values())[index])
                self.assertTrue(target["expected_reference"]["pointer"].endswith(f"/expected/{index}"))
                self.assertEqual(target["expected_reference"]["value_sha256"], contracts.value_digest(case["expected"][index]))
                self.assertEqual(target["exit_reference"]["value_sha256"], contracts.value_digest(case["reference_exits"][index]))
                note_cells += bool(case["expected"][index]) and all(row["severity"] == "note" for row in case["expected"][index])
        self.assertEqual(note_cells, 135)
        self.assertEqual(sum(case["entrypoints"] == ["."] for case in self.cases), 568)
        selected = [case for case in self.cases if case["entrypoints"] == ["main.py"]]
        self.assertEqual({case["name"] for case in selected}, contracts.SELECTED_NAMES)

    def test_lock_drift_target_ordinal_and_selected_pointer_swaps_cannot_pass(self):
        for alter in ("missing_case", "missing_target", "source_value", "expected_pointer", "exit_pointer", "selector"):
            changed = copy.deepcopy(self.lock)
            if alter == "missing_case": changed["cases"].pop()
            elif alter == "missing_target": changed["cases"][0]["targets"].pop()
            elif alter == "source_value": changed["cases"][0]["input_reference"]["value_sha256"] = "0" * 64
            elif alter == "expected_pointer": changed["cases"][0]["targets"][0]["expected_reference"]["pointer"] = changed["cases"][0]["targets"][1]["expected_reference"]["pointer"]
            elif alter == "exit_pointer": changed["cases"][0]["targets"][0]["exit_reference"]["pointer"] = changed["cases"][0]["targets"][1]["exit_reference"]["pointer"]
            else: next(row for row in changed["cases"] if row["entrypoints"] == ["main.py"])["entrypoints"] = ["."]
            with self.subTest(alter=alter), patch.object(contracts, "frozen_lock", return_value=changed), self.assertRaises(ValueError):
                contracts.load_inputs(REPOSITORY)

    def test_raw_lock_hash_and_source_owner_drift_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); path = root / contracts.LOCK_PATH; path.parent.mkdir(parents=True)
            path.write_bytes((REPOSITORY / contracts.LOCK_PATH).read_bytes() + b"\n")
            with self.assertRaises(ValueError): contracts.frozen_lock(root)
        changed = copy.deepcopy(self.lock); first = next(iter(changed["source_files"]))
        changed["source_files"][first]["sha256"] = "0" * 64
        with patch.object(contracts, "frozen_lock", return_value=changed), self.assertRaises(ValueError):
            contracts.load_inputs(REPOSITORY)

    def test_path_config_reference_shadow_collision_and_selection_guards(self):
        original = {"name": "control", "files": {"main.py": "x = 1\n"}, "source_sha256": {},
                    "expected": [[], [], []], "reference_exits": [0, 0, 0]}
        for files in ({"../escape.py": ""}, {"Mypy.py": ""}, {"mypy.ini/other.py": ""},
                      {"main.py": "", "MAIN.py": ""}, {"pkg.py": "", "PKG.py/other.py": ""},
                      {"pyproject.toml": ""}, {"main.py": "", "main.py/child.py": ""}):
            row = copy.deepcopy(original); row["files"] = files
            row["source_sha256"] = {name: contracts.digest(text.encode()) for name, text in files.items()}
            with self.subTest(files=files), self.assertRaises(ValueError): contracts.validate_case(row, "mypy_projects", {})
        row = copy.deepcopy(original); row["source_sha256"] = {"main.py": "0" * 64}
        with self.assertRaises(ValueError): contracts.validate_case(row, "mypy_projects", {})
        selected = next(case for case in self.cases if case["entrypoints"] == ["main.py"])
        owner = contracts.source_json(REPOSITORY, selected["source_path"])[selected["source_index"]]
        with self.assertRaises(ValueError): contracts.validate_case(owner, "pydantic_config", {})
        entries = {owner["name"]: {"entrypoints": ["main.py"], "source_sha256": {}}}
        with self.assertRaises(ValueError): contracts.validate_case(owner, "pydantic_config", entries)
        row = copy.deepcopy(original); row["source_sha256"] = {"main.py": contracts.digest(b"x = 1\n")}; row["expected"].pop()
        with self.assertRaises(ValueError): contracts.validate_case(row, "mypy_projects", {})

    def test_exact_directory_or_main_selection_keeps_all_fixed_strict_arguments(self):
        for entries in (["."], ["main.py"]):
            argv = contracts.argv_for(Path("/venv/python"), Path("/work"), entries, 12, self.lock["strict_flags"])
            self.assertEqual(argv[-1:], entries)
            self.assertEqual(argv[1:6], ["-I", "-B", "-m", "mypy", "--strict"])
            self.assertEqual(argv[6:19], self.lock["strict_flags"])
            self.assertIn("--cache-dir=/dev/null", argv)
            self.assertIn("/work/mypy.ini", argv)
        for entries in (["." , "main.py"], [], ["other.py"]):
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                contracts.argv_for(Path("/venv/python"), Path("/work"), entries, 12, self.lock["strict_flags"])

    def observe(self, temporary, case=None, stdout=None, **options):
        case = case or next(case for case in self.cases if case["expected"][0] and all(row["severity"] == "note" for row in case["expected"][0]))
        root = Path(temporary).resolve() / "case"
        stdout = raw_diagnostics(case["expected"][0]) if stdout is None else stdout
        with patch.object(contracts, "run_command", side_effect=fake_command(stdout, **options)):
            row = contracts.run_case(case, "3.12.11", Path("/never-executed"), {"identity_sha256": "static"}, self.lock["strict_flags"], root)
        return row, root

    def test_notes_only_exit_one_keeps_every_note_hint_and_actual_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            row, root = self.observe(temporary)
            self.assertEqual(row["errors"], [])
            self.assertEqual(row["status"], "passed")
            self.assertTrue(all(item["severity"] == "note" for item in row["diagnostics"]))
            self.assertEqual(row["receipt"]["actual_exit"], 1)
            self.assertEqual(json.loads((root / "command/returned-receipt.json").read_bytes()), row["returned_receipt"])
        with tempfile.TemporaryDirectory() as temporary:
            row, _ = self.observe(temporary, actual_exit=0)
            self.assertEqual(row["status"], "failed")
            self.assertTrue(row["diagnostics_match"]); self.assertFalse(row["actual_exit_matches"])

    def test_empty_blank_stdout_and_zero_columns_preserve_the_existing_projection(self):
        empty = next(case for case in self.cases if not case["expected"][0])
        with tempfile.TemporaryDirectory() as temporary:
            row, root = self.observe(temporary, empty, stdout=b"\n", actual_exit=0)
            self.assertEqual(row["status"], "passed"); self.assertEqual(row["diagnostics"], [])
            self.assertEqual((root / "command/stdout.raw").read_bytes(), b"\n")
        zero = next(case for case in self.cases if any(row["location"]["column"] == 0 for row in case["expected"][0]))
        with tempfile.TemporaryDirectory() as temporary:
            row, _ = self.observe(temporary, zero)
            self.assertEqual(row["errors"], []); self.assertEqual(row["status"], "passed")
            self.assertTrue(any(item["location"]["column"] == 0 for item in row["diagnostics"]))

    def test_global_order_positions_and_hint_text_are_not_dropped_or_sorted(self):
        case = next(case for case in self.cases if len(case["expected"][0]) > 1 and len({row["path"] for row in case["expected"][0]}) > 1)
        for mutate in ("order", "position", "hint"):
            expected = copy.deepcopy(case["expected"][0])
            if mutate == "order": expected.reverse()
            elif mutate == "position": expected[0]["location"]["end_column"] += 1
            else: expected[0]["hint"] = "changed\nmultiline hint"
            with self.subTest(mutate=mutate), tempfile.TemporaryDirectory() as temporary:
                row, _ = self.observe(temporary, case, stdout=raw_diagnostics(expected))
                self.assertEqual(row["status"], "failed"); self.assertFalse(row["diagnostics_match"])

    def test_missing_corrupt_and_unwaited_receipts_keep_known_child_accounting(self):
        for saved in ("missing", "corrupt"):
            with self.subTest(saved=saved), tempfile.TemporaryDirectory() as temporary:
                row, root = self.observe(temporary, saved=saved)
                self.assertEqual(row["status"], "failed")
                self.assertEqual(contracts.process_counts([row]), {"actual_project_processes": 1, "waited_project_processes": 1, "reconciled_project_receipts": 0})
                self.assertTrue((root / "command/returned-receipt.json").is_file())
        def unwaited(record, cwd): record["wait_completed"] = False
        with tempfile.TemporaryDirectory() as temporary:
            row, _ = self.observe(temporary, mutation=unwaited)
            self.assertEqual(row["status"], "failed")
            self.assertEqual(contracts.process_counts([row])["waited_project_processes"], 0)
        def boolean_waited_pid(record, cwd): record.update(pid=1, waited_pid=True)
        with tempfile.TemporaryDirectory() as temporary:
            row, _ = self.observe(temporary, mutation=boolean_waited_pid)
            self.assertEqual(row["status"], "failed")
            self.assertEqual(contracts.process_counts([row])["actual_project_processes"], 1)
            self.assertEqual(contracts.process_counts([row])["waited_project_processes"], 0)

    def test_changed_source_and_unexpected_stderr_preserve_first_failures(self):
        def changed(record, cwd): (cwd / "main.py").write_bytes(b"# changed source\n")
        for options in ({"mutation": changed}, {"stderr": b"unexpected stderr\n"}):
            with self.subTest(options=options), tempfile.TemporaryDirectory() as temporary:
                row, root = self.observe(temporary, **options)
                self.assertEqual(row["status"], "failed")
                self.assertTrue((root / "input-snapshot/main.py").is_file())
                self.assertTrue((root / "observation.json").is_file())

    def test_controller_failure_does_not_stop_the_next_independent_project(self):
        planned = self.cases[:2]
        def observation(case, version, executable, identity, flags, folder):
            if case is planned[0]: raise ValueError("injected controller failure")
            row = contracts.base_row(case, version); row["status"] = "passed"; return row
        with tempfile.TemporaryDirectory() as temporary, patch.object(contracts, "run_case", side_effect=observation):
            rows = contracts.execute_plan(planned, "3.12.11", Path("/never-executed"), {}, self.lock["strict_flags"], Path(temporary))
        self.assertEqual([row["status"] for row in rows], ["failed", "passed"])

    def test_final_files_and_both_indexes_agree_without_overwriting_initial_observation(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve(); case = self.cases[0]
            folder = output / "cases" / case["family"] / case["id"]
            row = contracts.base_row(case, "3.12.11"); row["status"] = "passed"
            folder.mkdir(parents=True); contracts.write_json(folder / "observation.json", row)
            initial = (folder / "observation.json").read_bytes()
            contracts.publish([row], [case], output, False)
            self.assertEqual(row["status"], "failed")
            self.assertEqual((folder / "observation.json").read_bytes(), initial)
            self.assertEqual(json.loads((output / row["result_pointer"]).read_bytes()), row)
            self.assertEqual(json.loads((output / "case-results.json").read_bytes()), [row])
            self.assertEqual([json.loads(line) for line in (output / "raw-index.jsonl").read_bytes().splitlines()], [row])

    def test_host_rejection_stops_downloads_and_emits_the_full_575_not_run_fallback(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve() / "output"
            with patch.object(contracts, "host_guard", side_effect=ValueError("not hosted")), \
                    patch.object(contracts, "download") as download, patch.object(contracts.capture, "prepare_environment") as prepare:
                code = contracts.run(REPOSITORY, output, Path("/never-executed"), "3.12.11")
            self.assertEqual(code, 1); download.assert_not_called(); prepare.assert_not_called()
            rows = json.loads((output / "case-results.json").read_bytes())
            self.assertEqual(len(rows), 575); self.assertTrue(all(row["status"] == "not_run" for row in rows))
            self.assertTrue(all(json.loads((output / row["result_pointer"]).read_bytes()) == row for row in rows))
            self.assertEqual(len((output / "raw-index.jsonl").read_bytes().splitlines()), 575)
            summary = json.loads((output / "summary.json").read_bytes())
            self.assertEqual(summary["actual_project_processes"], 0); self.assertFalse(summary["comparison_passed"])

    def test_preparation_failure_keeps_all_planned_projects_explicitly_not_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); python = root / "static-python"; python.write_bytes(b"not an executed interpreter")
            original_check = contracts.checked_bytes
            def allow_static_base(raw, expected):
                return raw if raw == b"not an executed interpreter" else original_check(raw, expected)
            with patch.object(contracts, "host_guard"), patch.dict(os.environ, {"MYPYPATH": ""}), \
                    patch.object(contracts, "checked_bytes", side_effect=allow_static_base), \
                    patch.object(contracts, "download"), \
                    patch.object(contracts.capture, "prepare_environment", side_effect=ValueError("injected prep failure")) as prepare, \
                    patch.object(contracts, "run_command") as command:
                code = contracts.run(REPOSITORY, root / "output", python, "3.12.11")
            self.assertEqual(code, 1); prepare.assert_called_once(); command.assert_not_called()
            rows = json.loads((root / "output/case-results.json").read_bytes())
            self.assertEqual(len(rows), 575); self.assertTrue(all(row["status"] == "not_run" for row in rows))
            self.assertTrue(all(row["errors"][0]["stage"] == "environment_preparation" for row in rows))

    def test_ambient_mypypath_stops_before_download_or_preparation(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(contracts, "host_guard"), \
                patch.dict(os.environ, {"MYPYPATH": "/unapproved"}), patch.object(contracts, "download") as download, \
                patch.object(contracts.capture, "prepare_environment") as prepare:
            code = contracts.run(REPOSITORY, Path(temporary).resolve() / "output", Path("/never-executed"), "3.12.11")
            self.assertEqual(code, 1); download.assert_not_called(); prepare.assert_not_called()


if __name__ == "__main__":
    unittest.main()
