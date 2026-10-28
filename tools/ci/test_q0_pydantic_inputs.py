"""Pure source, input, projection and integrity controls for Pydantic assets."""
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tempfile
import unittest
from unittest.mock import patch

import q0_pydantic_inputs as subject

REPOSITORY = Path(__file__).resolve().parents[2]
PROFILE = REPOSITORY / "crates/mamba-quality/assets/profile.json"
LOCK_PATH = REPOSITORY / "tools/ci/q0_pydantic_reference_lock.json"


def fixture_repository(root):
    lock = json.loads(LOCK_PATH.read_bytes())
    for name in [*lock["source_files"], "tools/ci/q0_pydantic_reference_lock.json"]:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPOSITORY / name, target)
    return lock


class FrozenAssets(unittest.TestCase):
    def test_all_four_original_frozen_assets_and_twelve_sections_are_verified(self):
        assets = subject.read_assets(REPOSITORY)
        self.assertEqual(tuple(assets), subject.FAMILIES)
        self.assertEqual(sum(len(asset["targets"]) for asset in assets.values()), 12)
        self.assertEqual(sum(subject.ASSET_PINS[f]["bytes"] for f in assets), 1438281)
        for family, asset in assets.items():
            self.assertEqual(list(asset), ["schema", "mypy", "pydantic", "targets"])
            self.assertEqual(list(asset["targets"]), ["3.12", "3.13", "3.14"])
            raw = (json.dumps(asset, indent=2) + "\n").encode()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), subject.ASSET_PINS[family]["sha256"])
        for value in assets["field-types"]["targets"].values():
            self.assertEqual(len(value["variants"]), 6)

    def test_source_inventory_uses_sealed_path_components_not_raw_string_order(self):
        mapping = json.loads(LOCK_PATH.read_bytes())["source_files"]
        rows = subject._source_rows(dict(reversed(list(mapping.items()))))
        self.assertEqual([r["path"] for r in rows], sorted(mapping, key=PurePosixPath))
        self.assertNotEqual([r["path"] for r in rows], sorted(mapping))
        self.assertEqual(subject.digest(json.dumps(rows, sort_keys=True).encode()), subject.SOURCE_INVENTORY_SHA256)
        source = "crates/mamba-quality/src/infrastructure/mypy/"
        names = [row["path"] for row in rows]
        self.assertLess(names.index(source + "pydantic/model_types.rs"), names.index(source + "pydantic.rs"))

    def test_unsafe_missing_or_changed_source_inventory_is_rejected(self):
        mapping = json.loads(LOCK_PATH.read_bytes())["source_files"]
        missing = dict(mapping); missing.pop(next(iter(missing)))
        changed = dict(mapping); changed[next(iter(changed))] = "0" * 64
        unsafe = dict(mapping); unsafe["../outside"] = unsafe.pop(next(iter(unsafe)))
        for candidate in [missing, changed, unsafe]:
            with self.subTest(candidate=candidate), self.assertRaises(ValueError):
                subject._source_rows(candidate)

    def test_changed_asset_profile_or_owning_source_is_rejected(self):
        names = [subject.ASSET_PINS["field-types"]["path"],
                 "crates/mamba-quality/assets/profile.json",
                 "crates/mamba-quality/src/infrastructure/mypy/tests/pydantic_config.rs"]
        for name in names:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); fixture_repository(root)
                changed = root / name
                changed.write_bytes(changed.read_bytes() + b"\n")
                with self.assertRaisesRegex(ValueError, "source bytes differ"):
                    subject.read_assets(root)

    def test_changed_lock_target_family_config_and_stub_pins_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); original = fixture_repository(root)
            lockpath = root / "tools/ci/q0_pydantic_reference_lock.json"
            changed_stubs = copy.deepcopy(original["stub_pins"])
            changed_stubs["field-types"]["filename"] = "other.pyi"
            for key, value in [("python_versions", ["3.12.11", "3.13.9", "3.14.0"]),
                               ("families", list(reversed(subject.FAMILIES))),
                               ("asset_target_count", 0), ("reference_timeout_seconds", 121),
                               ("config_sha256", "wrong"), ("stub_pins", changed_stubs)]:
                lockpath.write_text(json.dumps({**original, key: value}))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    subject.read_assets(root)


