"""Pure controls for the finite hosted contract lint capture.

No original checker, fixture program, package installer or native tool runs.
"""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import q0_16_contract_lint as lint


class ContractControls(unittest.TestCase):
    def test_known_wait_requires_physical_process_fields(self):
        fact = {"pid": 1, "waited_pid": 1, "wait_completed": True, "actual_exit": 2}
        self.assertTrue(lint.known_wait(fact))
        self.assertFalse(lint.known_wait(None))
        for change in ({"pid": True}, {"pid": 0}, {"waited_pid": True},
                       {"waited_pid": 2}, {"wait_completed": False},
                       {"actual_exit": None}, {"actual_exit": False}):
            self.assertFalse(lint.known_wait({**fact, **change}))

    def test_pointer_rejects_negative_indexes_and_bad_escape(self):
        for pointer in ("/-1", "/01", "/+1", "/~2"):
            with self.assertRaises(ValueError):
                lint.select([1, 2], pointer)

    def test_safe_selection_keeps_lexical_spelling(self):
        root = Path("/sealed/work")
        self.assertEqual(str(lint.lexical_path(root, "regular.py/child.py")), "/sealed/work/regular.py/child.py")
        for path in ("../escape.py", "/outside.py", "x/../../escape.py"):
            with self.assertRaises(ValueError):
                lint.lexical_path(root, path)

    def test_receipt_bool_is_not_integer(self):
        receipt = {"pid": 1, "waited_pid": True, "wait_completed": True, "timed_out": False,
            "launch_error": None, "actual_exit": 0, "passed": True}
        with self.assertRaises(ValueError):
            lint.terminal_receipt(receipt, (0,))
        receipt["waited_pid"] = 1
        lint.terminal_receipt(receipt, (0,))

    def test_saved_receipt_all_fields_and_JSON_types(self):
        returned = {"expected_exits": (0, 1), "pid": 3, "waited_pid": 3, "actual_exit": 1, "extra": {"value": 0}}
        stored = {**returned, "expected_exits": [0, 1]}
        lint.capture.require_saved_receipt(returned, stored, (0, 1))
        for change in ({"extra": {"value": False}}, {"actual_exit": True}, {"new": "field"}):
            bad = {**stored, **change}
            with self.assertRaises(ValueError):
                lint.capture.require_saved_receipt(returned, bad, (0, 1))

    def test_raw_mypy_projection_keeps_hint_and_end_sites(self):
        raw = {"file": "main.py", "line": 2, "column": 0, "end_line": 3, "end_column": -1,
            "message": "message", "hint": "note\nsecond", "code": "misc", "severity": "note"}
        projected = lint.normal_mypy(raw, Path("/work"), None)
        self.assertEqual(projected["hint"], "note\nsecond")
        self.assertEqual(projected["location"], {"line": 2, "column": 1, "end_line": 3, "end_column": -1})
        with self.assertRaises(ValueError):
            lint.json_lines(json.dumps({**raw, "unknown": 1}).encode())

    def test_whole_source_JSON_exit_selects_target_and_all_repeats(self):
        calls = [{"target": target, "reporter": reporter, "repeat": repeat,
            "actual_exit": 1 if target == "3.12" and reporter == "json" else 7}
            for target in ("3.12", "3.13") for reporter in ("json", "text") for repeat in (1, 2)]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); path = root / "lambda_binding.json"
            def setup():
                value = {"source": "static data", "original_calls": calls}
                path.write_text(json.dumps(value))
                ref = {"source_id": path.name, "sha256": lint.digest(path.read_bytes()), "json_pointer": "/source", "value_sha256": lint.value_sha(value["source"])}
                return lint.Sources(root), {"context": {"tool": "mypy", "target": "3.12"}}, {"unit_id": "mypy:ancillary:lambda_binding.json:whole-source", "case_source": ref}
            sources, case, alias = setup()
            code, binding = lint.explicit_exit(sources, case, alias)
            self.assertEqual(code, 1)
            self.assertEqual([ref["json_pointer"] for ref in binding["repeat_bindings"]],
                ["/original_calls/0/actual_exit", "/original_calls/1/actual_exit"])
            self.assertEqual([sources.resolve(ref) for ref in binding["repeat_bindings"]], [1, 1])
            calls[1]["actual_exit"] = 0
            sources, case, alias = setup()
            with self.assertRaisesRegex(ValueError, "conflict"):
                lint.explicit_exit(sources, case, alias)

    def test_metadata_path_binding_is_exact_prefix(self):
        site = Path("/case/metadata/venv/lib/python3.12/site-packages")
        self.assertEqual(lint.project_path(str(site / "pkg/mod.py"), Path("/case/work"), site),
            ".venv/lib/python3.12/site-packages/pkg/mod.py")
        self.assertEqual(lint.project_path("/other/pkg/mod.py", Path("/case/work"), site), "/other/pkg/mod.py")

    def test_duplicate_message_binding_requires_declared_entry(self):
        message = 'Duplicate module named "m" (also at "/old/a.py")'
        self.assertEqual(lint.bind_duplicate_message(message, "/old", Path("/new"), {"a.py": {}}),
            'Duplicate module named "m" (also at "/new/a.py")')
        for entries in ({}, {"b.py": {}}):
            with self.assertRaises(ValueError):
                lint.bind_duplicate_message(message, "/old", Path("/new"), entries)
        self.assertEqual(lint.bind_duplicate_message("other message", "/old", Path("/new"), {}), "other message")

    def test_crash_binding_has_narrow_message_and_rule_guard(self):
        message = ("main.py: Fatal error while checking 'main.py'. Please open an issue in our bug tracker so we address this. "
            "There is a pre-filled template that you can use in '/cache/pylint-crash-2026-10-29-01-02-03.txt'.")
        row = {"rule": "F0002", "path": "main.py", "message": message}
        self.assertIn("<timestamp>", lint.crash_filename([row])[0]["message"])
        for altered in ({**row, "rule": "F0010"}, {**row, "path": "other.py"}, {**row, "message": message.replace("Please open", "Please create")}):
            self.assertEqual(lint.crash_filename([altered]), [altered])

    def test_declared_argv_uses_real_metadata_python_and_sealed_config(self):
        case = {"context": {"tool": "mypy", "target": "3.12", "selection": {"paths": ["."], "absolute": False}}}
        argv = lint.argv_for(Path("/checker/python"), case, Path("/case/work"), Path("/case/control/mypy.ini"), Path("/case/metadata/venv/bin/python"))
        self.assertEqual(argv[1:4], ["-I", "-m", "mypy"])
        self.assertEqual(argv[argv.index("--python-executable") + 1], "/case/metadata/venv/bin/python")
        self.assertEqual(argv[argv.index("--config-file") + 1], "/case/control/mypy.ini")
        self.assertEqual(argv[-1], ".")

    def test_metadata_startup_files_rejected_before_process(self):
        for name in ("unsafe.pth", "sitecustomize.py", "usercustomize.py", "site.py", "sitecustomize/__init__.py", "usercustomize/__init__.py"):
            case = {"context": {"target": "3.12", "layout": {"model_packages": {name: lint.payload(b"raise RuntimeError()")}}}}
            with tempfile.TemporaryDirectory() as folder, patch.object(lint, "run_command") as command:
                with self.assertRaises(ValueError):
                    lint.prepare_metadata(Path("/base"), Path("/checker"), "3.12.11", case, Path(folder) / "metadata")
                command.assert_not_called()

    def test_permission_restore_list_survives_guard_failure(self):
        layout = {"restricted_path": "x.py", "source_role_name_for_FS_operations": None}
        case = {"context": {"layout": layout, "selection": {"paths": ["x.py"]}}}
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); path = root / "x.py"; path.write_text("data")
            restores = []
            with patch.object(Path, "open", side_effect=OSError(5, "wrong errno")):
                with self.assertRaises(ValueError):
                    lint.apply_filesystem(case, root, restores)
            self.assertEqual(len(restores), 1)
            for item, mode in reversed(restores):
                item.chmod(mode)
            self.assertEqual(path.read_text(), "data")

    def test_filesystem_snapshot_detects_added_file_and_mode(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); p = root / "main.py"; p.write_bytes(b"text")
            before = lint.filesystem_snapshot(root)
            p.chmod(0o600)
            self.assertNotEqual(before, lint.filesystem_snapshot(root))
            (root / "new.py").write_bytes(b"new")
            self.assertIn("new.py", lint.filesystem_snapshot(root))

    def test_final_receipt_tamper_preserves_initial(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); command = root / "command"; command.mkdir()
            receipt = {"pid": 3, "waited_pid": 3, "wait_completed": True, "timed_out": False,
                "launch_error": None, "actual_exit": 0, "passed": True, "expected_exits": [0], "nested": {"zero": 0}}
            for name, key in (("stdout.raw", "stdout_sha256"), ("stderr.raw", "stderr_sha256"), ("stdin.raw", "stdin_sha256"), ("inputs.json", "inputs_sha256")):
                raw = b"{}" if name == "inputs.json" else b""
                (command / name).write_bytes(raw); receipt[key] = lint.digest(raw)
            for name in ("receipt.json", "returned-receipt.json"):
                (command / name).write_text(json.dumps(receipt))
            initial = b'{"status":"observed_unreconciled"}\n'; (root / "initial-result.json").write_bytes(initial)
            row = {"process_launched": True, "receipt": copy.deepcopy(receipt)}
            lint.final_reconcile(row, root)
            bad = copy.deepcopy(receipt); bad["nested"]["zero"] = False
            (command / "receipt.json").write_text(json.dumps(bad))
            with self.assertRaises(ValueError):
                lint.final_reconcile(row, root)
            self.assertEqual((root / "initial-result.json").read_bytes(), initial)

    def test_preparation_failure_keeps_complete_intended_request(self):
        case = {"request_id": "one", "context": {"tool": "mypy", "target": "3.12"}, "source_aliases": [], "expected_bindings": []}
        lock = {"requests": [case], "source_files": {}}
        with tempfile.TemporaryDirectory() as folder:
            repo = Path(folder) / "repo"; (repo / "tools/ci").mkdir(parents=True)
            (repo / lint.LOCK).write_text(json.dumps(lock)); output = Path(folder) / "artifact"
            def denied():
                self.assertEqual(json.loads((output / "intended-requests.json").read_text()), [case])
                raise ValueError("mocked hosted-only denial")
            with patch.object(lint, "COUNTS", {"mypy": 3, "pylint": 3}), patch.object(lint, "load_inputs", return_value=(lock, [case])), \
                    patch.object(lint.capture, "source_manifest", return_value={}), patch.object(lint, "host_guard", side_effect=denied), \
                    patch.object(lint.capture, "prepare_environment") as prepare, patch.object(lint, "run_command") as command:
                self.assertEqual(lint.run(repo, output, Path("/base"), "3.12.11", "mypy"), 1)
                prepare.assert_not_called(); command.assert_not_called()
            row = json.loads((output / "case-results.json").read_text())[0]
            self.assertEqual(row["status"], "not_run")
            self.assertFalse(json.loads((output / "summary.json").read_text())["capture_passed"])
            self.assertTrue((output / "requests/one/initial-result.json").is_file())
            self.assertTrue((output / "requests/one/final-result.json").is_file())


class SourcePlanControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo = Path(__file__).resolve().parents[2]
        cls.lock, cls.cases = lint.build_lock(cls.repo)

    def test_complete_finite_source_plan(self):
        self.assertEqual(self.lock["request_counts"], {"mypy": 2982, "pylint": 1380})
        self.assertEqual(len({c["request_id"] for c in self.cases}), 4362)
        for tool, count in (("mypy", 994), ("pylint", 460)):
            for target in lint.TARGETS:
                self.assertEqual(sum(c["context"]["tool"] == tool and c["context"]["target"] == target for c in self.cases), count)
        self.assertEqual(sum(bool(c["context"]["layout"]["model_packages"]) for c in self.cases), 138)

    def test_all_source_arrays_and_explicit_exit_fields_resolve(self):
        sources = lint.Sources(self.repo)
        for case in self.cases:
            for alias, expected in zip(case["source_aliases"], case["expected_bindings"]):
                rows, _ = lint.expected_record(sources, expected["source"])
                self.assertIsInstance(rows, list)
                code, ref = lint.explicit_exit(sources, case, alias)
                if code is not None:
                    self.assertEqual(sources.resolve(ref), code)
                if case["context"]["tool"] == "pylint":
                    self.assertIsNotNone(code)

    def test_public_lock_has_no_private_payloads(self):
        text = lint.encode(self.lock).decode()
        self.assertNotIn("/private/", text)
        self.assertNotIn("/Users/", text)
        self.assertNotIn("bytes_hex", text)
        for request in self.lock["requests"]:
            self.assertEqual(request["request_id"], lint.value_sha(next(c["context"] for c in self.cases if c["request_id"] == request["request_id"])))

    def test_frozen_lock_matches_current_public_sources(self):
        frozen = json.loads((self.repo / lint.LOCK).read_text())
        self.assertEqual(lint.encode(self.lock), lint.encode(frozen))

    def test_expected_path_binding_does_not_mutate_frozen_source_cache(self):
        sources = lint.Sources(self.repo)
        case = next(c for c in self.cases if c["source_aliases"][0]["case_source"]["source_id"].endswith("/duplicate_modules.json"))
        alias = case["source_aliases"][0]
        value = sources.resolve(alias["case_source"])
        original_hash = lint.value_sha(value)
        expected, _ = lint.expected_record(sources, case["expected_bindings"][0]["source"])
        raw = copy.deepcopy(expected)
        for row in raw:
            row["message"] = row["message"].replace(value["original_case_root"].rstrip("/") + "/", "/sealed/current/")
        stdout = b"\n".join(json.dumps(row).encode() for row in raw) + b"\n"
        code, _ = lint.explicit_exit(sources, case, alias)
        lint.compare(sources, case, stdout, code, Path("/sealed/current"))
        self.assertEqual(lint.value_sha(sources.resolve(alias["case_source"])), original_hash)


if __name__ == "__main__":
    unittest.main()
