"""Isolated receipt-parser controls; never invokes Cargo or product binaries."""
import unittest

from test_receipt import counts, exact_inventory_matches, inventory_names, source_identity_matches


class ReceiptControls(unittest.TestCase):
    inventory = "alpha: test\nbeta: test\n\n2 tests, 0 benchmarks\n"
    good = ("test alpha ... ok\ntest beta ... ok\n"
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n")

    def test_complete_pass(self):
        result, valid = counts(self.inventory, self.good)
        self.assertTrue(valid)
        self.assertEqual(result["not_run"], 0)
        self.assertEqual(result["passed"], 2)

    def test_zero_inventory_blocks(self):
        self.assertFalse(counts("", self.good)[1])

    def test_incomplete_inventory_blocks(self):
        self.assertFalse(counts("alpha: test\nbeta: test\n", self.good)[1])

    def test_inventory_total_mismatch_blocks(self):
        self.assertFalse(counts(self.inventory.replace("2 tests", "3 tests"), self.good)[1])

    def test_missing_required_case_blocks_inventory(self):
        self.assertFalse(inventory_names(self.inventory, ["gamma"])[1])

    def test_required_cases_present_without_filtering(self):
        names, complete = inventory_names(self.inventory, ["alpha"])
        self.assertTrue(complete)
        self.assertEqual(names, ["alpha", "beta"])

    def test_assertion_failure_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.replace("beta ... ok", "beta ... FAILED"))[1])

    def test_expected_panic_annotation_matches_inventory(self):
        annotated = self.good.replace("beta ... ok", "beta - should panic ... ok")
        self.assertTrue(counts(self.inventory, annotated)[1])

    def test_expected_panic_failure_still_blocks(self):
        annotated = self.good.replace("beta ... ok", "beta - should panic ... FAILED")
        self.assertFalse(counts(self.inventory, annotated)[1])

    def test_ignored_required_case_blocks(self):
        ignored = self.good.replace("beta ... ok", "beta ... ignored").replace(
            "2 passed; 0 failed; 0 ignored", "1 passed; 0 failed; 1 ignored")
        self.assertFalse(counts(self.inventory, ignored)[1])

    def test_timeout_or_crash_without_summary_blocks(self):
        result, valid = counts(self.inventory, "test alpha ... ok\n")
        self.assertFalse(valid)
        self.assertEqual(result["not_run"], 1)

    def test_missing_and_substituted_case_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.replace("beta", "gamma"))[1])

    def test_duplicate_verdict_blocks(self):
        self.assertFalse(counts(self.inventory, "test alpha ... ok\n" + self.good)[1])

    def test_duplicate_with_panic_annotation_still_blocks(self):
        self.assertFalse(counts(self.inventory, "test alpha - should panic ... ok\n" + self.good)[1])

    def test_substitution_with_panic_annotation_still_blocks(self):
        annotated = self.good.replace("beta ... ok", "gamma - should panic ... ok")
        self.assertFalse(counts(self.inventory, annotated)[1])

    def test_missing_summary_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.split("test result:")[0])[1])

    def test_filtered_result_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.replace("0 filtered", "1 filtered"))[1])


class SourceIdentityControls(unittest.TestCase):
    head = "d81321bd899a766be1ffb537311c3a72996b43f3"

    def test_matching_head(self):
        self.assertTrue(source_identity_matches(self.head, self.head))

    def test_missing_expected_head_blocks(self):
        self.assertFalse(source_identity_matches(None, self.head))
        self.assertFalse(source_identity_matches("", self.head))

    def test_mismatched_checkout_blocks(self):
        self.assertFalse(source_identity_matches(self.head, "a" * 40))

    def test_non_commit_identity_blocks(self):
        self.assertFalse(source_identity_matches("main", "main"))


class ExactCaseControls(unittest.TestCase):
    inventory = "alpha: test\n\n1 test, 0 benchmarks\n"
    good = ("test alpha ... ok\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out\n")

    def test_exact_one_case_inventory(self):
        self.assertTrue(exact_inventory_matches(self.inventory, "alpha"))

    def test_zero_or_missing_case_blocks(self):
        self.assertFalse(exact_inventory_matches("0 tests, 0 benchmarks\n", "alpha"))
        self.assertFalse(exact_inventory_matches(self.inventory, "beta"))
        self.assertFalse(exact_inventory_matches(self.inventory, ""))

    def test_unexpected_or_duplicate_cases_block(self):
        extra = "alpha: test\nbeta: test\n\n2 tests, 0 benchmarks\n"
        duplicate = "alpha: test\nalpha: test\n\n2 tests, 0 benchmarks\n"
        self.assertFalse(exact_inventory_matches(extra, "alpha"))
        self.assertFalse(exact_inventory_matches(duplicate, "alpha"))

    def test_exact_receipt_checks_known_filtered_denominator(self):
        result, valid = counts(self.inventory, self.good, expected_filtered=42)
        self.assertTrue(valid)
        self.assertEqual(result["filtered_out"], 42)

    def test_full_target_still_rejects_filtered_results(self):
        self.assertFalse(counts(self.inventory, self.good)[1])

    def test_wrong_filtered_denominator_blocks(self):
        self.assertFalse(counts(self.inventory, self.good, expected_filtered=41)[1])

    def test_filtered_failure_or_ignore_or_incomplete_blocks(self):
        for output in [self.good.replace("alpha ... ok", "alpha ... FAILED"),
                       self.good.replace("alpha ... ok", "alpha ... ignored"),
                       "test alpha ... ok\n"]:
            self.assertFalse(counts(self.inventory, output, expected_filtered=42)[1])

    def test_filtered_duplicate_or_substitution_blocks(self):
        self.assertFalse(counts(self.inventory, "test alpha ... ok\n" + self.good, 42)[1])
        self.assertFalse(counts(self.inventory, self.good.replace("alpha", "beta"), 42)[1])


if __name__ == "__main__":
    unittest.main()
