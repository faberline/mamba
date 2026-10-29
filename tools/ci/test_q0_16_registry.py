"""Offline controls for the #16 unit inventory. No target program is invoked."""
from __future__ import annotations

from copy import deepcopy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import q0_16_registry as registry


class RegistryControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repository = Path(__file__).resolve().parents[2]
        cls.built = registry.build_registry(cls.repository)

    def changed(self):
        return deepcopy(self.built)

    def assert_rejected(self, value, text):
        with self.assertRaisesRegex(ValueError, text):
            registry.validate_registry(value)

    def test_saved_inventory_rebuilds_from_public_sources(self):
        saved = json.loads((self.repository / registry.OUTPUT).read_text())
        self.assertEqual(saved, self.built)

    def test_each_tier_keeps_its_own_nonzero_unit_denominators(self):
        counts = self.built["tiers"]
        self.assertEqual(counts["A"]["counts"]["profile_declarations"], 587)
        self.assertEqual(counts["B"]["counts"]["legacy_original_record_entries"], 7205)
        self.assertEqual(counts["C"]["counts"]["obligation_records"], 412)
        self.assertEqual(counts["D"]["counts"]["whole_metadata_target_sections"], 12)
        self.assertNotIn("passed_case_count", self.built)
        self.assertTrue(all(value is False for value in self.built["acceptance"].values()))

    def test_zero_denominator_cannot_be_accepted(self):
        value = self.changed()
        value["tiers"]["B"]["counts"]["manifest_entries"] = 0
        self.assert_rejected(value, "denominator differs")

    def test_missing_manifest_binding_is_rejected(self):
        value = self.changed()
        value["tiers"]["B"]["manifests"].pop()
        self.assert_rejected(value, "missing manifest")

    def test_orphan_record_is_rejected(self):
        value = self.changed()
        value["tiers"]["B"]["original_record_entries"][0]["manifest_id"] = "lost"
        self.assert_rejected(value, "orphan original record")

    def test_missing_source_binding_is_rejected(self):
        value = self.changed()
        value["tiers"]["B"]["original_record_entries"][0]["record_pointer"]["source_id"] = "missing.json"
        self.assert_rejected(value, "missing source binding")

    def test_changed_source_identity_is_rejected(self):
        value = self.changed()
        value["tiers"]["A"]["units"][0]["declaration"]["sha256"] = "0" * 64
        self.assert_rejected(value, "source binding hash differs")

    def test_unknown_role_cannot_become_pass(self):
        value = self.changed()
        value["tiers"]["A"]["units"][0]["target_roles"][0]["roles"]["hit"]["status"] = "pass"
        self.assert_rejected(value, "unknown role inflated")

    def test_unrun_record_cannot_become_native_pass(self):
        value = self.changed()
        value["tiers"]["B"]["original_record_entries"][0]["native_product_status"] = "passed"
        self.assert_rejected(value, "native gate inflated")

    def test_unrun_record_cannot_fill_profile_role(self):
        value = self.changed()
        value["tiers"]["B"]["original_record_entries"][0]["accepted_role_witness"] = True
        self.assert_rejected(value, "unknown/unrun role inflated")

    def test_legacy_character_entries_keep_two_whole_source_bindings(self):
        tier = self.built["tiers"]["B"]
        chars = [r for r in tier["original_record_entries"] if r["unit_kind"] == "legacy_source_character_entry"]
        self.assertEqual(len(chars), 353)
        self.assertEqual(sorted(r["characters"] for r in tier["whole_scalar_sources"]), [92, 261])
        self.assertTrue(all(r["record_pointer"]["json_pointer"] == "/source" for r in chars))
        self.assertTrue(all(r["binding"]["sections"] == [] for r in chars))
        self.assertEqual(sum(len(r["target_sections"]) for r in tier["whole_scalar_sources"]), 6)

    def test_character_entry_cannot_be_inflated_to_target_case(self):
        value = self.changed()
        row = next(r for r in value["tiers"]["B"]["original_record_entries"] if r["unit_kind"] == "legacy_source_character_entry")
        row["binding"]["sections"] = [{"target": "3.12"}]
        self.assert_rejected(value, "character inflated")

    def test_black_width_slots_do_not_supply_a_python_target_axis(self):
        rows = [r for r in self.built["tiers"]["B"]["original_record_entries"]
                if r["record_pointer"]["source_id"].endswith("/width_data.rs")]
        self.assertEqual(len(rows), 21)
        self.assertTrue(all(r["binding"]["kind"] == "target_provenance_unknown" for r in rows))
        self.assertTrue(all(len(r["binding"]["stored_reference_value_slots"]) == 3 for r in rows))
        self.assertTrue(all(r["binding"]["sections"] == [] for r in rows))

    def test_catalog_selectors_join_existing_sources_instead_of_new_cases(self):
        rows = [r for r in self.built["tiers"]["B"]["original_record_entries"] if r["unit_kind"] == "existing_catalog_selector"]
        self.assertEqual(len(rows), 2990)
        self.assertTrue(all(r["selected_case_pointer"]["constant"] == "CASES" for r in rows))
        self.assertTrue(all(len(r["binding"]["sections"]) == 3 for r in rows))
        self.assertTrue(all(r["binding"]["inherited_target_axis_id"] == registry.SELECTOR_AXIS_ID for r in rows))
        self.assertTrue(all([section["target_index"] for section in r["binding"]["sections"]] == [0, 1, 2] for r in rows))
        relation = self.built["tiers"]["B"]["inherited_target_axes"][0]
        self.assertEqual(relation["caller_source"]["source_id"], registry.SELECTOR_CALLER)
        self.assertEqual(relation["wrapper_source"]["source_id"], registry.SELECTOR_WRAPPER)
        self.assertEqual(relation["target_axis"], list(registry.TARGETS))
        self.assertIn(registry.SELECTOR_CALLER, {value["source_id"] for value in self.built["sources"]})

    def assert_selector_cache_mutation_rejected(self, old, new, reason):
        # Only one private in-memory source cache is changed. All original
        # catalog bytes, including the three expected slots, remain untouched.
        sources = registry.Sources(self.repository)
        caller = sources.read(registry.SELECTOR_CALLER)
        self.assertEqual(caller.count(old), 1)
        changed = caller.replace(old, new, 1)
        sources.data[registry.SELECTOR_CALLER] = changed
        sources.facts[registry.SELECTOR_CALLER].update(sha256=registry.digest(changed), bytes=len(changed))
        selected = next(r for r in self.built["tiers"]["B"]["original_record_entries"] if r["unit_kind"] == "existing_catalog_selector")
        expected_source = self.repository / selected["selected_case_pointer"]["source_id"]
        before = expected_source.read_bytes()
        case_index = int(selected["selected_case_pointer"]["json_pointer"][1:])
        self.assertEqual(len(registry.parse_constant(before.decode())[case_index]["expected"]), 3)
        with patch.object(registry, "Sources", return_value=sources):
            with self.assertRaisesRegex(ValueError, reason):
                registry.build_registry(self.repository)
        self.assertEqual(expected_source.read_bytes(), before)

    def test_selector_build_rejects_swapped_caller_target_order(self):
        self.assert_selector_cache_mutation_rejected(
            b"PythonVersion::Py312,\n            PythonVersion::Py313,",
            b"PythonVersion::Py313,\n            PythonVersion::Py312,",
            "inherited selector target axis/order differs")

    def test_selector_build_rejects_removed_target_forwarding(self):
        self.assert_selector_cache_mutation_rejected(
            b"builtin.record(&case, version, target, &report, &actual);", b"",
            "selector target forwarding differs")

    def test_selector_build_rejects_changed_target_forwarding(self):
        self.assert_selector_cache_mutation_rejected(
            b"builtin.record(&case, version, target, &report, &actual);",
            b"builtin.record(&case, version, 0, &report, &actual);",
            "selector target forwarding differs")

    def test_selector_build_rejects_changed_expected_target_index(self):
        self.assert_selector_cache_mutation_rejected(
            b'let expected = &case["expected"][target];',
            b'let expected = &case["expected"][0];',
            "selector expected target indexing differs")

    def test_missing_selector_inherited_binding_is_rejected(self):
        value = self.changed()
        row = next(r for r in value["tiers"]["B"]["original_record_entries"] if r["unit_kind"] == "existing_catalog_selector")
        row["binding"].pop("inherited_target_axis_id")
        self.assert_rejected(value, "missing selector inherited target binding")

    def test_duplicate_consumer_preserves_unique_source_count(self):
        tier = self.built["tiers"]["B"]
        self.assertEqual(len(tier["manifests"]), 119)
        self.assertEqual(len({r["source"]["source_id"] for r in tier["manifests"]}), 118)
        self.assertEqual(tier["pydantic_config"]["shared_source_consumers"], ["mypy", "pydantic"])

    def test_black_delegated_comparison_has_a_source_bound_axis(self):
        manifests = [r for r in self.built["tiers"]["B"]["manifests"]
                     if r["source"]["source_id"].endswith("/template_grammar.json")]
        self.assertEqual(len(manifests), 1)
        delegated = [r for r in manifests[0]["owning_rust_bindings"] if r["binding_kind"] == "delegated_compare_target_axis"]
        self.assertEqual(len(delegated), 1)
        self.assertEqual(delegated[0]["target_axis"], list(registry.TARGETS))

    def test_missing_construct_binding_is_rejected(self):
        value = self.changed()
        value["tiers"]["B"]["pydantic_construct_selectors"].pop()
        self.assert_rejected(value, "missing construct selector")

    def test_invalid_ancillary_target_is_rejected(self):
        value = self.changed()
        row = next(r for r in value["tiers"]["B"]["original_record_entries"] if r["binding"]["kind"] == "row_target")
        row["binding"]["sections"][0]["target"] = "9.99"
        self.assert_rejected(value, "ancillary row target differs")

    def test_missing_ancillary_target_section_is_rejected(self):
        value = self.changed()
        row = next(r for r in value["tiers"]["B"]["original_record_entries"] if r["binding"]["kind"] == "owner_indexed_expected")
        row["binding"]["sections"] = []
        self.assert_rejected(value, "missing ancillary target section")

    def test_nested_byte_inputs_and_symlink_metadata_are_bound(self):
        rows = self.built["tiers"]["B"]["original_record_entries"]
        for name, count in (("ast_constant_controls.json", 75), ("ast_setter_context.json", 36),
                            ("union_known_members.json", 18), ("union_missing_members.json", 18),
                            ("entry_io_data.json", 27), ("entry_path_errno_data.json", 30),
                            ("duplicate_modules.json", 53), ("shadow_aliases.json", 18)):
            selected = [r for r in rows if r["record_pointer"]["source_id"].endswith("/" + name)]
            self.assertEqual(sum(len(r["input_bindings"]) for r in selected), count, name)
        symlinks = [b for r in rows for b in r.get("input_bindings", []) if b.get("entry_kind") == "symlink"]
        self.assertEqual(len(symlinks), 11)
        self.assertTrue(all(b["symlink_target_is_dereferenced_content"] is False for b in symlinks))

    def test_tuple_oracle_retains_external_input_identity(self):
        rows = [r for r in self.built["tiers"]["B"]["original_record_entries"] if r["record_pointer"]["source_id"].endswith("/tuple_binding_oracles.json")]
        self.assertEqual(len(rows), 3)
        self.assertTrue(all(len(r["input_bindings"]) == 1 for r in rows))
        self.assertTrue(all(r["input_bindings"][0]["pointer"]["source_id"].endswith("/tuple_binding.py") for r in rows))

    def test_missing_input_controls_keep_the_absence_boundary(self):
        rows = [r for r in self.built["tiers"]["B"]["original_record_entries"] if r["record_pointer"]["source_id"].endswith("/entry_inputs_data.json") and not r["input_bindings"]]
        self.assertEqual(len(rows), 18)
        self.assertTrue(all(set(r["input_boundary_bindings"]) >= {"inputs", "files"} for r in rows))

    def test_metadata_subunits_do_not_become_runtime_cases(self):
        sections = self.built["tiers"]["D"]["sections"]
        self.assertEqual([r["metadata_subunits"]["Field_overloads"] for r in sections if "Field_overloads" in r["metadata_subunits"]], [6, 6, 6])
        self.assertEqual([r["metadata_subunits"]["BaseModel_methods"] for r in sections if "BaseModel_methods" in r["metadata_subunits"]], [26, 26, 26])
        self.assertTrue(all(r["runtime_case_count_claimed"] is False for r in sections))

    def test_supplemental_packets_are_authored_intent_only(self):
        rows = self.built["supplemental_author_intents"]
        cases = [r for r in rows if "case_id" in r]
        self.assertEqual(len(cases), 343)
        self.assertTrue(all(r["status"] == "authored_not_run" for r in cases))
        self.assertTrue(all(r["accepted_role_witness"] is False for r in rows))

    def test_source_loader_rejects_changed_bytes_and_path_escape(self):
        with tempfile.TemporaryDirectory(prefix="registry-build-") as directory:
            root = Path(directory)
            (root / "source.json").write_text("[]")
            sources = registry.Sources(root)
            with self.assertRaisesRegex(ValueError, "source hash differs"):
                sources.read("source.json", "0" * 64)
            with self.assertRaisesRegex(ValueError, "repository relative"):
                sources.read("../source.json")

    def test_source_pointer_hash_is_verified(self):
        sources = registry.Sources(self.repository)
        ref = dict(self.built["tiers"]["A"]["units"][0]["declaration"])
        ref["value_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "reference value hash differs"):
            registry.resolve_reference(sources, ref)

    def test_scalar_pointer_cannot_silently_act_as_array_pointer(self):
        with self.assertRaisesRegex(ValueError, "scalar"):
            registry.select({"source": "abc"}, ["source", 1])

    def test_optional_shared_constraint_binding_preserves_separate_units(self):
        with tempfile.TemporaryDirectory(prefix="role-export-") as directory:
            file = Path(directory) / "shared-annotation.json"
            file.write_text(json.dumps([{"unit_id": "shared:line_width", "target": "3.12",
                "role": "hit", "source_id": registry.PROFILE, "json_pointer": "/line_width"}]))
            built = registry.build_registry(self.repository, [file])
            self.assertEqual(len(built["external_author_annotations"]), 1)
            annotation = built["external_author_annotations"][0]
            self.assertEqual(annotation["unit_id"], "shared:line_width")
            self.assertEqual(registry.resolve_reference(registry.Sources(self.repository), annotation["pointer"]), 88)
            self.assertFalse(annotation["accepted_role_witness"])
            self.assertEqual(built["tiers"]["A"]["counts"]["profile_declarations"], 587)
            self.assertEqual(built["tiers"]["A"]["counts"]["shared_constraints_or_identities"], 6)
            self.assertTrue(all(value is False for value in built["acceptance"].values()))

    def test_explicit_author_annotation_does_not_grant_acceptance(self):
        with tempfile.TemporaryDirectory(prefix="registry-build-") as directory:
            file = Path(directory) / "annotations.json"
            file.write_text(json.dumps([{"unit_id": "mypy:strict-flag:--warn-redundant-casts",
                "target": "3.12", "role": "hit", "source_id": registry.PROFILE,
                "json_pointer": "/mypy/strict_flags/7"}]))
            rows = registry.role_annotations(registry.Sources(self.repository), [file], self.built["tiers"]["A"]["units"])
            self.assertEqual(rows[0]["status"], "author_annotation_not_accepted")
            self.assertFalse(rows[0]["accepted_role_witness"])
            self.assertNotIn(directory, json.dumps(rows))


if __name__ == "__main__":
    unittest.main()