class FixedInputs(unittest.TestCase):
    def test_exact_original_stub_and_config_bytes(self):
        pins = [("field-types", 36, "018ead575559be46dc008f42f57060a3c6d986f3b832d891bc9b701907676aef"),
                ("model-types", 86, "c3b69f68843dbad4b0a7566d02c578efb1aed3b681035141ed6d9fe410f0e9e4"),
                ("model-methods", 86, "c3b69f68843dbad4b0a7566d02c578efb1aed3b681035141ed6d9fe410f0e9e4"),
                ("model-config-field", 70, "64b4189432f42deab3504305b04705be16a9f9a35a07f2c0dd3555f7d3b7f1e8")]
        for family, size, sha256 in pins:
            spec = subject.input_spec(family)
            self.assertEqual(spec["source_bytes"], size)
            self.assertEqual(spec["source_sha256"], sha256)
            self.assertEqual(spec["config_bytes"], 127)
            self.assertEqual(spec["config_sha256"], "82ca0b8a00280cb3546ec816852c81ec1b68a667c863bdff6d8c22119f1d893e")
            self.assertEqual(spec["config_filename"], "mypy.ini")
        self.assertEqual(subject.input_spec("model-config-field")["filename"], "config_field.pyi")

    def test_first_three_materialize_only_the_two_exact_files(self):
        with tempfile.TemporaryDirectory() as directory:
            for family in subject.FAMILIES[:3]:
                work = Path(directory) / family
                proof = subject.materialize_work(family, work, PROFILE)
                spec = subject.input_spec(family)
                self.assertEqual({p.name for p in work.iterdir()}, {spec["filename"], "mypy.ini"})
                self.assertEqual((work / spec["filename"]).read_bytes(), spec["source"].encode())
                self.assertEqual((work / "mypy.ini").read_bytes(), spec["config"].encode())
                self.assertEqual(proof["files"][spec["filename"]], {"bytes": spec["source_bytes"], "sha256": spec["source_sha256"]})
                self.assertTrue(proof["work_created_by_helper"])
                self.assertFalse(proof["original_child_creates_work"])

    def test_config_field_leaves_work_and_missing_parent_uncreated(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory) / "absent-parent" / "work"
            proof = subject.materialize_work("model-config-field", work, PROFILE)
            self.assertFalse(work.exists())
            self.assertFalse(work.parent.exists())
            self.assertFalse(proof["work_created_by_helper"])
            self.assertTrue(proof["original_child_creates_work"])
            self.assertEqual(set(proof["files"]), {"config_field.pyi", "mypy.ini"})

    def test_existing_work_and_bad_profile_do_not_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory) / "existing"; work.mkdir()
            preserved = work / "keep"; preserved.write_bytes(b"keep")
            for family in subject.FAMILIES:
                with self.subTest(family=family), self.assertRaises(FileExistsError):
                    subject.materialize_work(family, work, PROFILE)
            self.assertEqual(preserved.read_bytes(), b"keep")
            profile = Path(directory) / "profile.json"; profile.write_bytes(b"{}")
            absent = Path(directory) / "not-created"
            with self.assertRaisesRegex(ValueError, "profile differs"):
                subject.materialize_work("field-types", absent, profile)
            self.assertFalse(absent.exists())

    def test_unknown_family_and_relative_work_are_rejected(self):
        with self.assertRaises(ValueError): subject.input_spec("runtime-validation")
        with self.assertRaises(ValueError): subject.materialize_work("field-types", Path("relative"), PROFILE)

    def test_input_construction_never_reads_expected_assets(self):
        with patch.object(subject, "read_assets", side_effect=AssertionError("expected data was read")):
            spec = subject.input_spec("field-types")
            with tempfile.TemporaryDirectory() as directory:
                proof = subject.materialize_work("field-types", Path(directory) / "work", PROFILE)
        self.assertEqual(spec["source"], "from pydantic import Field as Field\n")
        self.assertFalse(proof["expected_values_used_for_inputs"])


