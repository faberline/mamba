"""Pure data controls. No original checker or fixture body is executed."""
import ast
import copy
import json
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch

import q0_16_contract_lint as lint
import q0_16_shadow_reference as shadow
from q0_16_packet import load_packets


class ShadowReferenceControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.repo = Path(__file__).resolve().parents[2]
        cls.lock, cases = lint.build_lock(cls.repo)
        cls.case = next(case for case in cases if case["request_id"] == shadow.REQUEST)
        cls.packet, cls.packet_raw = shadow.load_packet(cls.repo, cls.case[shadow.BINDING])

    def values(self, observation_id="collections_abc", root=None):
        root = root or Path("/run/requests") / shadow.REQUEST
        packet, case = copy.deepcopy(self.packet), copy.deepcopy(self.case)
        observation = next(value for value in packet["observations"] if value["id"] == observation_id)
        stdout = bytes.fromhex(observation["stdout_hex"])
        inputs = {"request_id": shadow.REQUEST, "context": lint.public_context(case["context"]),
            "source_aliases": case["source_aliases"], "expected_bindings": case["expected_bindings"],
            "source_snapshot": copy.deepcopy(packet["binding"]["source_snapshot"]), "checker_config": copy.deepcopy(packet["binding"]["checker_config"]),
            "environment": {**packet["binding"]["environment"], "executable": str(root.parent.parent / "environments/mypy/venv/bin/python"),
                "base_executable": "/pinned/python/bin/python3.13"}, "metadata": None}
        arguments = [str(root / "control/mypy.ini") if value == "<CASE_ROOT>/control/mypy.ini" else value for value in packet["command"]["arguments"]]
        receipt = {"argv": [inputs["environment"]["executable"], *arguments], "cwd": str(root / "work"),
            "expected_exits": [2], "pid": 42, "waited_pid": 42, "wait_completed": True, "actual_exit": 2,
            "timed_out": False, "launch_error": None, "passed": True}
        for name, raw in (("stdout", stdout), ("stderr", b""), ("stdin", b""), ("inputs", shadow.encoded(inputs, ordered=True))):
            receipt[name + "_sha256"] = shadow.digest(raw)
        row = lint.initial_row(case, "3.13.5")
        try:
            _, comparisons = lint.compare(lint.Sources(self.repo), case, stdout, 2, root / "work")
            row["status"] = "passed"
        except lint.ComparisonError as error:
            comparisons = error.comparisons
            row["errors"] = [{"stage": "request", "type": type(error).__name__, "message": str(error)}]
            row["status"] = "failed"
        row.update(comparison_results=comparisons, source_files_unchanged=True, clean_tool_health=True,
            receipt_reconciled=True, process_launched=True, receipt=copy.deepcopy(receipt), returned_receipt=copy.deepcopy(receipt))
        return packet, case, root, row, receipt, inputs, stdout, b"", b""

    def test_both_complete_observations_preserve_strict_result_and_error(self):
        for identifier in ("collections", "collections_abc"):
            values = self.values(identifier); row = values[3]; before = copy.deepcopy(row)
            proof = shadow.evaluate(*values)
            self.assertEqual(row, before)
            self.assertTrue(proof["reference_eligible"])
            self.assertEqual(proof["strict_source_match"], identifier == "collections")
            self.assertEqual(proof["native_gate"], "not_run")
            if identifier == "collections_abc":
                self.assertIs(row["comparison_results"][0]["full_ordered_diagnostics_match"], False)
                self.assertIsNone(row["comparison_results"][0]["matched_complete_list_binding"])
                self.assertEqual(row["errors"][0]["type"], "ComparisonError")

    def test_wrong_scope_profile_source_input_and_selection_fail(self):
        changes = [lambda values: values[1].update(request_id="wrong"),
            lambda values: values[1]["context"].update(target="3.12"),
            lambda values: values[3].update(python_version="3.14.0"),
            lambda values: values[1]["source_aliases"][0]["case_source"].update(json_pointer="/15"),
            lambda values: values[1]["context"]["profile_source"].update(sha256="0" * 64),
            lambda values: values[1]["context"]["layout"]["files"]["main.py"].update(sha256="0" * 64),
            lambda values: values[1]["context"]["selection"].update(paths=["."]),
            lambda values: values[5]["source_snapshot"]["main.py"].update(sha256="0" * 64),
            lambda values: values[5]["environment"].update(executable_sha256="0" * 64),
            lambda values: values[4]["argv"].append("extra.py")]
        for change in changes:
            values = list(self.values()); change(values)
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)

    def test_every_diagnostic_field_and_list_boundary_is_exact(self):
        changes = {"file": "collections/other.py", "line": 0, "column": False, "end_line": None,
            "end_column": False, "message": "changed message", "hint": "changed hint", "code": "", "severity": "warning"}
        for key, value in changes.items():
            values = list(self.values()); rows = [json.loads(line) for line in values[6].splitlines()]
            rows[0][key] = value; values[6] = b"\n".join(json.dumps(row).encode() for row in rows) + b"\n"
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)
        for stdout in (b"", b"[]\n", self.values()[6] * 2,
                       self.values()[6].rstrip()[:-1] + b', "extra": 1}\n'):
            values = list(self.values()); values[6] = stdout
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)

    def test_stderr_stdin_exit_and_boolean_process_corruption_fail(self):
        for key, value in (("actual_exit", 0), ("actual_exit", 1), ("actual_exit", True), ("actual_exit", False),
                           ("pid", True), ("waited_pid", True), ("wait_completed", 1), ("timed_out", True), ("passed", 1)):
            values = list(self.values()); values[4][key] = value
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)
        for position in (7, 8):
            values = list(self.values()); values[position] = b"unexpected raw stream"
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)

    def test_strict_false_and_retained_errors_cannot_be_replaced(self):
        for change in (lambda row: row["comparison_results"][0].update(full_ordered_diagnostics_match=True),
                       lambda row: row.update(errors=[]),
                       lambda row: row["errors"].append({"stage": "preservation", "message": "failed"}),
                       lambda row: row.update(source_files_unchanged=False),
                       lambda row: row.update(native_gate="passed")):
            values = list(self.values()); change(values[3])
            with self.assertRaises(ValueError):
                shadow.evaluate(*values)

    def materialize_mock(self, root, values):
        packet, case, _, row, receipt, inputs, stdout, stderr, stdin = values
        command = root / "command"; command.mkdir(parents=True)
        files = {**case["context"]["layout"]["files"], "pyproject.toml": lint.payload(lint.PYPROJECT)}
        for name, fact in packet["binding"]["source_snapshot"].items():
            path = root / "work" / name
            if fact["kind"] == "directory":
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(bytes.fromhex(files[name]["bytes_hex"]))
            path.chmod(fact["mode"])
        (root / "control").mkdir(); (root / "control/mypy.ini").write_bytes(lint.CONFIG_BYTES)
        for name in ("source-before.json", "source-after.json"):
            (root / name).write_bytes(shadow.encoded(packet["binding"]["source_snapshot"]))
        for name, value in (("receipt.json", receipt), ("returned-receipt.json", receipt)):
            (command / name).write_bytes(shadow.encoded(value))
        for name, raw in (("stdout.raw", stdout), ("stderr.raw", stderr), ("stdin.raw", stdin), ("inputs.json", shadow.encoded(inputs, ordered=True))):
            (command / name).write_bytes(raw)

    def test_final_reread_rejects_packet_raw_receipt_source_and_proof_mutation(self):
        for target in ("reference-variant-packet.json", "reference-variant-gate.json", "command/stdout.raw",
                       "command/receipt.json", "command/returned-receipt.json", "command/inputs.json", "source-after.json", "work/main.py"):
            with tempfile.TemporaryDirectory() as folder:
                root = Path(folder) / "run/requests" / shadow.REQUEST
                values = self.values(root=root); self.materialize_mock(root, values)
                row, case = values[3], values[1]
                with patch.object(shadow, "check_identity"):
                    proof = shadow.capture(self.repo, case, root, row)
                    row["independent_shadow_reference"] = proof; row["status"] = "passed_reference_variant"
                    shadow.reconcile(self.repo, case, root, row)
                    path = root / target; path.write_bytes(path.read_bytes() + b" ")
                    with self.assertRaises((ValueError, json.JSONDecodeError)):
                        shadow.reconcile(self.repo, case, root, row)
                self.assertIs(row["comparison_results"][0]["full_ordered_diagnostics_match"], False)
                self.assertEqual(row["errors"][0]["type"], "ComparisonError")
        with tempfile.TemporaryDirectory() as folder:
            repo = Path(folder); path = repo / shadow.PACKET; path.parent.mkdir(parents=True); path.write_bytes(self.packet_raw + b" ")
            with self.assertRaisesRegex(ValueError, "packet bytes changed"):
                shadow.load_packet(repo, self.case[shadow.BINDING])

    def test_reference_status_requires_final_gate_and_keeps_source_count_separate(self):
        row = self.values()[3]
        row["status"] = "passed_reference_variant"
        row["independent_shadow_reference"] = {"reference_eligible": True, "classification": "approved_reference_variant", "strict_source_match": False}
        self.assertFalse(shadow.reference_pass(row))
        row["independent_shadow_final_reconciled"] = True
        self.assertTrue(shadow.reference_pass(row))
        self.assertNotEqual(row["status"], "passed")
        row["status"] = "failed"; self.assertFalse(shadow.reference_pass(row))

    def test_real_identity_hash_and_interpreter_paths_are_bound(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "run/requests" / shadow.REQUEST
            values = self.values(root=root); packet, inputs = values[0], values[5]
            identity = {"executable": inputs["environment"]["executable"],
                "base_identity": {"executable": inputs["environment"]["base_executable"]}}
            path = root.parent.parent / "environments/mypy/identity.json"; path.parent.mkdir(parents=True)
            raw = shadow.encoded(identity); path.write_bytes(raw)
            packet["binding"]["environment"]["identity_sha256"] = shadow.digest(raw)
            shadow.check_identity(root, inputs, packet)
            inputs["environment"]["base_executable"] = "/foreign/python"
            with self.assertRaisesRegex(ValueError, "interpreter paths differ"):
                shadow.check_identity(root, inputs, packet)
            inputs["environment"]["base_executable"] = identity["base_identity"]["executable"]
            path.write_bytes(raw + b" ")
            with self.assertRaisesRegex(ValueError, "identity changed"):
                shadow.check_identity(root, inputs, packet)

    def test_original_comparator_and_original_denominators_are_unchanged(self):
        path = self.repo / "tools/ci/q0_16_contract_lint.py"; raw = path.read_bytes()
        node = next(node for node in ast.parse(raw).body if isinstance(node, ast.FunctionDef) and node.name == "compare")
        fragment = b"".join(raw.splitlines(keepends=True)[node.lineno - 1:node.end_lineno])
        self.assertEqual(len(fragment), 6749)
        self.assertEqual(shadow.digest(fragment), "74825a58b2b66b8359fbeb548bdb915ddae372adb47996d1ec36535056ee8bab")
        self.assertEqual(self.lock["request_counts"], {"mypy": 2982, "pylint": 1380})
        gated = [case for case in self.lock["requests"] if shadow.BINDING in case]
        self.assertEqual(len(gated), 1)
        self.assertEqual(len(gated[0]["source_aliases"]), 1)
        self.assertEqual(self.packet["original_contexts_added"], 0)
        self.assertEqual(self.packet["original_aliases_added"], 0)

    def test_nested_reference_packet_preserves_original_supplement_discovery(self):
        self.assertEqual(shadow.PACKET, "tools/ci/q0_16_cases/reference/mypy_shadow_order.json")
        self.assertTrue((self.repo / shadow.PACKET).is_file())
        self.assertFalse((self.repo / "tools/ci/q0_16_cases/mypy_shadow_order.json").exists())
        self.assertEqual(shadow.digest((self.repo / "tools/ci/q0_16_packet.py").read_bytes()),
            "fc3ca14da898de1227e3614273a06c82068d1241e3485ca94cf0d50fbbc7e166")
        profile = shadow.read_json((self.repo / "crates/mamba-quality/assets/profile.json").read_bytes())
        packets = load_packets(self.repo, profile)
        self.assertEqual([row["packet"]["tool"] for row in packets], ["mypy", "pydantic", "pylint"])
        self.assertEqual({row["proof"]["tool"]: row["proof"]["case_count"] for row in packets}, {"mypy": 7, "pydantic": 9, "pylint": 327})
        self.assertEqual(sum(row["proof"]["case_count"] for row in packets), 343)
        self.assertEqual(sum(row["proof"]["planned_target_cells"] for row in packets), 1029)
        self.assertEqual(sum(len(row["proof"]["disabled_rule_pairs"]) for row in packets), 136)


if __name__ == "__main__":
    unittest.main()
