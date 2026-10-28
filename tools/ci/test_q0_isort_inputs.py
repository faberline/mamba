"""Fail-closed controls for isort input and evidence inventories."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from q0_isort_inputs import construct, published, verify_checker_results, verify_excluded_api_results, verify_results


class InputControls(unittest.TestCase):
    def test_archive_parent_path_is_rejected(self):
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w:gz") as archive:
            member = tarfile.TarInfo("../outside.py")
            member.size = 1
            archive.addfile(member, io.BytesIO(b"x"))
        data = raw.getvalue()
        with tempfile.TemporaryDirectory() as name:
            repo = Path(name)
            folder = repo / "tools/ci"
            folder.mkdir(parents=True)
            (folder / "q0_isort_reference_lock.json").write_text(json.dumps({"sdist": {"sha256": hashlib.sha256(data).hexdigest()}}))
            with self.assertRaisesRegex(ValueError, "unsafe"):
                construct(data, repo)

    def test_empty_published_inventory_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as name:
            repo = Path(name)
            folder = repo / "crates/mamba-quality/src/infrastructure/isort/tests"
            folder.mkdir(parents=True)
            (folder / "excluded_sources.json").write_text("[]")
            with self.assertRaisesRegex(ValueError, "denominator"):
                published(repo)

    def test_missing_target_rows_are_rejected(self):
        repo = Path(__file__).resolve().parents[2]
        with self.assertRaisesRegex(ValueError, "denominator/order"):
            verify_results(repo, [{"id": "required", "source": "import os"}], {})

    def test_missing_checker_cases_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "denominator"):
            verify_checker_results(Path(__file__).resolve().parents[2], [], {})

    def test_missing_excluded_api_cases_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "denominator"):
            verify_excluded_api_results(Path(__file__).resolve().parents[2], [], {})


if __name__ == "__main__":
    unittest.main()