class Projection(unittest.TestCase):
    def test_field_projection_preserves_all_six_overloads_and_exact_fields(self):
        raw = {"serialized_type": {".class": "Overloaded"}, "aliases": {"alias": {"id": 9}},
               "variants": ["overload " + str(i) for i in range(6)], "fields_sha256": "source", "identity": "not projected"}
        projected = subject.project_record("field-types", raw)
        self.assertEqual(list(projected), ["serialized_type", "aliases", "variants", "fields_sha256"])
        self.assertEqual(projected["variants"], raw["variants"])
        self.assertEqual(projected["aliases"], raw["aliases"])
        self.assertNotIn("identity", projected)

    def test_model_projections_drop_only_original_unprojected_stdout_fields(self):
        raw = {"model_types": {"value": [1, 2]}, "aliases": {"x": "y"}, "variants": ["still raw"], "python": "3.12"}
        for family in ["model-types", "model-methods"]:
            self.assertEqual(subject.project_record(family, raw), {"model_types": raw["model_types"], "aliases": raw["aliases"]})
        with self.assertRaises(KeyError): subject.project_record("model-types", {"model_types": {}})

    def test_config_field_uses_whole_sidecar_and_does_not_replace_it_with_stdout(self):
        field = {"type": {".class": "TypeAliasType"}, "class_var": True, "final": False, "unexpected": "kept"}
        self.assertEqual(subject.project_record("model-config-field", {"class_var": False}, field=field), field)
        with self.assertRaises(ValueError): subject.project_record("model-config-field", {"type": "not authoritative"})

    def test_all_original_envelopes_and_bytes_reassemble_without_normalization(self):
        assets = subject.read_assets(REPOSITORY)
        for family, asset in assets.items():
            sections = dict(reversed(list(asset["targets"].items())))
            assembled = subject.assemble_asset(family, sections)
            self.assertEqual(list(assembled["targets"]), ["3.12", "3.13", "3.14"])
            self.assertEqual(assembled, asset)
            self.assertEqual(subject.digest((json.dumps(assembled, indent=2) + "\n").encode()), subject.ASSET_PINS[family]["sha256"])

    def test_missing_extra_or_renamed_target_cannot_assemble(self):
        for sections in [{}, {"3.12": {}, "3.13": {}}, {"3.12": {}, "3.13": {}, "3.14": {}, "3.15": {}}]:
            with self.subTest(sections=sections), self.assertRaises(ValueError): subject.assemble_asset("field-types", sections)

    def test_python_primary_equality_and_scalar_integrity_stay_separate(self):
        for actual, expected in [(True, 1), (1, 1.0), ({"value": True}, {"value": 1})]:
            self.assertTrue(actual == expected)
            self.assertTrue(subject.json_type_errors(actual, expected))
        self.assertFalse(["first", "second"] == ["second", "first"])
        self.assertEqual(subject.json_type_errors(["first", "second"], ["second", "first"]), [])

    def test_object_key_order_is_not_equality_and_shape_changes_are_reported(self):
        self.assertTrue({"a": 1, "b": [True]} == {"b": [True], "a": 1})
        self.assertEqual(subject.json_type_errors({"a": 1, "b": [True]}, {"b": [True], "a": 1}), [])
        for actual, expected in [({"extra": 1}, {}), ([1], [1, 2]), ({"a/b": {"~": None}}, {"a/b": {"~": "text"}}), ({1: "bad"}, {"1": "text"})]:
            self.assertTrue(subject.json_type_errors(actual, expected))
        self.assertIn("/a~1b/~0", subject.json_type_errors({"a/b": {"~": None}}, {"a/b": {"~": "text"}})[0])


if __name__ == "__main__":
    unittest.main()
