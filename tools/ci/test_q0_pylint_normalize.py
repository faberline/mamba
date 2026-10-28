"""Pure tests for the published comparator; no Pylint or product execution."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from q0_pylint_normalize import PylintComparator, normalize_diagnostics

REPOSITORY = Path(__file__).resolve().parents[2]
PREFIX = "error while code parsing: Wrong or no encoding specified for "


def diagnostic(rule="F0010", message=None, path="main.py"):
    return {"rule": rule, "message": message if message is not None else PREFIX + path + ".",
            "hint": None, "path": path, "severity": "fatal",
            "location": {"line": 1, "column": 1, "end_line": 1, "end_column": 1}}


def crash(first="main.py", second="main.py", cache="/cache/", timestamp="2026-10-11-00-00-00"):
    return (f"{first}: Fatal error while checking '{second}'. Please open an issue in our bug tracker so we address this. "
            f"There is a pre-filled template that you can use in '{cache}pylint-crash-{timestamp}.txt'.")


class Normalizers(unittest.TestCase):
    def test_encoding_accepts_relative_and_slash_bounded_prefix(self):
        for embedded in ["main.py", "/old/project/main.py", "relative/main.py"]:
            original = [diagnostic(message=PREFIX + embedded + ".")]
            actual = normalize_diagnostics(original)
            self.assertEqual(actual[0]["message"], PREFIX + "<project>/main.py.")
            self.assertEqual(original[0]["message"], PREFIX + embedded + ".")

    def test_encoding_rejects_wrong_path_and_template(self):
        for message in [PREFIX + "/project/other.py.", PREFIX + "/project/notmain.py.",
                        PREFIX + "/project/main.py", PREFIX + "/project/main.py..",
                        PREFIX + "/project/main.py.\n", PREFIX.replace("error", "Error") + "main.py.",
                        PREFIX + r"C:\project\main.py."]:
            with self.subTest(message=message):
                original = [diagnostic(message=message)]
                self.assertEqual(normalize_diagnostics(original), original)

    def test_nested_path_boundary_is_exact(self):
        valid = diagnostic(message=PREFIX + "/project/pkg/main.py.", path="pkg/main.py")
        self.assertEqual(normalize_diagnostics([valid])[0]["message"], PREFIX + "<project>/pkg/main.py.")
        invalid = diagnostic(message=PREFIX + "/project/notpkg/main.py.", path="pkg/main.py")
        self.assertEqual(normalize_diagnostics([invalid]), [invalid])

    def test_encoding_does_not_change_other_rule(self):
        original = [diagnostic(rule="F9999", message=PREFIX + "/root/main.py.")]
        self.assertEqual(normalize_diagnostics(original), original)

    def test_crash_changes_only_approved_message_fields(self):
        original = [diagnostic("F0002", crash("/old/project/main.py", "/old/project/main.py"))]
        expected = copy.deepcopy(original)
        expected[0]["message"] = crash(cache="<cache>/", timestamp="<timestamp>")
        self.assertEqual(normalize_diagnostics(original), expected)
        self.assertEqual(original[0]["message"], crash("/old/project/main.py", "/old/project/main.py"))

    def test_crash_accepts_exact_timestamp_pattern_and_optional_cache(self):
        # The Rust regex checks the digit shape; it does not parse a calendar date.
        for cache in ["", "cache/", "/cache/deep/"]:
            original = [diagnostic("F0002", crash(cache=cache, timestamp="9999-99-99-99-99-99"))]
            self.assertEqual(normalize_diagnostics(original)[0]["message"], crash(cache="<cache>/", timestamp="<timestamp>"))

    def test_crash_rejects_wrong_path_template_or_timestamp(self):
        message = crash()
        changed = [crash("other.py"), crash(second="other.py"), crash("notmain.py"),
                   message.replace("Please open", "Please create"),
                   message.replace("pylint-crash-", "other-crash-"),
                   message.replace("2026-10-11", "26-10-11"), message + "\n",
                   message + " address=0x123", message.replace(".txt'.", ".txt'")]
        for text in changed:
            with self.subTest(message=text):
                original = [diagnostic("F0002", text)]
                self.assertEqual(normalize_diagnostics(original), original)

    def test_unrelated_address_path_order_and_location_are_unchanged(self):
        original = [diagnostic("F0010", "unexpected address 0x123 at /root/main.py"),
                    diagnostic("F0002", "other fatal at 0xABC"), diagnostic("W0001", "warning")]
        self.assertEqual(normalize_diagnostics(original), original)


class Comparator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.comparator = PylintComparator(REPOSITORY)
        path = REPOSITORY / "crates/mamba-quality/src/infrastructure/pylint/tests/variants.json"
        cls.fixture = json.loads(path.read_bytes())

    def variant_case(self, entry):
        exit_code = entry["runs"][0]["exit_code"]
        return {"name": entry["name"], "files": {}, "source_sha256": copy.deepcopy(entry["source_hashes"]),
                "expected": [[], [], []], "reference_exits": [exit_code] * 3}

    def canonical_case(self, expected, exit_code=1):
        return {"name": "pure-control", "files": {}, "source_sha256": {},
                "expected": [expected, expected, expected], "reference_exits": [exit_code] * 3}

    def test_canonical_normalizes_both_sides(self):
        expected = [diagnostic(message=PREFIX + "/original/main.py."), diagnostic("F0002", crash(cache="/old/"))]
        actual = [diagnostic(message=PREFIX + "/current/main.py."),
                  diagnostic("F0002", crash(cache="/new/", timestamp="2030-01-02-03-04-05"))]
        before = copy.deepcopy(actual)
        result = self.comparator.compare(self.canonical_case(expected), 12, actual, 1)
        self.assertTrue(result["matched"])
        self.assertEqual(result["matched_by"], "canonical")
        self.assertEqual(result["normalized_actual"], result["normalized_expected"])
        self.assertEqual(actual, before)

    def test_canonical_requires_exit_and_all_fields(self):
        expected = [diagnostic()]
        case = self.canonical_case(expected)
        self.assertFalse(self.comparator.compare(case, 12, expected, 0)["matched"])
        for field, value in [("path", "other.py"), ("severity", "warning"), ("hint", "new"), ("rule", "F9999")]:
            actual = copy.deepcopy(expected)
            actual[0][field] = value
            self.assertFalse(self.comparator.compare(case, 12, actual, 1)["matched"])
        actual = copy.deepcopy(expected)
        actual[0]["location"]["column"] = 2
        self.assertFalse(self.comparator.compare(case, 12, actual, 1)["matched"])

    def test_json_boolean_does_not_equal_integer(self):
        expected = [diagnostic()]
        actual = copy.deepcopy(expected)
        actual[0]["location"]["column"] = True
        self.assertFalse(self.comparator.compare(self.canonical_case(expected), 12, actual, 1)["matched"])
        self.assertFalse(self.comparator.compare(self.canonical_case([], 0), 12, [], False)["matched"])

    def test_unapproved_list_order_fails(self):
        expected = [diagnostic("W0001", "first"), diagnostic("W0002", "second")]
        self.assertFalse(self.comparator.compare(self.canonical_case(expected, 4), 12, list(reversed(expected)), 4)["matched"])

    def test_all_87_published_whole_observations_match(self):
        observations = 0
        self.assertEqual(len(self.fixture["cases"]), 29)
        for entry in self.fixture["cases"]:
            case = self.variant_case(entry)
            minor = int(entry["python"].split(".")[1])
            for run in entry["runs"]:
                result = self.comparator.compare(case, minor, run["diagnostics"], run["exit_code"])
                self.assertEqual(result["matched_by"], "approved_whole_list_variant")
                observations += 1
        self.assertEqual(observations, 87)

    def test_canonical_match_takes_priority(self):
        entry = self.fixture["cases"][0]
        case = self.variant_case(entry)
        run = entry["runs"][0]
        case["expected"] = [run["diagnostics"]] * 3
        self.assertEqual(self.comparator.compare(case, 13, run["diagnostics"], run["exit_code"])["matched_by"], "canonical")

    def test_variant_requires_exact_name_target_source_and_exit(self):
        entry = self.fixture["cases"][0]  # This case has no 3.12 approval.
        run = entry["runs"][0]
        case = self.variant_case(entry)
        for changed in [dict(case, name="unapproved"), dict(case, source_sha256={"main.py": "wrong"}),
                        dict(case, reference_exits=[0, 0, 0])]:
            self.assertFalse(self.comparator.compare(changed, 13, run["diagnostics"], run["exit_code"])["matched"])
        self.assertFalse(self.comparator.compare(case, 12, run["diagnostics"], run["exit_code"])["matched"])
        self.assertFalse(self.comparator.compare(case, 13, run["diagnostics"], run["exit_code"] ^ 1)["matched"])

    def test_variant_rejects_each_unapproved_diagnostic_field(self):
        entry = self.fixture["cases"][0]
        run = entry["runs"][0]
        case = self.variant_case(entry)
        for field in ["message", "rule", "path", "severity", "hint"]:
            actual = copy.deepcopy(run["diagnostics"])
            actual[0][field] = "unapproved"
            self.assertFalse(self.comparator.compare(case, 13, actual, run["exit_code"])["matched"])
        for field in ["line", "column", "end_line", "end_column"]:
            actual = copy.deepcopy(run["diagnostics"])
            actual[0]["location"][field] = -999
            self.assertFalse(self.comparator.compare(case, 13, actual, run["exit_code"])["matched"])

    def test_observed_diagnostics_cannot_be_combined_into_new_lists(self):
        entry = self.fixture["cases"][0]
        case = self.variant_case(entry)
        first, second = entry["runs"]
        actual = copy.deepcopy(first["diagnostics"])
        actual[1] = copy.deepcopy(second["diagnostics"][1])
        self.assertNotIn(actual, [run["diagnostics"] for run in entry["runs"]])
        self.assertFalse(self.comparator.compare(case, 13, actual, first["exit_code"])["matched"])

    def test_variant_rejects_deleted_added_or_unapproved_reordered_rows(self):
        entry = self.fixture["cases"][0]
        case = self.variant_case(entry)
        run = entry["runs"][0]
        for actual in [run["diagnostics"][:-1], run["diagnostics"] + run["diagnostics"][:1],
                       list(reversed(run["diagnostics"]))]:
            self.assertFalse(self.comparator.compare(case, 13, actual, run["exit_code"])["matched"])

    def test_fatal_variation_is_only_canonical(self):
        self.assertFalse(any(d["rule"] == "F0002" for c in self.fixture["cases"] for r in c["runs"] for d in r["diagnostics"]))
        entry = self.fixture["cases"][0]
        actual = [diagnostic("F0002", crash())]
        result = self.comparator.compare(self.variant_case(entry), 13, actual, entry["runs"][0]["exit_code"])
        self.assertFalse(result["matched"])

    def test_fixture_is_cached_after_verified_load(self):
        case = self.canonical_case([])
        with patch.object(Path, "read_bytes", side_effect=AssertionError("unexpected re-read")):
            self.assertTrue(self.comparator.compare(case, 12, [], 1)["matched"])

    def test_changed_fixture_bytes_fail_before_use(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            destination = repo / "crates/mamba-quality/src/infrastructure/pylint/tests/variants.json"
            destination.parent.mkdir(parents=True)
            destination.write_text(json.dumps(self.fixture) + "\n")
            with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
                PylintComparator(repo)


if __name__ == "__main__":
    unittest.main()
