"""Control tests for archive safety and fixed-input identity checks."""

import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from q0_ruff_inputs import baseline, expanded, verify_frozen_inputs


class InputControls(unittest.TestCase):
    def test_archive_rejects_parent_path(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            member = tarfile.TarInfo("../outside.py")
            member.size = 1
            archive.addfile(member, io.BytesIO(b"x"))
        with self.assertRaisesRegex(ValueError, "unsafe"):
            baseline(buffer.getvalue())

    def test_archive_rejects_links(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            member = tarfile.TarInfo("ruff_linter-0.16.10/link")
            member.type = tarfile.SYMTYPE
            member.linkname = "/outside"
            archive.addfile(member)
        with self.assertRaisesRegex(ValueError, "unsafe"):
            baseline(buffer.getvalue())

    def test_zero_case_archive_fails(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz"):
            pass
        with self.assertRaisesRegex(ValueError, "counts differ"):
            baseline(buffer.getvalue())

    def test_expansion_rejects_wrong_denominator(self):
        with self.assertRaisesRegex(ValueError, "denominator"):
            expanded({}, {}, {}, [])

    def test_identity_check_rejects_empty_catalog(self):
        with tempfile.TemporaryDirectory() as name:
            with self.assertRaisesRegex(ValueError, "expected=0 actual=0"):
                verify_frozen_inputs(Path(name), {})

    def test_identity_check_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            catalog = root / "crates/mamba-quality/src/infrastructure/ruff/tests/catalog"
            catalog.mkdir(parents=True)
            rows = [{"path": f"case{n}.py", "source_sha256": "0" * 64} for n in range(784)]
            text = json.dumps(rows)
            (catalog / "part_000.rs").write_text(f'pub const CASES: &str = r#"{text}"#;\n')
            with self.assertRaisesRegex(ValueError, "different="):
                verify_frozen_inputs(root, {r["path"]: b"changed" for r in rows})


if __name__ == "__main__":
    unittest.main()
