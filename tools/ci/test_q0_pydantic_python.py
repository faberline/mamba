"""Pure controls for the Pydantic bootstrap lock and host boundary."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_pydantic_python as subject

REPO = Path(__file__).resolve().parents[2]


def locked():
    return json.loads((REPO / "tools/ci/q0_pydantic_reference_lock.json").read_bytes())


class PydanticBootstrap(unittest.TestCase):
    def test_exact_pydantic_mypy_and_three_patch_lock(self):
        original = locked()
        with patch.object(subject, "require_host_context"):
            for version in subject.VERSIONS:
                self.assertEqual(subject.require_lock(original, version), original["interpreters"][version])
            for key, value in (("version", "2.13.4"), ("mypy", "2.3.0"),
                               ("schema", "other"), ("python_versions", ["3.12.11"]),
                               ("interpreters", {})):
                altered = copy.deepcopy(original)
                altered[key] = value
                with self.subTest(key=key), self.assertRaises(ValueError):
                    subject.require_lock(altered, "3.12.11")
            with self.assertRaises(ValueError):
                subject.require_lock(original, "3.13.9")

    def test_every_interpreter_pin_and_locked_platform_are_checked(self):
        for field, value in (("sha256", "bad"), ("bytes", True),
                             ("url", "https://example.com/python"),
                             ("executable_sha256", "")):
            altered = locked()
            altered["interpreters"]["3.14.0"][field] = value
            with self.subTest(field=field), patch.object(subject, "require_host_context"), \
                    self.assertRaises(ValueError):
                subject.require_lock(altered, "3.12.11")
        altered = locked()
        altered["platform"]["runner_arch"] = "X64"
        with self.assertRaises(ValueError):
            subject.require_lock(altered, "3.12.11")

    def test_non_hosted_call_keeps_failure_and_launches_no_download_or_python(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            argv = ["bootstrap", "--repository", str(REPO), "--output", str(root / "receipts"),
                    "--prefix", str(root / "prefix"), "--python-version", "3.12.11"]
            with patch.object(sys, "argv", argv), patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}), \
                    patch.object(subject, "download") as download, patch.object(subject, "command") as command:
                self.assertEqual(subject.main(), 1)
            download.assert_not_called()
            command.assert_not_called()
            summary = json.loads((root / "receipts/summary.json").read_bytes())
            self.assertFalse(summary["passed"])
            self.assertFalse(summary["reference_tool_executed"])
            self.assertFalse(summary["historical_environment_recovered"])
            self.assertTrue((root / "receipts/artifact-manifest.json").is_file())


if __name__ == "__main__":
    unittest.main()
