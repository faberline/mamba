"""Controls for the original Pylint input identity, files and JSON2 fields."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from q0_pylint_inputs import (INPUT_SHA256, materialize_case, prepare_inputs,
                              project_diagnostics, relative_file)


class Inputs(unittest.TestCase):
    def test_all_public_inputs_reproduce_original_order_and_bytes(self):
        cases, proof = prepare_inputs(Path(__file__).resolve().parents[2])
        self.assertEqual(len(cases), 19747)
        self.assertEqual(proof["input_sha256"], INPUT_SHA256)
        self.assertEqual(proof["input_bytes"], 5911082)
        self.assertEqual(proof["source_file_count"], 20651)
        self.assertEqual(len(proof["published_sources"]), 1314)
        self.assertEqual([c["index"] for c in cases], list(range(19747)))
        self.assertTrue(all("expected" not in c and "reference_exits" not in c for c in cases))

    def test_materialization_keeps_all_non_python_fixture_bytes(self):
        files = {"z.py": "# z\r\n", "a/main.py": "x = 1\n", "lib.so": "\x00\x01",
                 "lib.pyd": "", "lib.dylib": "", "note.txt": "\ufefftext", "stub.pyi": "x: int\n"}
        case = {"files": files, "source_sha256": {n: hashlib.sha256(s.encode()).hexdigest() for n, s in files.items()}}
        with tempfile.TemporaryDirectory() as parent:
            project = Path(parent) / "case_0000"
            result = materialize_case(case, project)
            self.assertEqual(result["python_files"], ["a/main.py", "z.py"])
            self.assertEqual(set(result["files"]), set(files))
            self.assertEqual({str(p.relative_to(project)) for p in project.rglob('*') if p.is_file()}, set(files))
            for name, text in files.items():
                self.assertEqual((project / name).read_bytes(), text.encode())
            with self.assertRaises(FileExistsError):
                materialize_case(case, project)

    def test_source_hash_mismatch_does_not_write_a_false_source(self):
        with tempfile.TemporaryDirectory() as parent:
            project = Path(parent) / "case_0000"
            with self.assertRaises(ValueError):
                materialize_case({"files": {"main.py": "x = 1"}, "source_sha256": {"main.py": "wrong"}}, project)
            self.assertFalse((project / "main.py").exists())

    def test_invalid_or_empty_materialization_is_rejected(self):
        for name in ("", ".", "/main.py", "../main.py", "a/../../main.py"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                relative_file(name)
        with tempfile.TemporaryDirectory() as parent, self.assertRaises(ValueError):
            materialize_case({"files": {}, "source_sha256": {}}, Path(parent) / "case_0000")


class Projection(unittest.TestCase):
    project = Path('/fixed/python312/case_0000')

    def raw(self, **changes):
        message = {"absolutePath": str(self.project / 'main.py'), "messageId": 'E0602',
                   "message": 'exact message at 0x1234', "type": 'error',
                   "line": 2, "column": 0, "endLine": 2, "endColumn": -1}
        message.update(changes)
        return json.dumps({"messages": [message], "statistics": {"score": 0}}).encode()

    def test_keeps_zero_based_locations_severity_message_and_order(self):
        first = json.loads(self.raw())["messages"][0]
        second = {**first, "messageId": 'I0010', "type": 'info', "endColumn": None}
        result = project_diagnostics(json.dumps({"messages": [first, second]}).encode(), self.project)
        self.assertEqual([x["rule"] for x in result], ['E0602', 'I0010'])
        self.assertEqual(result[0]["location"], {"line": 2, "column": 0, "end_line": 2, "end_column": -1})
        self.assertEqual(result[0]["message"], 'exact message at 0x1234')
        self.assertEqual(result[1]["severity"], 'info')
        self.assertIsNone(result[1]["location"]["end_column"])
        self.assertIsNone(result[0]["hint"])

    def test_relative_absolute_path_uses_original_project_prefix_rule(self):
        result = project_diagnostics(self.raw(absolutePath='nested/main.py'), self.project)
        self.assertEqual(result[0]["path"], 'nested/main.py')

    def test_empty_messages_remain_a_valid_empty_diagnostic_list(self):
        self.assertEqual(project_diagnostics(b'{"messages": []}', self.project), [])

    def test_missing_malformed_or_outside_fields_are_not_filtered(self):
        for raw in (b'not json', b'{}', b'{"messages":null}', self.raw(absolutePath='/outside/main.py'), self.raw(absolutePath='../main.py')):
            with self.subTest(raw=raw), self.assertRaises((ValueError, KeyError, TypeError)):
                project_diagnostics(raw, self.project)


if __name__ == '__main__':
    unittest.main()
