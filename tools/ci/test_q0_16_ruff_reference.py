"""Static input and mocked capture controls. No reference package executes."""
from __future__ import annotations

import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import q0_16_ruff_reference as runner
import q0_ruff_reference as old_capture

REPOSITORY = Path(__file__).resolve().parents[2]
CORPUS = (REPOSITORY / runner.CORPUS_PATH).read_bytes()
LOCK = (REPOSITORY / "tools/ci/q0_ruff_reference_lock.json").read_bytes()
PROFILE = (REPOSITORY / "crates/mamba-quality/assets/profile.json").read_bytes()


def inputs():
    return runner.validate_inputs(CORPUS, LOCK, PROFILE)


def raw_point(raw: bytes, offset: int) -> dict:
    prefix = raw[:offset].decode()
    return {"row": prefix.count("\n") + 1, "column": len(prefix.rsplit("\n", 1)[-1]) + 1}


def raw_messages(frozen: list[dict], project: Path) -> list[dict]:
    result = []
    for diagnostic in frozen:
        row = diagnostic["location"]
        fix = copy.deepcopy(diagnostic["fix"])
        if fix:
            source = (project / diagnostic["path"]).read_bytes()
            fix["edits"] = [{"location": raw_point(source, item["start"]),
                "end_location": raw_point(source, item["end"]), "content": item["replacement"]} for item in fix["edits"]]
        result.append({"code": diagnostic["rule"], "message": diagnostic["message"],
            "filename": str(project / diagnostic["path"]), "location": {"row": row["line"], "column": row["column"]},
            "end_location": {"row": row["end_line"], "column": row["end_column"]}, "fix": fix})
    return result


def fake_ruff(target: dict, mutation=None):
    def capture(binary, project, names, minor, output, *, fixing=False):
        output.mkdir()
        supplied = {name: {"bytes": len((project / name).read_bytes()),
                          "sha256": runner.digest((project / name).read_bytes())} for name in names}
        runner.write_json(output / "inputs.json", supplied)
        (output / "stdin.raw").write_bytes(b"")
        if fixing:
            for name, text in target["fixed_hex"].items():
                (project / name).write_bytes(bytes.fromhex(text))
        messages = raw_messages(target["before" if output.name == "before" else "after"], project)
        record = {"argv": [str(binary), "check", *names], "cwd": str(project), "expected_exits": (0, 1),
            "started_utc": "2026-10-29T00:00:00+00:00", "pid": 123, "waited_pid": 123,
            "wait_completed": True, "actual_exit": 1, "timed_out": False, "launch_error": None,
            "duration_seconds": 0.1, "stdin_sha256": runner.digest(b""), "passed": True}
        stderr = b""
        if mutation:
            stderr = mutation(output.name, messages, record, project) or b""
        stdout = json.dumps(messages).encode()
        (output / "stdout.raw").write_bytes(stdout); (output / "stderr.raw").write_bytes(stderr)
        record.update(stdout_sha256=runner.digest(stdout), stderr_sha256=runner.digest(stderr),
                      inputs_sha256=runner.digest((output / "inputs.json").read_bytes()))
        runner.write_json(output / "receipt.json", record)
        return messages, record
    return capture


