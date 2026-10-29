"""Static receipt custody controls. No fixture program or reference tool runs."""
import hashlib
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

import q0_16_artifact as custody


class ArtifactControls(unittest.TestCase):
    def test_loop_and_broken_links_are_records_not_traversals(self):
        with tempfile.TemporaryDirectory() as text:
            base = Path(text)
            root = base / "receipts"
            root.mkdir()
            (root / "loop.py").symlink_to("loop.py")
            (root / "broken.py").symlink_to("missing.py")
            raw = b"line\r\n\xff\x00"
            (root / ".raw").write_bytes(raw)
            (root / "back\\slash.py").write_bytes(raw)
            result = custody.pack(root, base / "safe.tar", base / "manifest.json", "pylint")
            self.assertFalse(result["symlinks_followed"])
            with tarfile.open(base / "safe.tar") as archive:
                self.assertTrue(archive.getmember("receipts/loop.py").issym())
                self.assertEqual(archive.getmember("receipts/loop.py").linkname, "loop.py")
                self.assertEqual(archive.getmember("receipts/broken.py").linkname, "missing.py")
                self.assertEqual(archive.extractfile("receipts/.raw").read(), raw)
                self.assertEqual(archive.extractfile("receipts/back\\slash.py").read(), raw)
            self.assertEqual((root / ".raw").read_bytes(), raw)
            self.assertEqual(result["archive"]["sha256"], hashlib.sha256((base / "safe.tar").read_bytes()).hexdigest())
            self.assertEqual(json.loads((base / "manifest.json").read_bytes()), result)

    def test_only_declared_venv_roots_are_omitted(self):
        with tempfile.TemporaryDirectory() as text:
            base = Path(text)
            root = base / "receipts"
            request = "a" * 64
            omitted = ["run/environments/mypy/venv", "run/requests/" + request + "/metadata/venv"]
            retained = ["run/environments/mypy/identity.json", "run/requests/" + request + "/metadata/input-snapshot/pkg.py"]
            for name in [p + "/payload" for p in omitted] + retained:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"retained data")
            result = custody.pack(root, base / "safe.tar", base / "manifest.json", "mypy")
            self.assertEqual(result["excluded_venv_roots"], sorted(omitted))
            names = {r["path"] for r in result["entries"]}
            self.assertTrue(set(retained) <= names)
            self.assertFalse(any(name + "/payload" in names for name in omitted))

    def test_outputs_cannot_enter_input_or_replace_evidence(self):
        with tempfile.TemporaryDirectory() as text:
            base = Path(text)
            root = base / "receipts"
            root.mkdir()
            for output, manifest in [(root / "inside.tar", base / "m.json"), (base / "x", base / "x")]:
                with self.assertRaises(ValueError):
                    custody.pack(root, output, manifest, "mypy")


if __name__ == "__main__":
    unittest.main()
