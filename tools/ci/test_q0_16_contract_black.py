"""Offline controls. These tests never import or run the original Black package."""
from __future__ import annotations

from copy import deepcopy
import ast
from enum import Enum
import hashlib
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

import q0_16_contract_black as runner


class BlackContractControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo = Path(__file__).resolve().parents[2]
        cls.lock, cls.requests, _ = runner.load_plan(cls.repo)

    def successful_actual(self, request, output, unchanged=False):
        raw = bytes.fromhex(request["source_hex"])
        output_bytes = output.encode()
        row = dict.fromkeys(runner.API_RECORD_KEYS)
        row.update(request_id=request["request_id"], target=request["target"], operation=request["operation"],
                   input_sha256=runner.digest(raw), input_bytes=len(raw), mode=request["mode"],
                   decoded_input_sha256=runner.digest(raw), decoded_input_bytes=len(raw),
                   api_calls={"decode_bytes": 1, "format_file_contents": 1},
                   fast=False, lines=[], kind="unchanged" if unchanged else "formatted", output=output,
                   output_hex=output_bytes.hex(), output_sha256=runner.digest(output_bytes),
                   exception_type="black.report.NothingChanged" if unchanged else None,
                   message="" if unchanged else None, encoding="utf-8", newline="\n",
                   computed_reference_exit=int(output_bytes != raw),
                   internal_equivalence_and_stability_completed=not unchanged)
        return row

    def plain_success_request(self):
        return next(r for r in self.requests if r["operation"] == "black_safe_format_API"
                    and len(r["expected"]) == 1 and r["expected"][0]["value"].get("kind") == "formatted"
                    and r["expected"][0]["value"].get("reference_exit") == 1
                    and "output" in r["expected"][0]["value"])

    def test_plan_keeps_all_contexts_and_distinct_operations(self):
        self.assertEqual(len(self.requests), 747)
        rows = runner.plan_rows(self.requests)
        self.assertEqual(len({r["request_id"] for r in rows}), 747)
        self.assertTrue(all(r["status"] == "not_run" and r["actual_api_calls"] is None for r in rows))
        for target in runner.VERSIONS:
            self.assertEqual(sum(r["target"] == target for r in rows), 249)
            self.assertEqual(sum(r["target"] == target and r["operation"] == "black_decode_bytes_safe_format_API" for r in rows), 10)
        self.assertNotIn("source_hex", rows[0])

    def test_static_plan_does_not_launch_processes_or_import_black(self):
        with mock.patch("subprocess.Popen", side_effect=AssertionError("no process allowed")):
            runner.load_plan(self.repo)
        self.assertNotIn("black", __import__("sys").modules)

    def test_zero_or_partial_denominator_is_rejected(self):
        broken = deepcopy(self.lock)
        broken["requests"] = broken["requests"][:-1]
        original = runner.Sources.read
        def read(sources, name, expected=None):
            return runner.canonical(broken) if name == runner.LOCK else original(sources, name, expected)
        with mock.patch.object(runner.Sources, "read", read):
            with self.assertRaisesRegex(ValueError, "denominator"):
                runner.load_plan(self.repo)

    def test_bad_value_hash_and_noncanonical_pointer_are_rejected(self):
        source = runner.Sources(self.repo)
        ref = dict(self.requests[0]["input_binding"]["source"])
        ref["value_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "value SHA256"):
            source.value(ref)
        ref["json_pointer"] = "/cells/01/source_hex"
        with self.assertRaisesRegex(ValueError, "array pointer"):
            source.value(ref)

    def test_hosted_guard_rejects_local_or_ambient_python_paths(self):
        with self.assertRaisesRegex(ValueError, "GitHub-hosted"):
            runner.require_hosted({}, "darwin")
        env = {"GITHUB_ACTIONS":"true", "RUNNER_OS":"macOS", "RUNNER_ARCH":"ARM64", "PYTHONPATH":"/foreign"}
        with self.assertRaisesRegex(ValueError, "ambient"):
            runner.require_hosted(env, "darwin")

    def test_host_failure_still_keeps_all_747_initial_and_final_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "capture"
            with mock.patch.object(runner, "require_hosted", side_effect=ValueError("local execution denied")):
                self.assertEqual(runner.run(self.repo, output, Path("/unused/python"), "3.12.11"), 1)
            initial = json.loads((output / "initial-results.json").read_bytes())
            final = json.loads((output / "final-results.json").read_bytes())
            self.assertEqual(len(initial), 747)
            self.assertEqual(initial, final)
            self.assertTrue(all(r["status"] == "not_run" for r in final))
            self.assertFalse(json.loads((output / "summary.json").read_bytes())["contract_capture_passed"])

    def test_saved_receipt_must_match_complete_returned_receipt(self):
        returned = {"pid":123, "waited_pid":123, "wait_completed":True, "timed_out":False,
                    "launch_error":None, "actual_exit":0, "expected_exits":[0], "passed":True}
        runner.require_saved_receipt(returned, dict(returned))
        with self.assertRaisesRegex(ValueError, "differs"):
            runner.require_saved_receipt(returned, {**returned, "pid":124})
        with self.assertRaisesRegex(ValueError, "completed"):
            runner.require_saved_receipt({**returned, "wait_completed":False}, {**returned, "wait_completed":False})
        with self.assertRaisesRegex(ValueError, "differs"):
            runner.require_saved_receipt(returned, {**returned, "actual_exit":False})
        with self.assertRaisesRegex(ValueError, "completed"):
            runner.require_saved_receipt({**returned, "actual_exit":False}, {**returned, "actual_exit":False})

    def test_final_reconciliation_rejects_saved_raw_input_or_receipt_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory)
            payloads = {"stdin.raw":b"sealed input", "stdout.raw":b"[]", "stderr.raw":b"",
                        "inputs.json":b"{}\n", "environment-before.json":b"{}\n", "environment-after.json":b"{}\n"}
            receipt = {"pid":123, "waited_pid":123, "wait_completed":True, "timed_out":False,
                       "launch_error":None, "actual_exit":0, "expected_exits":[0], "passed":True}
            for filename, key in (("stdin.raw","stdin_sha256"), ("stdout.raw","stdout_sha256"),
                                  ("stderr.raw","stderr_sha256"), ("inputs.json","inputs_sha256"),
                                  ("environment-before.json","environment_before_sha256"),
                                  ("environment-after.json","environment_after_sha256")):
                (capture / filename).write_bytes(payloads[filename])
                receipt[key] = runner.digest(payloads[filename])
            runner.save(capture / "receipt.json", receipt)
            runner.save(capture / "returned-receipt.json", receipt)
            self.assertEqual(runner.reconcile_capture_files(capture, receipt, payloads["stdin.raw"]), b"[]")
            for filename in ("stdin.raw", "stdout.raw", "inputs.json", "returned-receipt.json"):
                path = capture / filename
                original = path.read_bytes()
                path.write_bytes(original + b"changed")
                with self.assertRaises((ValueError, json.JSONDecodeError)):
                    runner.reconcile_capture_files(capture, receipt, payloads["stdin.raw"])
                path.write_bytes(original)

    def test_partial_or_reordered_api_batch_is_rejected(self):
        selected = [r for r in self.requests if r["target"] == "3.12"]
        rows = [{"request_id": r["request_id"]} for r in selected]
        runner.validate_batch_ids(rows, selected)
        with self.assertRaisesRegex(ValueError, "partial"):
            runner.validate_batch_ids(rows[:-1], selected)
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaisesRegex(ValueError, "out of order"):
            runner.validate_batch_ids(rows, selected)

    def test_output_and_mode_changes_cannot_match(self):
        request = self.plain_success_request()
        actual = self.successful_actual(request, request["expected"][0]["value"]["output"])
        self.assertTrue(all(r["equal"] for r in runner.compare_record(actual, request, [])))
        changed = self.successful_actual(request, actual["output"] + "# changed\n")
        self.assertFalse(all(r["equal"] for r in runner.compare_record(changed, request, [])))
        actual["mode"] = {**actual["mode"], "fast":True}
        with self.assertRaisesRegex(ValueError, "mode"):
            runner.compare_record(actual, request, [])

    def test_nothing_changed_retains_actual_exception_and_no_internal_check_claim(self):
        request = next(r for r in self.requests if len(r["expected"]) == 1
                       and r["expected"][0]["value"].get("kind") == "formatted"
                       and r["expected"][0]["value"].get("reference_exit") == 0
                       and "output" in r["expected"][0]["value"])
        actual = self.successful_actual(request, bytes.fromhex(request["source_hex"]).decode(), unchanged=True)
        self.assertTrue(all(r["equal"] for r in runner.compare_record(actual, request, [])))
        self.assertEqual(actual["exception_type"], "black.report.NothingChanged")
        self.assertFalse(actual["internal_equivalence_and_stability_completed"])
        self.assertEqual(actual["api_calls"]["format_file_contents"], 1)

    def test_error_message_is_not_globally_normalized(self):
        self.assertFalse(runner.message_equal("Cannot parse: changed", "Cannot parse: original",
                                            "black.parsing.InvalidInput", "format_wrapper", [], None))
        self.assertFalse(runner.message_equal("source changed", "source original",
                                            "black.parsing.SourceASTParseError", "format_wrapper", [], None))

    def test_allowed_log_path_requires_retained_hash_for_invalid_output(self):
        marker = runner.LOG_MARKERS[1]
        prefix = "INTERNAL ERROR: Black 26.10.0 on Python (CPython) 3.12.11: "
        actual, expected = prefix + marker + "/tmp/blk_actual.log", prefix + marker + "/old/blk_original.log"
        log = {"marker":marker, "reported_path":"/tmp/blk_actual.log", "sha256":"a" * 64}
        self.assertTrue(runner.message_equal(actual, expected, "black.parsing.InvalidInput", "format_wrapper", [log], "a" * 64))
        self.assertFalse(runner.message_equal(actual, expected, "black.parsing.InvalidInput", "format_wrapper", [log], "b" * 64))
        self.assertFalse(runner.message_equal(actual, expected, "black.parsing.InvalidInput", "literal_nothing_changed", [log], "a" * 64))

    def test_observer_source_compiles_without_execution(self):
        text = runner.observer_source(Path("/hosted/artifact/python312"))
        compile(text, "sealed-black-observer", "exec")
        self.assertEqual(text.count("black.format_file_contents("), 1)
        self.assertIn("fast=False", text)
        self.assertIn("black.decode_bytes(raw, mode)", text)
        self.assertNotIn("exec(", text)

    def fake_observer_loop(self, operation, raw, response=None, decoded=None, error=None):
        """Run only the real controller loop against fake API methods, never source bodies."""
        Target = Enum("Target", {"PY312":312})
        target = Target.PY312
        class NothingChanged(Exception):
            pass
        def mode(**values):
            return SimpleNamespace(**values)
        api = SimpleNamespace(TargetVersion=SimpleNamespace(PY312=target), Mode=mode, NothingChanged=NothingChanged)
        api.decode_bytes = mock.Mock(return_value=decoded if decoded is not None else (raw.decode("utf-8"), "utf-8", "\n"))
        api.format_file_contents = mock.Mock(side_effect=error, return_value=response)
        case = {"request_id":"mock-context", "target":"3.12", "operation":operation, "source_hex":raw.hex()}
        scope = {"black":api, "hashlib":hashlib, "cases":[case], "results":[], "retain":mock.Mock()}
        tree = ast.parse(runner.OBSERVER)
        loop = next(node for node in tree.body if isinstance(node, ast.For)
                    and isinstance(node.iter, ast.Name) and node.iter.id == "cases")
        exec(compile(ast.Module(body=[loop], type_ignores=[]), "mock-black-api-loop", "exec"), scope)
        return scope["results"][0], api, scope["retain"]

    def test_real_observer_loop_uses_file_decoder_before_safe_format(self):
        raw = b"VALUE=1\r\nNEXT=2\r\n"
        decoded = "VALUE=1\nNEXT=2\n"
        row, api, _ = self.fake_observer_loop("black_safe_format_API", raw,
                    response="VALUE = 1\nNEXT = 2\n", decoded=(decoded, "utf-8", "\r\n"))
        self.assertEqual(api.format_file_contents.call_args.args[0], decoded)
        api.decode_bytes.assert_called_once()
        self.assertEqual(bytes.fromhex(row["output_hex"]), b"VALUE = 1\r\nNEXT = 2\r\n")
        self.assertEqual(row["decoded_input_sha256"], runner.digest(decoded.encode()))
        self.assertEqual(row["api_calls"], {"decode_bytes":1, "format_file_contents":1})
        self.assertTrue(row["internal_equivalence_and_stability_completed"])

    def test_real_observer_loop_preserves_BOM_encoding_and_CRLF(self):
        raw = b"\xef\xbb\xbfVALUE=1\r\n"
        row, api, _ = self.fake_observer_loop("black_decode_bytes_safe_format_API", raw,
                    response="VALUE = 1\n", decoded=("VALUE=1\n", "utf-8-sig", "\r\n"))
        self.assertEqual(bytes.fromhex(row["output_hex"]), b"\xef\xbb\xbfVALUE = 1\r\n")
        self.assertEqual(row["encoding"], "utf-8-sig")
        self.assertEqual(row["newline"], "\r\n")
        self.assertEqual(row["api_calls"], {"decode_bytes":1, "format_file_contents":1})
        self.assertEqual(api.format_file_contents.call_args.kwargs["fast"], False)
        self.assertEqual(api.format_file_contents.call_args.kwargs["lines"], ())

    def test_real_observer_loop_keeps_one_actual_error_call(self):
        Error = type("ASTSafetyError", (Exception,), {"__module__":"black.parsing"})
        error = Error("exact saved error")
        row, api, retained = self.fake_observer_loop("black_safe_format_API", b"SOURCE_DATA\n", error=error)
        self.assertEqual(api.format_file_contents.call_count, 1)
        self.assertEqual(row["kind"], "error")
        self.assertEqual(row["exception_type"], "black.parsing.ASTSafetyError")
        self.assertEqual(row["message"], "exact saved error")
        self.assertFalse(row["internal_equivalence_and_stability_completed"])
        retained.assert_called_once_with(error, "mock-context")


if __name__ == "__main__":
    unittest.main()
