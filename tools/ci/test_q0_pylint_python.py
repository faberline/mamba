"""Pure bootstrap controls. No downloaded interpreter or original tool runs."""
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import q0_pylint_python as bootstrap


def pin(raw=b"archive"):
    release, version = "20250612", "3.12.11"
    filename = f"cpython-{version}+{release}-aarch64-apple-darwin-install_only.tar.gz"
    return {"version": version, "release": release, "flavor": "install_only", "filename": filename,
            "url": f"https://github.com/astral-sh/python-build-standalone/releases/download/{release}/{filename.replace('+', '%2B')}",
            "bytes": len(raw), "sha256": bootstrap.digest(raw), "executable_sha256": "a" * 64}


def archive(path, entries):
    with tarfile.open(path, "w:gz") as stream:
        for name, kind, payload in entries:
            member = tarfile.TarInfo(name)
            member.mode = 0o755
            if kind == "file":
                member.size = len(payload)
                stream.addfile(member, io.BytesIO(payload))
            else:
                member.type = {"symlink": tarfile.SYMTYPE, "fifo": tarfile.FIFOTYPE}[kind]
                member.linkname = payload
                stream.addfile(member)


class BootstrapControls(unittest.TestCase):
    def test_exact_target_artifact_identity_is_required(self):
        bootstrap.require_pin("3.12.11", pin())
        for key, value in [("version", "3.12.10"), ("release", "latest"), ("flavor", "debug"),
                           ("filename", "../bad"), ("url", "https://example.com/python"),
                           ("sha256", "A" * 64), ("executable_sha256", ""), ("bytes", 100_000_001)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                bootstrap.require_pin("3.12.11", {**pin(), key: value})

    def response(self, raw, host="release-assets.githubusercontent.com"):
        response = io.BytesIO(raw)
        response.geturl = lambda: f"https://{host}/asset?secret=must-not-be-recorded"
        response.status = 200
        return response

    def test_download_retains_exact_bytes_and_receipt_without_signed_query(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(bootstrap, "urlopen", return_value=self.response(b"archive")):
                path = bootstrap.download(pin(), root)
            self.assertEqual(path.read_bytes(), b"archive")
            record = (root / "download.json").read_text()
            self.assertTrue(json.loads(record)["passed"])
            self.assertNotIn("must-not-be-recorded", record)

    def test_download_mismatch_and_external_redirect_fail_with_receipts(self):
        for raw, host in [(b"wrong", "release-assets.githubusercontent.com"), (b"archive", "example.com")]:
            with self.subTest(host=host), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with patch.object(bootstrap, "urlopen", return_value=self.response(raw, host)), self.assertRaises(ValueError):
                    bootstrap.download(pin(), root)
                self.assertFalse(json.loads((root / "download.json").read_bytes())["passed"])

    def test_extract_records_payload_bytes_links_and_modes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive(root / "input.tgz", [("python/bin/python3.12", "file", b"synthetic executable"),
                                          ("python/bin/python3", "symlink", "python3.12")])
            members, installed = bootstrap.extract(root / "input.tgz", root / "fresh")
            self.assertEqual(len(members), 2)
            self.assertEqual(installed["python/bin/python3.12"]["sha256"], bootstrap.digest(b"synthetic executable"))
            self.assertEqual(installed["python/bin/python3"]["target"], "python3.12")
            self.assertTrue(all("mode" in row for row in installed.values()))

    def test_unsafe_duplicate_alias_special_and_external_link_members_fail(self):
        for entries in [[("/python/x", "file", b"x")], [("python/../x", "file", b"x")],
                        [("python/./x", "file", b"x")], [("other/x", "file", b"x")],
                        [("python/x", "file", b"x"), ("python/x", "file", b"x")],
                        [("python/x", "fifo", "")], [("python/x", "symlink", "/tmp/outside")]]:
            with self.subTest(entries=entries), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive(root / "input.tgz", entries)
                with self.assertRaises((ValueError, tarfile.FilterError)):
                    bootstrap.extract(root / "input.tgz", root / "fresh")

    def test_non_hosted_main_records_failure_before_download_or_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            argv = ["bootstrap", "--repository", str(root), "--output", str(root / "output"),
                    "--prefix", str(root / "prefix"), "--python-version", "3.12.11"]
            with patch.object(sys, "argv", argv), patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}), \
                    patch.object(bootstrap, "download") as download, patch.object(bootstrap, "command") as command:
                self.assertEqual(bootstrap.main(), 1)
            download.assert_not_called()
            command.assert_not_called()
            self.assertFalse(json.loads((root / "output/summary.json").read_bytes())["passed"])
            self.assertTrue((root / "output/artifact-manifest.json").is_file())


if __name__ == "__main__":
    unittest.main()
