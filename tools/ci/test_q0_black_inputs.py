"""Pure controls for the fixed Black source, input order and byte identities."""
import copy
import io
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import q0_black_inputs as subject

REPOSITORY = Path(__file__).resolve().parents[2]


def archive_with(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as bundle:
        for name, raw, kind in entries:
            member = tarfile.TarInfo(name)
            member.type = kind
            member.size = len(raw) if kind == tarfile.REGTYPE else 0
            if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                member.linkname = "outside"
            bundle.addfile(member, io.BytesIO(raw) if member.isfile() else None)
    return output.getvalue()


def custom_row(extra):
    return {"id": subject.digest(json.dumps(extra, sort_keys=True).encode()), **extra,
            "source_sha256": subject.digest(extra["source"].encode()),
            "origins": [{"path": subject.EXTRA_ORIGIN, "case": extra["name"]}],
            "expected": [{"this_is_not_input": True}]}


class SourceReader(unittest.TestCase):
    def test_full_whitespace_marker_is_removed(self):
        source, flags = subject.read_source(
            ("x = 1\n" + subject.EMPTY_MARKER + "   \n" + "y = 2\n").encode())
        self.assertEqual(source, "x = 1\n   \ny = 2\n")
        self.assertEqual(flags, "")

    def test_short_marker_remains_literal_source(self):
        self.assertEqual(subject.read_source(b"# EMPTY LINE WITH WHITESPACE\nx = 1\n")[0],
                         "# EMPTY LINE WITH WHITESPACE\nx = 1\n")

    def test_only_initial_flags_are_removed_without_added_separator(self):
        source, flags = subject.read_source(b"# flags: --pyi \n# flags: --preview\nx=1\n# flags: later\n")
        self.assertEqual(flags, "--pyi--preview")
        self.assertEqual(source, "x=1\n# flags: later\n")

    def test_blank_line_stops_leading_flag_collection(self):
        source, flags = subject.read_source(b"\n# flags: --pyi\nx=1\n")
        self.assertEqual(flags, "")
        self.assertEqual(source, "# flags: --pyi\nx=1\n")

    def test_flag_prefix_requires_its_exact_trailing_space(self):
        source, flags = subject.read_source(b"# flags:--pyi\nx=1\n")
        self.assertEqual(flags, "")
        self.assertEqual(source, "# flags:--pyi\nx=1\n")

    def test_exact_output_marker_stops_before_expected_fixture_output(self):
        source, _ = subject.read_source(b"x=1\n# output  \t\nTHIS IS NOT INPUT\n")
        self.assertEqual(source, "x=1\n")
        self.assertIn("# output extra", subject.read_source(b"x=1\n# output extra\ny=2\n")[0])

    def test_crlf_interior_is_preserved_and_final_lf_is_fixed(self):
        self.assertEqual(subject.read_source(b"\r\nx=1\r\ny=2\r\n\r\n")[0], "x=1\r\ny=2\n")

    def test_invalid_utf8_is_not_replaced(self):
        with self.assertRaises(UnicodeDecodeError):
            subject.read_source(b"x = '\xff'\n")


class ArchiveReader(unittest.TestCase):
    def test_sorted_members_extension_flags_and_raw_hash(self):
        raw = archive_with([
            ("pkg/tests/data/cases/z.py", b"# flags: --pyi\nz :int\n", tarfile.REGTYPE),
            ("pkg/elsewhere/ignored.py", b"ignored", tarfile.REGTYPE),
            ("pkg/tests/data/miscellaneous/a.pyi", b"a:int\n# output\nnot input\n", tarfile.REGTYPE),
            ("pkg/tests/data/cases/a.py.txt", b"ignored", tarfile.REGTYPE),
        ])
        cases, files, members = subject.archive_inputs(raw)
        self.assertEqual(len(members), 4)
        self.assertEqual([case["source"] for case in cases], ["z :int\n", "a:int\n"])
        self.assertEqual([case["extension"] for case in cases], ["pyi", "pyi"])
        self.assertEqual(files[0]["sha256"], subject.digest(b"# flags: --pyi\nz :int\n"))
        self.assertEqual(cases[0]["id"], subject.digest((files[0]["path"] + "\0z :int\n").encode()))
        self.assertEqual(list(cases[0]), ["id", "source", "source_sha256", "extension", "origins"])

    def test_unsafe_paths_and_member_types_are_rejected_even_outside_input_scope(self):
        for name, kind in [("../escape", tarfile.REGTYPE), ("/absolute", tarfile.REGTYPE),
                           ("pkg/../escape", tarfile.REGTYPE), ("pkg//escape", tarfile.REGTYPE),
                           ("pkg/./escape", tarfile.REGTYPE), ("pkg\\escape", tarfile.REGTYPE),
                           ("pkg/link", tarfile.SYMTYPE), ("pkg/hard", tarfile.LNKTYPE),
                           ("pkg/fifo", tarfile.FIFOTYPE)]:
            with self.subTest(name=name, kind=kind), self.assertRaises(ValueError):
                subject.archive_inputs(archive_with([(name, b"x", kind)]))

    def test_duplicate_member_names_are_rejected(self):
        with self.assertRaises(ValueError):
            subject.archive_inputs(archive_with([("pkg/a", b"a", tarfile.REGTYPE),
                                                ("pkg/a", b"b", tarfile.REGTYPE)]))

    def test_archive_identity_changes_are_rejected_before_tar_read(self):
        with self.assertRaisesRegex(ValueError, "archive identity"):
            subject.build_inputs(REPOSITORY, b"not the pinned archive")


class CustomReader(unittest.TestCase):
    def test_custom_field_order_and_extra_serialization_are_preserved(self):
        extras = [{"name": "first", "extension": "pyi", "source": "x:int\n"},
                  {"source": "y=1\n", "name": "second"},
                  {"name": "third", "source": "z=1\n", "extension": "py"}]
        cases, raw = subject.custom_inputs([custom_row(extra) for extra in extras])
        self.assertEqual(raw, (json.dumps(extras, indent=2, ensure_ascii=True) + "\n").encode())
        self.assertEqual(list(cases[0]), ["id", "name", "extension", "source", "source_sha256", "origins"])
        self.assertEqual(list(cases[1]), ["id", "source", "name", "source_sha256", "origins"])
        self.assertTrue(all("expected" not in case for case in cases))

    def test_expected_objects_have_no_effect_on_custom_inputs(self):
        row = custom_row({"name": "plain", "source": "x=1\n"})
        cases, raw = subject.custom_inputs([row])
        row["expected"] = object()
        self.assertEqual(subject.custom_inputs([row]), (cases, raw))

    def test_changed_source_or_id_is_rejected(self):
        for key, value in (("source", "changed"), ("source_sha256", "wrong"), ("id", "wrong")):
            row = custom_row({"name": "plain", "source": "x=1\n"})
            row[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                subject.custom_inputs([row])

    def test_extra_fields_or_changed_origin_are_rejected(self):
        row = custom_row({"name": "plain", "source": "x=1\n"})
        for changes in ({"config": "invented"}, {"origins": [{"path": "wrong", "case": "plain"}]}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                subject.custom_inputs([{**row, **changes}])


class FixedPublishedCorpus(unittest.TestCase):
    def test_all_published_rows_have_exact_separate_frozen_content_identity(self):
        rows = subject.read_expected(REPOSITORY)
        self.assertEqual(len(rows), 4172)
        self.assertEqual(len(subject.source_inventory(REPOSITORY)), 132)
        self.assertEqual(sum(any("reference_error" in item for item in row["expected"]) for row in rows), 235)
        self.assertEqual([sum("reference_error" in row["expected"][target] for row in rows)
                          for target in range(3)], [235, 197, 189])

    def test_changed_catalog_source_is_rejected_by_inventory(self):
        names = [row["path"] for row in subject.source_inventory(REPOSITORY)]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in names:
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(REPOSITORY / name, destination)
            changed = root / names[1]
            changed.write_bytes(changed.read_bytes() + b"\n")
            with self.assertRaisesRegex(ValueError, "source inventory"):
                subject._verify_repository(root)

    def test_case_chunk_order_denominator_is_fixed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            header = root / subject.TEST_ROOT / "catalog.rs"
            header.parent.mkdir(parents=True)
            original = (REPOSITORY / subject.TEST_ROOT / "catalog.rs").read_text()
            header.write_text(original.replace("part_000::CASES", "part_001::CASES"))
            with self.assertRaisesRegex(ValueError, "index identity or order"):
                subject.source_inventory(root)

    def test_case_denominators_and_extensions_cannot_shrink(self):
        rows = subject._published(REPOSITORY)
        inputs = [{key: value for key, value in row.items() if key != "expected"} for row in rows]
        subject._fixed_counts(inputs, 260, 3912)
        for cases, upstream, extras in ((inputs[:-1], 260, 3912), (inputs, 259, 3913),
                                        ([{**inputs[0], "extension": "pyi"}, *inputs[1:]], 260, 3912)):
            with self.subTest(upstream=upstream, extra=extras, count=len(cases)), self.assertRaises(ValueError):
                subject._fixed_counts(cases, upstream, extras)

    def test_full_static_archive_reproduces_every_original_input_byte(self):
        path = os.environ.get("Q0_BLACK_TEST_ARCHIVE")
        if not path:
            self.skipTest("Set Q0_BLACK_TEST_ARCHIVE to the statically downloaded pinned sdist")
        archive = Path(path).read_bytes()
        cases, proof = subject.build_inputs(REPOSITORY, archive)
        self.assertEqual(proof["input_bytes"], 2036273)
        self.assertEqual(proof["input_sha256"], subject.INPUT_SHA256)
        self.assertEqual(proof["extra_input_bytes"], 641041)
        self.assertEqual(proof["extra_input_sha256"], subject.EXTRA_SHA256)
        self.assertEqual(proof["archive_member_count"], 495)
        self.assertEqual(len(proof["test_files"]), 260)
        self.assertEqual(proof["extension_counts"], {"py": 4131, "pyi": 41})
        self.assertEqual(len(cases), 4172)
        self.assertTrue(all("expected" not in case for case in cases))
        # Poison every expected object. Input bytes must stay identical.
        rows = subject._published(REPOSITORY)
        changed = copy.deepcopy(rows)
        for row in changed:
            row["expected"] = object()
        with patch.object(subject, "_published", return_value=changed):
            repeated, repeated_proof = subject.build_inputs(REPOSITORY, archive)
        self.assertEqual(repeated, cases)
        self.assertEqual(repeated_proof, proof)


if __name__ == "__main__":
    unittest.main()
