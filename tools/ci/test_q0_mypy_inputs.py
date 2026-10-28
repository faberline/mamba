"""Negative controls for fixed input identity and exact diagnostic projection."""
import json
from pathlib import Path
import tempfile
import unittest

from q0_mypy_inputs import (CASE_COUNT, INPUT_SHA256, materialize,
                            parse_diagnostics, prepare_inputs)


class FixedInputs(unittest.TestCase):
    def test_complete_public_input_bytes_match_original_pin(self):
        repo = Path(__file__).resolve().parents[2]
        cases, proof = prepare_inputs(repo)
        self.assertEqual(len(cases), CASE_COUNT)
        self.assertEqual(proof["original_input_sha256"], INPUT_SHA256)
        self.assertEqual(proof["original_input_bytes"], 6335487)
        self.assertEqual(proof["unique_source_hashes"], 21652)
        self.assertEqual((proof["py_files"], proof["pyi_files"]), (18822, 4076))
        self.assertTrue(all("expected" not in c for c in cases))
        self.assertEqual(list(cases[1177])[:3], ["name", "extension", "source"])

    def test_zero_and_partial_materialization_are_rejected(self):
        with tempfile.TemporaryDirectory() as parent:
            for rows in ([], [{"name": "one"}]):
                with self.assertRaises(ValueError):
                    materialize(rows, Path(parent) / "project")
            self.assertFalse((Path(parent) / "project").exists())


class ExactProjection(unittest.TestCase):
    root = Path("/fixed/project")

    def raw(self, **changes):
        row = {"file": "case_0000.py", "line": 2, "column": 0,
               "end_line": 2, "end_column": -1, "code": None,
               "message": "object at 0x1234", "hint": None, "severity": "note"}
        row.update(changes)
        return json.dumps(row).encode()

    def test_preserves_order_nulls_messages_and_negative_or_zero_end_column(self):
        raw = self.raw(file=str(self.root / "case_0000.py")) + b"\n" + self.raw(end_column=0)
        grouped = parse_diagnostics(raw, {"case_0000.py", "case_0001.pyi"}, self.root)
        self.assertEqual(grouped["case_0001.pyi"], [])
        first, second = grouped["case_0000.py"]
        self.assertEqual(first["message"], "object at 0x1234")
        self.assertIsNone(first["rule"])
        self.assertIsNone(first["hint"])
        self.assertEqual(first["severity"], "note")
        self.assertEqual(first["location"], {"line": 2, "column": 1, "end_line": 2, "end_column": -1})
        self.assertEqual(second["location"]["end_column"], 0)

    def test_no_json_or_blank_line_filter(self):
        for raw in (b"Found 1 error", self.raw() + b"\n\n", self.raw() + b"\nstatus"):
            with self.subTest(raw=raw), self.assertRaises((ValueError, json.JSONDecodeError)):
                parse_diagnostics(raw, {"case_0000.py"}, self.root)

    def test_unknown_and_outside_paths_are_not_discarded(self):
        for filename in ("other.py", "./case_0000.py", "/fixed/project-other/case_0000.py"):
            with self.subTest(filename=filename), self.assertRaises(ValueError):
                parse_diagnostics(self.raw(file=filename), {"case_0000.py"}, self.root)

    def test_missing_fields_are_not_synthesized(self):
        row = json.loads(self.raw())
        del row["hint"]
        with self.assertRaises(KeyError):
            parse_diagnostics(json.dumps(row).encode(), {"case_0000.py"}, self.root)

    def test_literal_unicode_line_separators_stay_inside_the_json_message(self):
        row = json.loads(self.raw())
        row["message"] = "left\u0085middle\u2028right\u2029end"
        raw = json.dumps(row, ensure_ascii=False).encode()
        grouped = parse_diagnostics(raw, {"case_0000.py"}, self.root)
        self.assertEqual(grouped["case_0000.py"][0]["message"], row["message"])

    def test_zero_input_denominator_is_rejected(self):
        with self.assertRaises(ValueError):
            parse_diagnostics(b"", set(), self.root)


if __name__ == "__main__":
    unittest.main()
