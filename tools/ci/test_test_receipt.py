"""Isolated receipt-parser controls; never invokes Cargo or product binaries."""
import unittest

from test_receipt import counts


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

    def test_assertion_failure_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.replace("beta ... ok", "beta ... FAILED"))[1])

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

    def test_missing_summary_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.split("test result:")[0])[1])

    def test_filtered_result_blocks(self):
        self.assertFalse(counts(self.inventory, self.good.replace("0 filtered", "1 filtered"))[1])


if __name__ == "__main__":
    unittest.main()
