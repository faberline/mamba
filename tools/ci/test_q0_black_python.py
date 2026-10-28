"""Pure Black bootstrap controls. No downloaded interpreter or tool runs."""
import copy
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import q0_black_python as bootstrap


REPOSITORY = Path(__file__).resolve().parents[2]


def fixed_lock():
    return json.loads((REPOSITORY / "tools/ci/q0_black_reference_lock.json").read_bytes())


def make_archive(path, entries):
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


class BlackBootstrapControls(unittest.TestCase):
    def test_fixed_lock_has_all_three_original_patches(self):
        lock = fixed_lock()
        with patch.object(bootstrap, "require_host_context"):
            for version in bootstrap.VERSIONS:
                self.assertEqual(bootstrap.require_lock(lock, version), lock["interpreters"][version])

    def test_unknown_patch_wrong_lock_and_partial_targets_fail(self):
        original = fixed_lock()
        bad = []
        for key, value in (("schema", "other"), ("version", "26.9.0"),
                           ("python_versions", ["3.12.11"]), ("interpreters", {})):
            row = copy.deepcopy(original)
            row[key] = value
            bad.append(row)
        with patch.object(bootstrap, "require_host_context"):
            for lock in bad:
                with self.subTest(lock=lock), self.assertRaises(ValueError):
                    bootstrap.require_lock(lock, "3.12.11")
            with self.assertRaises(ValueError):
                bootstrap.require_lock(original, "3.12.10")

    def test_wrong_interpreter_artifact_fields_fail_before_download(self):
        for field, value in (("version", "3.12.10"), ("url", "https://example.com/python"),
                             ("sha256", "A" * 64), ("executable_sha256", ""),
                             ("filename", "../python.tgz"), ("bytes", True)):
            lock = fixed_lock()
            lock["interpreters"]["3.12.11"][field] = value
            with self.subTest(field=field), patch.object(bootstrap, "require_host_context"), \
                    self.assertRaises(ValueError):
                bootstrap.require_lock(lock, "3.12.11")

    def test_platform_and_non_hosted_context_are_rejected(self):
        lock = fixed_lock()
        lock["platform"]["runner_arch"] = "X64"
        with self.assertRaises(ValueError):
            bootstrap.require_lock(lock, "3.12.11")
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}), self.assertRaises(ValueError):
            bootstrap.require_host_context(bootstrap.HOST_CONTEXT)

    def response(self, raw, host="release-assets.githubusercontent.com"):
        response = io.BytesIO(raw)
        response.geturl = lambda: f"https://{host}/asset?secret=must-not-be-recorded"
        response.status = 200
        return response

    def test_reused_download_checks_digest_and_retains_failed_receipt(self):
        pin = fixed_lock()["interpreters"]["3.12.11"]
        for raw, host in ((b"wrong bytes", "release-assets.githubusercontent.com"),
                          (b"wrong bytes", "example.com")):
            with self.subTest(host=host), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                with patch.object(bootstrap.shared_python, "urlopen", return_value=self.response(raw, host)), \
                        self.assertRaises(ValueError):
                    bootstrap.download(pin, output)
                record = (output / "download.json").read_text()
                self.assertFalse(json.loads(record)["passed"])
                self.assertNotIn("must-not-be-recorded", record)

    def test_reused_extract_rejects_traversal_alias_duplicate_and_external_link(self):
        for entries in ([('python/../escape', 'file', b'x')],
                        [('python/./alias', 'file', b'x')],
                        [('python/x', 'file', b'x'), ('python/x', 'file', b'x')],
                        [('python/x', 'symlink', '/tmp/outside')],
                        [('python/x', 'fifo', '')]):
            with self.subTest(entries=entries), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                make_archive(root / 'synthetic.tar.gz', entries)
                with self.assertRaises((ValueError, tarfile.FilterError)):
                    bootstrap.extract(root / 'synthetic.tar.gz', root / 'fresh')

    def test_identity_requires_exact_patch_path_isolation_and_host(self):
        executable = Path('/fresh/python/bin/python3.12')
        original = {'python': [3, 12, 11], 'isolated': 1, 'executable': str(executable),
                    'machine': 'arm64', 'sys_platform': 'darwin'}
        bootstrap.require_probe(original, '3.12.11', executable)
        for field, value in (('python', [3, 12, 10]), ('isolated', 0), ('isolated', True),
                             ('executable', '/other/python'), ('machine', 'x86_64'),
                             ('sys_platform', 'linux')):
            with self.subTest(field=field), self.assertRaises(ValueError):
                bootstrap.require_probe({**original, field: value}, '3.12.11', executable)

    def test_non_hosted_main_retains_failure_and_starts_nothing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            argv = ['bootstrap', '--repository', str(root), '--output', str(root / 'output'),
                    '--prefix', str(root / 'prefix'), '--python-version', '3.12.11']
            with patch.object(sys, 'argv', argv), patch.dict(os.environ, {'GITHUB_ACTIONS': 'false'}), \
                    patch.object(bootstrap, 'download') as download, patch.object(bootstrap, 'command') as command:
                self.assertEqual(bootstrap.main(), 1)
            download.assert_not_called()
            command.assert_not_called()
            summary = json.loads((root / 'output/summary.json').read_bytes())
            self.assertFalse(summary['passed'])
            self.assertFalse(summary['reference_tool_executed'])
            self.assertTrue((root / 'output/artifact-manifest.json').is_file())

    def test_source_head_must_match_the_actual_run(self):
        with patch.object(bootstrap.subprocess, 'check_output', return_value='a' * 40 + '\n'), \
                patch.dict(os.environ, {'GITHUB_SHA': 'b' * 40}), self.assertRaises(ValueError):
            bootstrap.source_context(REPOSITORY)

    def test_synthetic_main_keeps_archive_payload_probe_and_output_linked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'tools/ci').mkdir(parents=True)
            lock = fixed_lock()
            payload = b'synthetic executable bytes; never executed'
            pin = lock['interpreters']['3.12.11']
            pin['executable_sha256'] = bootstrap.digest(payload)
            (root / 'tools/ci/q0_black_reference_lock.json').write_text(json.dumps(lock))
            output, prefix = root / 'output', root / 'prefix'
            github_output = root / 'github-output'
            argv = ['bootstrap', '--repository', str(root), '--output', str(output),
                    '--prefix', str(prefix), '--python-version', '3.12.11']

            def fake_download(saved_pin, destination):
                path = destination / saved_pin['filename']
                path.write_bytes(b'synthetic archive; shared download has its own controls')
                return path

            def fake_extract(archive, destination):
                executable = destination / 'python/bin/python3.12'
                executable.parent.mkdir(parents=True)
                executable.write_bytes(payload)
                return ([{'name': 'python/bin/python3.12', 'sha256': bootstrap.digest(payload)}],
                        {'python/bin/python3.12': {'type': 'file', 'sha256': bootstrap.digest(payload)}})

            def fake_command(argv, cwd, destination, **options):
                self.assertEqual(argv[1:4], ['-I', '-B', '-c'])
                self.assertEqual(cwd, output)
                self.assertEqual(options['inputs']['executable_sha256'], bootstrap.digest(payload))
                self.assertIn('initial_payload_sha256', options['inputs'])
                destination.mkdir()
                identity = {'python': [3, 12, 11], 'isolated': 1, 'executable': argv[0],
                            'machine': 'arm64', 'sys_platform': 'darwin'}
                (destination / 'stdout.raw').write_text(json.dumps(identity))
                return {'pid': 1, 'waited_pid': 1, 'actual_exit': 0, 'wait_completed': True}

            with patch.object(sys, 'argv', argv), patch.dict(os.environ, {'GITHUB_OUTPUT': str(github_output)}), \
                    patch.object(bootstrap, 'require_host_context'), \
                    patch.object(bootstrap, 'source_context', return_value={'source_sha': 'a' * 40}), \
                    patch.object(bootstrap, 'download', side_effect=fake_download), \
                    patch.object(bootstrap, 'extract', side_effect=fake_extract), \
                    patch.object(bootstrap, 'command', side_effect=fake_command):
                result = bootstrap.main()
            summary = json.loads((output / 'summary.json').read_bytes())
            self.assertEqual(result, 0, summary)
            self.assertTrue(summary['passed'])
            self.assertFalse(summary['historical_environment_recovered'])
            self.assertEqual(summary['source_sha'], 'a' * 40)
            self.assertEqual(github_output.read_text(), f'python-path={prefix}/python/bin/python3.12\n')
            manifest = json.loads((output / 'artifact-manifest.json').read_bytes())
            for name in (pin['filename'], 'archive-members.json', 'installed-payload.json',
                         'identity-probe/stdout.raw', 'summary.json'):
                self.assertIn(name, manifest)


if __name__ == '__main__':
    unittest.main()
