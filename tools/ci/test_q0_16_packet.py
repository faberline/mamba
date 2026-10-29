"""Pure input-contract controls. No case or reference tool is executed."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

from q0_16_packet import (CONFIG_BYTES, evaluate_predicates, materialize, read_json,
                         safe_path, validate_case, validate_packet)

PROFILE = {"mypy": {"strict_flags": ["--warn-redundant-casts"]},
           "pydantic": {"init_typed": True},
           "pylint": {"rules": [{"code": "E0602"}, {"code": "F0001"}]}}


def case(tool="mypy", role="hit", ident="case-one"):
    behavior = {"mypy": "--warn-redundant-casts", "pydantic": "pydantic.init_typed", "pylint": "E0602"}[tool]
    rule = "E0602" if tool == "pylint" else "redundant-cast"
    return {"id": ident, "behavior": behavior, "role": role, "group": "one-boundary",
            "files": {"main.py": "value = missing_name\n"}, "entrypoints": ["main.py"],
            "python_targets": ["3.12", "3.13", "3.14"],
            "requires": [{"rule": rule, "presence": "absent" if role == "non_trigger" else "present"}],
            "intent": "A declared static control; this test does not run its source.",
            "provenance": {"kind": "unit-control", "reference_executed_at_authoring": False}}


def packet(tool="mypy", cases=None):
    return {"schema": "mamba.q0.16.supplement-inputs.v1", "tool": tool,
            "cases": [case(tool)] if cases is None else cases, "dispositions": []}


class PacketControls(unittest.TestCase):
    def test_path_traversal_and_cli_option_inputs_are_rejected(self):
        for name in ("../escape.py", "/absolute.py", "x/../escape.py", "x//y.py", "x/./y.py",
                     "x\\y.py", "--config-file=evil.py", "@args.py", "x\n.py"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                safe_path(name)

    def test_duplicate_json_fields_are_not_silently_dropped(self):
        with self.assertRaises(ValueError):
            read_json(b'{"cases": [], "cases": [1]}')
        with self.assertRaises(ValueError):
            read_json(b'{"number": NaN}')

    def test_zero_case_denominator_is_rejected(self):
        with self.assertRaises(ValueError):
            validate_packet(packet(cases=[]), PROFILE)

    def test_empty_predicates_cannot_pass(self):
        value = case()
        value["requires"] = []
        with self.assertRaises(ValueError):
            validate_case(value, "mypy", PROFILE)
        with self.assertRaises(ValueError):
            evaluate_predicates([], [])

    def test_config_and_reference_package_shadowing_are_rejected(self):
        for name in ("mypy.ini", "pydantic.py", "pydantic/__init__.py", "mypy/__init__.py"):
            value = case()
            value["files"][name] = "# not executed\n"
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate_case(value, "mypy", PROFILE)

    def test_file_directory_collision_is_rejected(self):
        value = case()
        value["files"]["main.py/child.py"] = "pass\n"
        with self.assertRaises(ValueError):
            validate_case(value, "mypy", PROFILE)

    def test_missing_entrypoint_requires_explicit_fatal_boundary(self):
        value = case("pylint")
        value["entrypoints"] = ["missing.py"]
        with self.assertRaises(ValueError):
            validate_case(value, "pylint", PROFILE)
        value.update(behavior="F0001", files={}, missing_entrypoints=["missing.py"],
                     requires=[{"rule": "F0001", "presence": "present"}])
        proof = validate_case(value, "pylint", PROFILE)
        self.assertEqual(proof["source_files"], {})
        value["requires"][0]["presence"] = "absent"
        with self.assertRaises(ValueError):
            validate_case(value, "pylint", PROFILE)

    def test_missing_path_cannot_be_a_materialized_directory(self):
        value = case("pylint")
        value.update(behavior="F0001", files={"missing.py/child.py": "pass\n"},
                     entrypoints=["missing.py"], missing_entrypoints=["missing.py"],
                     requires=[{"rule": "F0001", "presence": "present"}])
        with self.assertRaises(ValueError):
            validate_case(value, "pylint", PROFILE)

    def test_suppression_requires_a_fresh_identical_hit_partner(self):
        hit = case("pylint", ident="hit")
        negative = case("pylint", "non_trigger", "control")
        negative["disabled_rules"] = ["E0602"]
        with self.assertRaises(ValueError):
            validate_packet(packet("pylint", [negative]), PROFILE)
        proof = validate_packet(packet("pylint", [hit, negative]), PROFILE)
        self.assertEqual(proof["disabled_rule_pairs"], {"control": "hit"})
        negative["files"]["main.py"] = "# unrelated silent file\n"
        with self.assertRaises(ValueError):
            validate_packet(packet("pylint", [hit, negative]), PROFILE)

    def test_suppression_cannot_disable_all_or_a_different_rule(self):
        negative = case("pylint", "non_trigger")
        for disabled in (["all"], ["F0001"], ["E0602", "F0001"]):
            negative["disabled_rules"] = disabled
            with self.subTest(disabled=disabled), self.assertRaises(ValueError):
                validate_case(negative, "pylint", PROFILE)

    def test_source_hash_mismatch_and_claimed_outputs_are_rejected(self):
        value = case()
        value["provenance"]["source_sha256"] = {"main.py": "0" * 64}
        with self.assertRaises(ValueError):
            validate_case(value, "mypy", PROFILE)
        for key, content in (("expected", []), ("argv", ["sh"]), ("extra_args", ["--help"])):
            value = case()
            value[key] = content
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_case(value, "mypy", PROFILE)

    def test_ids_and_target_sets_cannot_collapse_the_denominator(self):
        value = case()
        with self.assertRaises(ValueError):
            validate_packet(packet(cases=[value, copy.deepcopy(value)]), PROFILE)
        value["python_targets"] = []
        with self.assertRaises(ValueError):
            validate_case(value, "mypy", PROFILE)

    def test_materialization_keeps_text_and_adds_only_pinned_config(self):
        value = case()
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary).resolve() / "work"
            proof = materialize(value, "mypy", project)
            self.assertEqual((project / "main.py").read_text(), value["files"]["main.py"])
            self.assertEqual((project / "mypy.ini").read_bytes(), CONFIG_BYTES)
            self.assertEqual(proof["case_unit_denominator"], 1)
            with self.assertRaises(FileExistsError):
                materialize(value, "mypy", project)

    def test_predicates_keep_exact_sites_and_message_text(self):
        diagnostics = [{"rule": "misc", "path": "main.py", "message": "Exact Name", "location": {"line": 2}}]
        predicates = [{"rule": "misc", "presence": "present", "path": "main.py", "line": 2,
                       "message_contains": "Exact Name"},
                      {"rule": "misc", "presence": "absent", "path": "main.py", "line": 3},
                      {"rule": "misc", "presence": "present", "message_contains": "exact name"}]
        result = evaluate_predicates(diagnostics, predicates)
        self.assertEqual([item["passed"] for item in result], [True, True, False])
        self.assertEqual(result[0]["matching_diagnostic_indices"], [0])

    def test_wildcard_cannot_be_present_global_or_message_filtered(self):
        valid = {"rule": "*", "presence": "absent", "severity": "error", "path": "main.py", "line": 1}
        for changes in ({"presence": "present"}, {"line": 0}, {"severity": "note"},
                        {"message_contains": "only one message"}):
            value = case()
            value["requires"] = [{**valid, **changes}]
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_case(value, "mypy", PROFILE)
        for missing in ("line", "path", "severity"):
            value = case()
            value["requires"] = [{key: item for key, item in valid.items() if key != missing}]
            with self.subTest(missing=missing), self.assertRaises(ValueError):
                validate_case(value, "mypy", PROFILE)

    def test_bounded_wildcard_rejects_unrelated_error_at_its_site(self):
        predicate = {"rule": "*", "presence": "absent", "severity": "error", "path": "main.py", "line": 1}
        messages = [{"rule": "call-arg", "severity": "error", "path": "main.py", "message": "unrelated error",
                     "location": {"line": 1}}]
        self.assertFalse(evaluate_predicates(messages, [predicate])[0]["passed"])
        messages[0]["location"]["line"] = 2
        self.assertTrue(evaluate_predicates(messages, [predicate])[0]["passed"])
        messages[0].update(severity="note", location={"line": 1})
        self.assertTrue(evaluate_predicates(messages, [predicate])[0]["passed"])


if __name__ == "__main__":
    unittest.main()