class RuffInteractionControls(unittest.TestCase):
    def run_mocked_matrix(self, temporary, mutation=None, saved_receipt_mode=None):
        data, _, _ = inputs()
        root = Path(temporary).resolve(); output = root / "output"
        pythons = {}
        for minor in (12, 13, 14):
            pythons[minor] = root / f"python3.{minor}"
            pythons[minor].write_bytes(b"static interpreter identity; never executed")
        binary_bytes = b"static Ruff binary identity; never executed"
        wheel = io.BytesIO()
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("ruff-0.16.10.data/scripts/ruff", binary_bytes)
        def fake_download(pin, destination):
            destination.mkdir(parents=True)
            raw = wheel.getvalue() if pin["filename"].endswith(".whl") else b"static archive placeholder"
            (destination / pin["filename"]).write_bytes(raw)
            return raw
        def fake_prepare(python, version, destination, wheel):
            destination.mkdir(parents=True)
            (destination / "venv/bin").mkdir(parents=True)
            (destination / "venv/bin/python").write_bytes(python.read_bytes())
            binary = destination / "venv/bin/ruff"; binary.write_bytes(binary_bytes)
            record = destination / "venv/lib/ruff-0.16.10.dist-info/RECORD"
            record.parent.mkdir(parents=True); record.write_bytes(b"static RECORD retained before and after\n")
            runner.write_json(destination / "identity.json", {"python": version, "mocked": True})
            return binary
        calls = []
        def fake_capture(binary, project, names, minor, destination, *, fixing=False):
            calls.append((minor, destination.name))
            def mutate(stage, messages, receipt, work):
                return mutation(minor, stage, messages, receipt, work) if mutation else None
            observed = fake_ruff(data["versions"][minor - 12], mutate)(
                binary, project, names, minor, destination, fixing=fixing)
            if minor == 12 and destination.name == "before" and saved_receipt_mode:
                saved = destination / "receipt.json"
                if saved_receipt_mode == "missing":
                    saved.unlink()
                elif saved_receipt_mode == "malformed":
                    saved.write_bytes(b"{")
                else:
                    changed = json.loads(saved.read_bytes())
                    changed["pid"] = 778899
                    saved.write_text(json.dumps(changed))
            return observed
        with patch.object(runner, "host_guard"), patch.object(runner, "download", side_effect=fake_download), \
                patch.object(runner, "prepare", side_effect=fake_prepare), patch.object(runner, "ruff", side_effect=fake_capture):
            exit_code = runner.run(REPOSITORY, output, pythons)
        return exit_code, output, calls

    def test_full_mocked_matrix_retains_12_processes_42_rows_84_roles_and_records(self):
        with tempfile.TemporaryDirectory() as temporary:
            code, output, calls = self.run_mocked_matrix(temporary)
            self.assertEqual(code, 0)
            self.assertEqual(calls, [(minor, stage) for minor in (12, 13, 14) for stage in runner.STAGES])
            summary = json.loads((output / "summary.json").read_bytes())
            self.assertEqual(summary["terminal_project_commands"], 12)
            self.assertTrue(summary["comparison_passed"])
            self.assertFalse(summary["issue_accepted"])
            rows = json.loads((output / "rule-target-index.json").read_bytes())
            self.assertEqual(len(rows), 42)
            self.assertTrue(all(row["status"] == "passed" and row["project_before_exit"] == 1 for row in rows))
            self.assertEqual(sum(len(row["roles"]) for row in rows), 84)
            manifest = json.loads((output / "artifact-manifest.json").read_bytes())
            for minor in (12, 13, 14):
                self.assertTrue(summary["targets"][runner.VERSIONS[minor]]["environment_unchanged"])
                self.assertTrue((output / f"python3{minor}/package-records-before/lib/ruff-0.16.10.dist-info/RECORD").is_file())
                self.assertFalse(any(name.startswith(f"python3{minor}/venv/") for name in manifest))
                for stage in runner.STAGES:
                    self.assertIn(f"python3{minor}/{stage}/stderr.raw", manifest)

    def test_first_failed_project_receipt_is_kept_and_all_later_stages_and_targets_run(self):
        def unwaited(minor, stage, messages, receipt, project):
            if minor == 12 and stage == "before": receipt["wait_completed"] = False
        with tempfile.TemporaryDirectory() as temporary:
            code, output, calls = self.run_mocked_matrix(temporary, unwaited)
            self.assertEqual(code, 1)
            self.assertEqual(len(calls), 12)
            initial = json.loads((output / "python312/before/receipt.json").read_bytes())
            self.assertFalse(initial["wait_completed"])
            rows = json.loads((output / "rule-target-index.json").read_bytes())
            self.assertTrue(all(row["status"] == "failed" for row in rows if row["python_target"] == "3.12"))
            self.assertTrue(all(row["status"] == "passed" for row in rows if row["python_target"] in {"3.13", "3.14"}))

    def test_bad_saved_receipts_do_not_erase_known_completed_project_commands(self):
        for mode in ("missing", "malformed", "mismatch"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                code, output, calls = self.run_mocked_matrix(temporary, saved_receipt_mode=mode)
                self.assertEqual(code, 1)
                self.assertEqual(len(calls), 12)
                summary = json.loads((output / "summary.json").read_bytes())
                self.assertFalse(summary["comparison_passed"])
                self.assertEqual(summary["terminal_project_commands"], 12)
                observed = summary["targets"][runner.VERSIONS[12]]["stages"]["before"]
                self.assertEqual(observed["status"], "failed")
                self.assertTrue(observed["process_launched"])
                self.assertIsNone(observed["diagnostics"])
                returned_path = output / "python312" / observed["returned_receipt_pointer"]
                returned = json.loads(returned_path.read_bytes())
                self.assertEqual(returned["pid"], 123)
                self.assertTrue(returned["wait_completed"])
                self.assertEqual(observed["returned_receipt_sha256"], runner.digest(returned_path.read_bytes()))
                if mode == "mismatch":
                    self.assertEqual(observed["receipt"]["pid"], 778899)
                else:
                    self.assertIsNone(observed["receipt"])
                rows = json.loads((output / "rule-target-index.json").read_bytes())
                self.assertTrue(all(row["status"] == "failed" for row in rows if row["python_target"] == "3.12"))
                self.assertTrue(all(row["status"] == "passed" for row in rows if row["python_target"] in {"3.13", "3.14"}))

    def test_static_imports_do_not_load_reference_packages(self):
        self.assertFalse({name.split(".")[0] for name in sys.modules} & {"ruff", "mypy", "pylint", "astroid", "pydantic"})

    def test_all_frozen_targets_sources_and_same_file_roles_are_nonzero(self):
        data, sources, _ = inputs()
        self.assertEqual(len(sources), 29)
        self.assertEqual([row["python"] for row in data["versions"]], ["3.12", "3.13", "3.14"])
        rows = [row for minor, target in zip((12, 13, 14), data["versions"])
                for row in runner.rule_rows(target["before"], data["rules"], minor)]
        self.assertEqual(len(rows), 42)
        self.assertEqual(sum(len(row["roles"]) for row in rows), 84)
        self.assertTrue(all(row["role_observations_passed"] for row in rows))
        self.assertTrue(all(role["per_file_process_exit"] is None for row in rows for role in row["roles"].values()))

    def test_wrong_profile_lock_or_corpus_hash_cannot_enter_the_runner(self):
        for corpus, lock, profile in ((CORPUS + b"\n", LOCK, PROFILE), (CORPUS, LOCK + b"\n", PROFILE),
                                     (CORPUS, LOCK, PROFILE + b"\n")):
            with self.subTest(corpus=len(corpus), lock=len(lock), profile=len(profile)), self.assertRaises(ValueError):
                runner.validate_inputs(corpus, lock, profile)

    def test_source_hash_paths_and_missing_target_are_rejected_independently(self):
        data, _, lock = inputs()
        for name in ("../escape.py", "--config.py", "nested/file.py"):
            changed = copy.deepcopy(data); changed["sources"][name] = changed["sources"].pop(next(iter(changed["sources"])))
            with self.subTest(name=name), self.assertRaises(ValueError):
                runner.decode_sources(changed)
        changed = copy.deepcopy(data); changed["sources"][next(iter(changed["sources"]))]["source_sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            runner.decode_sources(changed)
        changed = copy.deepcopy(data); changed["versions"].pop()
        with self.assertRaises(ValueError):
            runner.validate_contract(changed, lock, json.loads(PROFILE))
        changed = copy.deepcopy(data); changed["versions"][0]["fixed_hex"].pop(next(iter(changed["sources"])))
        with self.assertRaises(ValueError):
            runner.validate_contract(changed, lock, json.loads(PROFILE))

    def test_projection_preserves_every_ordered_message_position_and_fix(self):
        data, sources, _ = inputs()
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary).resolve() / "project"
            runner.write_sources(project, sources)
            raw = raw_messages(data["versions"][0]["before"], project)
            self.assertEqual(runner.project_messages(raw, project, sources), data["versions"][0]["before"])
            raw[0]["filename"] = "/outside/main.py"
            with self.assertRaises(ValueError):
                runner.project_messages(raw, project, sources)

    def test_projection_uses_utf8_byte_edit_offsets_without_message_changes(self):
        project = Path("/project")
        message = {"filename": "/project/main.py", "code": "F401", "message": " exact 水 message ",
            "location": {"row": 1, "column": 2}, "end_location": {"row": 1, "column": 3},
            "fix": {"applicability": "unsafe", "message": " exact fix ", "edits": [
                {"location": {"row": 1, "column": 2}, "end_location": {"row": 1, "column": 3}, "content": "置"}]}}
        projected = runner.project_messages([message], project, {"main.py": "水x\n".encode()})[0]
        self.assertEqual(projected["fix"]["edits"], [{"start": 3, "end": 4, "replacement": "置"}])
        self.assertEqual(projected["message"], " exact 水 message ")

    def test_old_command_keeps_the_entire_default_select_and_safe_fix_flags(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary).resolve(); (project / "main.py").write_bytes(b"x = 1\n")
            with patch.object(old_capture, "checked_command", return_value={}) as capture, \
                    patch.object(old_capture, "read_messages", return_value=[]):
                old_capture.ruff(Path("/never-executed"), project, ["main.py"], 12, project / "out", fixing=True)
            argv = capture.call_args.args[0]
            self.assertEqual(argv[argv.index("--select") + 1], "E4,E7,E9,F,UP,B,SIM,RUF100")
            self.assertEqual(argv[argv.index("--target-version") + 1], "py312")
            self.assertTrue({"--isolated", "--no-cache", "--no-preview", "--fix", "--no-unsafe-fixes"} <= set(argv))

    def run_one_stage(self, temporary, name="before", mutation=None):
        data, sources, _ = inputs(); target = data["versions"][0]
        root = Path(temporary).resolve(); project = root / "project"
        actual = sources if name in {"before", "fix-1"} else {path: bytes.fromhex(raw) for path, raw in target["fixed_hex"].items()}
        runner.write_sources(project, {**actual, "pyproject.toml": runner.CONFIG})
        with patch.object(runner, "ruff", side_effect=fake_ruff(target, mutation)):
            row = runner.stage(Path("/not-executed"), project, sorted(sources), 12, target, name, root)
        return row, root

    def test_all_four_exact_capture_stages_pass_pure_controls(self):
        for stage in runner.STAGES:
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as temporary:
                row, root = self.run_one_stage(temporary, stage)
                self.assertEqual(row["errors"], [])
                self.assertEqual(row["status"], "passed")
                self.assertEqual(row["receipt"]["actual_exit"], 1)
                self.assertEqual(json.loads((root / row["receipt_pointer"]).read_bytes()), row["receipt"])

    def test_order_position_fix_wait_exit_and_stderr_mutations_cannot_pass(self):
        def order(stage, messages, receipt, project): messages.reverse()
        def position(stage, messages, receipt, project): messages[0]["location"]["column"] += 1
        def fix(stage, messages, receipt, project):
            next(row for row in messages if row["fix"])["fix"]["edits"][0]["content"] = "wrong replacement"
        def unwaited(stage, messages, receipt, project): receipt["wait_completed"] = False
        def exit_code(stage, messages, receipt, project): receipt["actual_exit"] = 0
        def stderr(stage, messages, receipt, project): return b"unexpected warning\n"
        for mutate in (order, position, fix, unwaited, exit_code, stderr):
            with self.subTest(mutation=mutate.__name__), tempfile.TemporaryDirectory() as temporary:
                row, root = self.run_one_stage(temporary, mutation=mutate)
                self.assertEqual(row["status"], "failed")
                self.assertTrue(row["errors"])
                self.assertTrue((root / "before/receipt.json").is_file())
                if mutate is stderr:
                    self.assertEqual((root / "before/stderr.raw").read_bytes(), b"unexpected warning\n")
                    self.assertTrue((root / "states/before-output/b022-negative.py").is_file())

    def test_full_fixed_byte_map_and_repeat_stability_are_not_message_only_checks(self):
        for stage in ("fix-1", "fix-2", "after"):
            def change(name, messages, receipt, project):
                path = project / "b022-negative.py"; path.write_bytes(path.read_bytes() + b"# changed\n")
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as temporary:
                row, _ = self.run_one_stage(temporary, stage, change)
                self.assertEqual(row["status"], "failed")
                self.assertIn("fixed bytes", row["errors"][0]["message"])

    def test_project_check_never_changes_source_or_config(self):
        def change(name, messages, receipt, project): (project / "pyproject.toml").write_bytes(b"[project]\n")
        with tempfile.TemporaryDirectory() as temporary:
            row, _ = self.run_one_stage(temporary, mutation=change)
            self.assertEqual(row["status"], "failed")

    def test_missing_positive_partner_and_unrelated_negative_error_fail_roles(self):
        data, _, _ = inputs(); messages = data["versions"][0]["before"]
        messages = [row for row in messages if not (row["path"] == "b022-positive.py" and row["rule"] == "B016")]
        rows = runner.rule_rows(messages, data["rules"], 12)
        self.assertFalse(next(row for row in rows if row["rule"] == "B022")["role_observations_passed"])
        messages.append({"path": "b022-negative.py", "rule": "B016"})
        self.assertFalse(next(row for row in runner.rule_rows(messages, data["rules"], 12) if row["rule"] == "B022")["roles"]["negative"]["passed"])

    def test_nonhosted_run_stops_before_downloads_and_keeps_all_42_not_run_rows(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve() / "output"
            with patch.object(runner, "host_guard", side_effect=ValueError("not hosted")), \
                    patch.object(runner, "download") as download, patch.object(runner, "prepare") as prepare:
                result = runner.run(Path("/not-read"), output, {})
            self.assertEqual(result, 1); download.assert_not_called(); prepare.assert_not_called()
            rows = json.loads((output / "rule-target-index.json").read_bytes())
            self.assertEqual(len(rows), 42)
            self.assertTrue(all(row["status"] == "not_run" and row["before_result_pointer"] is None for row in rows))
            summary = json.loads((output / "summary.json").read_bytes())
            self.assertFalse(summary["issue_accepted"])
            self.assertEqual(summary["terminal_project_commands"], 0)


if __name__ == "__main__":
    unittest.main()
