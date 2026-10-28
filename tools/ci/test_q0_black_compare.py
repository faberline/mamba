"""Pure negative controls. No Black or native engine is imported or run."""
import copy
import hashlib
import unittest

from q0_black_compare import (compare_records, compare_secondary_fields,
                              validate_batch, validate_record)


class BlackRecordControls(unittest.TestCase):
    case = {"source": "x=1\n"}

    def success(self, output="x = 1\n", **changes):
        row = {"output": output, "sha256": hashlib.sha256(output.encode()).hexdigest(),
               "reference_converges": True, "target_syntax_hint": None,
               "reference_check_exit": int(output != self.case["source"]), "newline": "\n"}
        row.update(changes)
        return row

    def error(self, message="InvalidInput: Cannot parse: 1:2: x=", **changes):
        row = {"reference_error": message, "target_syntax_hint": "invalid syntax (<unknown>, line 1)",
               "reference_check_exit": 123}
        row.update(changes)
        return row

    def safety(self, path="/tmp/blk_first_1.log", prefix="INTERNAL ERROR: fixed.  "):
        return self.error("ASTSafetyError: " + prefix + "This diff might be helpful: " + path)

    def test_valid_success_error_and_unchanged_computed_status(self):
        for row in (self.success(), self.success("x=1\n"), self.error()):
            with self.subTest(row=row):
                self.assertEqual(validate_record(row, self.case), [])
                self.assertTrue(compare_records(row, row)["equal"])

    def test_output_byte_changes_are_never_normalized(self):
        expected = self.success()
        for output in ("x = 1\r\n", "x = 1", "x = 1 \n", "x = 1\n\n", "x = １\n"):
            with self.subTest(output=output):
                row = self.success(output)
                self.assertFalse(compare_records(row, expected)["equal"])
                self.assertFalse(compare_records(row, expected)["approved_path_variant"])

    def test_normal_errors_match_exact_class_and_message_split_once(self):
        expected = self.error("InvalidInput: detail: same remainder")
        self.assertTrue(compare_records(self.error(expected["reference_error"]), expected)["equal"])
        for message in ("OtherError: detail: same remainder", "InvalidInput: detail: other",
                        "InvalidInput: detail: same remainder ", "InvalidInput:detail"):
            with self.subTest(message=message):
                self.assertFalse(compare_records(self.error(message), expected)["equal"])

    def test_ast_safety_existing_basename_rule_only(self):
        expected = self.safety()
        for path in ("/tmp/blk_other_2.log", "blk_A0_z.log", "/other/root/blk_9.log"):
            with self.subTest(path=path):
                result = compare_records(self.safety(path), expected)
                self.assertTrue(result["equal"])
                self.assertTrue(result["approved_path_variant"])
        self.assertFalse(compare_records(expected, expected)["approved_path_variant"])

    def test_ast_safety_bad_suffix_and_prefix_are_rejected_even_if_identical(self):
        for path in ("/tmp/blk_.log", "/tmp/blk_a-b.log", "/tmp/blk_é.log", "/tmp/blk_١.log",
                     "/tmp/other_a.log", "/tmp/blk_a.txt", "/tmp/blk_a.log trailing"):
            with self.subTest(path=path):
                row = self.safety(path)
                self.assertFalse(compare_records(row, self.safety())["equal"])
                self.assertFalse(compare_records(row, row)["equal"])
        self.assertFalse(compare_records(self.safety(prefix="changed.  "), self.safety())["equal"])

    def test_invalid_output_marker_and_other_error_classes_never_get_path_rule(self):
        old = self.error("ASTSafetyError: bad. This invalid output might be helpful: /tmp/blk_a.log")
        new = self.error("ASTSafetyError: bad. This invalid output might be helpful: /tmp/blk_b.log")
        self.assertFalse(compare_records(old, old)["equal"])
        self.assertFalse(compare_records(new, old)["equal"])
        old = self.error("InvalidInput: bad. This diff might be helpful: /tmp/blk_a.log")
        new = self.error("InvalidInput: bad. This diff might be helpful: /tmp/blk_b.log")
        self.assertFalse(compare_records(new, old)["equal"])

    def test_last_marker_has_rust_rsplit_semantics(self):
        prefix = "first This diff might be helpful: /tmp/blk_prior.log then "
        self.assertTrue(compare_records(self.safety("/tmp/blk_a.log", prefix),
                                        self.safety("/tmp/blk_b.log", prefix))["equal"])
        self.assertFalse(compare_records(self.safety("/tmp/blk_a.log", prefix + "other "),
                                         self.safety("/tmp/blk_b.log", prefix))["equal"])

    def test_success_and_error_are_distinct(self):
        self.assertFalse(compare_records(self.error(), self.success())["equal"])
        self.assertFalse(compare_records(self.success(), self.error())["equal"])

    def test_exact_field_schema_and_order_are_required(self):
        original = self.success()
        rows = [{}, {**original, "extra": None}, dict(reversed(list(original.items()))),
                {key: value for key, value in original.items() if key != "newline"},
                {**original, "reference_error": "InvalidInput: mixed"}]
        for row in rows:
            with self.subTest(row=row):
                self.assertTrue(validate_record(row, self.case))
                self.assertFalse(compare_records(row, original)["equal"])

    def test_wrong_hash_and_computed_status_fail_validation(self):
        for row in (self.success(sha256="0" * 64), self.success(sha256="F" * 64),
                    self.success(reference_check_exit=0), self.success("x=1\n", reference_check_exit=1),
                    self.error(reference_check_exit=0)):
            with self.subTest(row=row):
                self.assertTrue(validate_record(row, self.case))

    def test_bool_is_not_an_integer_status_and_scalar_types_stay_exact(self):
        for changes in ({"reference_check_exit": True}, {"reference_check_exit": 1.0},
                        {"reference_converges": 1}, {"target_syntax_hint": 1},
                        {"newline": ""}, {"newline": "LF"}, {"sha256": None}, {"output": None}):
            with self.subTest(changes=changes):
                row = self.success()
                row.update(changes)
                self.assertTrue(validate_record(row, self.case))
        self.assertTrue(validate_record(self.error(reference_check_exit=True), self.case))

    def test_nonconvergence_is_preserved_as_valid_recorded_false(self):
        actual = self.success(reference_converges=False)
        expected = self.success()
        self.assertEqual(validate_record(actual, self.case), [])
        self.assertTrue(compare_records(actual, expected)["equal"])
        secondary = compare_secondary_fields(actual, expected)
        self.assertFalse(secondary["equal"])
        self.assertEqual(secondary["differing_fields"], ["reference_converges"])
        self.assertIs(actual["reference_converges"], False)

    def test_secondary_metadata_never_changes_primary_comparison(self):
        expected = self.success()
        actual = self.success(sha256="0" * 64, target_syntax_hint="other", reference_check_exit=0,
                              newline="\r\n")
        self.assertTrue(compare_records(actual, expected)["equal"])
        self.assertTrue(validate_record(actual, self.case))
        self.assertEqual(compare_secondary_fields(actual, expected)["differing_fields"],
                         ["sha256", "target_syntax_hint", "reference_check_exit", "newline"])
        actual = self.error(target_syntax_hint=None)
        self.assertTrue(compare_records(actual, self.error())["equal"])
        self.assertFalse(compare_secondary_fields(actual, self.error())["equal"])

    def test_utf8_bytes_and_unpaired_surrogates(self):
        row = self.success("value = 'é'\n")
        self.assertEqual(validate_record(row, self.case), [])
        bad = self.success()
        bad["output"] = "\ud800"
        self.assertTrue(validate_record(bad, self.case))
        self.assertFalse(compare_records(bad, row)["equal"])
        self.assertTrue(validate_record(self.success(), {"source": "\ud800"}))
        self.assertTrue(validate_record(self.error(target_syntax_hint="\ud800"), self.case))

    def test_empty_partial_and_invalid_batches_fail_closed(self):
        self.assertTrue(validate_batch([], []))
        self.assertTrue(validate_batch([], [self.case]))
        self.assertTrue(validate_batch([self.success()], []))
        self.assertTrue(validate_batch([{}], [self.case]))
        self.assertTrue(validate_batch({}, [self.case]))
        self.assertEqual(validate_batch([self.success()], [self.case]), [])

    def test_functions_do_not_change_any_input_record(self):
        actual, expected = self.safety(), self.safety("/tmp/blk_other.log")
        before = copy.deepcopy((actual, expected, self.case))
        validate_record(actual, self.case)
        compare_records(actual, expected)
        compare_secondary_fields(actual, expected)
        self.assertEqual((actual, expected, self.case), before)


if __name__ == "__main__":
    unittest.main()
