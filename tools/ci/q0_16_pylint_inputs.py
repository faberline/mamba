"""Build independent Q0-16 Pylint input controls without running Pylint.

The fixed corpus and its expectations are read only.  Observation predicates
express the new controls' intent; they are not full expected tool outputs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

TARGETS = ["3.12", "3.13", "3.14"]
PROFILE_SHA256 = "59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7"
NEGATIVE_GAPS = set("""
C0103 C0112 C0114 C0115 C0116 C0201 C0202 C0203 C0204 C0209 C0301 C0303
C0304 C0305 C0327 C0328 C0401 C0402 C0403 C0410 C0413 C0414 C0415 C3001
C3002 E0011 E0013 E0014 E0015 E0103 E0106 E0108 E0114 E0202 E0211 E0213
E0239 E0302 E0305 E0306 E0308 E0309 E0310 E0311 E0312 E0401 E0402 E0601
E0606 E0611 E0643 E0704 E0711 E1102 E1111 E1121 E1124 E1125 E1128 E1131
E1132 E1133 E1134 E1135 E1200 E1201 E1205 E1301 E1302 E1303 E1304 E1305
E1519 E1520 E1700 E1904 E2501 E2511 E2514 E3102 F0001 F0002 F0010 F0011
F0202 R0022 R0205 R0206 R0402 R0903 R1704 R1710 R1711 R1712 R1721 R1722
R1732 R1734 R1735 W0101 W0104 W0105 W0107 W0109 W0120 W0125 W0127 W0128
W0129 W0133 W0134 W0136 W0137 W0150 W0199 W0211 W0236 W0301 W0311 W0401
W0404 W0406 W0407 W0410 W0601 W0602 W0603 W0604 W0614 W0621 W0622 W0631
W0640 W0641 W0702 W0706 W0707 W0716 W0718 W1113 W1117 W1300 W1301 W1302
W1304 W1305 W1405 W1502 W1503 W1514 W1518 W2301 W2601 W2602 W2603
W2604 W2605 W2606 W2607 W4901
""".split())
INTERACTION_GAPS = set("""
C0114 C0302 C0328 C0401 C0402 C0403 C2503 E0001 E0013 E0014 E0015 E0106
E0108 E0402 E1131 E1200 E1201 E1205 E1901 E2501 E2514 F0001 F0002 F0010
F0011 F0202 R0401 R0402 R0904 W0133 W0406 W0407 W0601 W0604 W1402 W1405
W1502 W2402 W2601 W2602 W2603 W2604 W2605 W2606 W2607
""".split())
HIT_GAPS = set("""
C0328 C0401 C0402 C0403 E0013 E0014 E0015 E0106 E1131 E2501 E2514 F0001
F0011 F0202 W0407 W1405 W1502 W2601 W2602 W2603 W2604 W2605 W2606 W2607
""".split())
FAILED_FIXED_CASES = set("""
nesting2_multifile_0_1_6 nesting2_multifile_1_0_6 nesting2_multifile_1_1_6
signature1_project_0 signature1_project_1 signature1_project_2
similarity1_padded_indentation strings_format_27_00 strings_format_27_01
strings_format_27_02 strings_format_27_03 strings_percent_22_08
""".split())
PROBE_NAME = "q0_16_boundary_probe.py"
PROBE_SOURCE = '"""Independent project-boundary lint probe."""\nprint(q0_16_missing_probe_value)\n'
RAW_LITERAL = re.compile(r'pub const CASES: &str = r(?P<h>#+)"(?P<body>.*)"(?P=h);', re.S)
REENABLE = re.compile(r"pylint\s*:\s*enable", re.I)


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def safe_relative(value: str) -> bool:
    path = PurePosixPath(value)
    return (bool(value) and not path.is_absolute() and "\\" not in value
            and all(part not in ("", ".", "..") for part in value.split("/"))
            and path.as_posix() == value)


def catalog(repository: Path) -> tuple[list[dict], list[dict]]:
    rows, inventory = [], []
    root = repository / "crates/mamba-quality/src/infrastructure/pylint/tests/catalog"
    for path in sorted(root.rglob("part_*.rs")):
        raw = path.read_bytes()
        match = RAW_LITERAL.search(raw.decode())
        if match is None:
            raise ValueError("invalid fixed Pylint catalog container")
        chunk = json.loads(match["body"])
        inventory.append({"path": path.relative_to(repository).as_posix(),
                          "bytes": len(raw), "sha256": digest(raw)})
        for index, row in enumerate(chunk):
            if len(row["expected"]) != 3 or len(row["reference_exits"]) != 3:
                raise ValueError("fixed Pylint target count differs")
            if row["files"].keys() != row["source_sha256"].keys():
                raise ValueError("fixed Pylint source inventory differs")
            for name, text in row["files"].items():
                if not safe_relative(name) or digest(text.encode()) != row["source_sha256"][name]:
                    raise ValueError("fixed source path or bytes differ")
            rows.append({"row": row, "path": path.relative_to(repository).as_posix(),
                         "container_sha256": digest(raw), "index": index})
    if len(rows) != 19747 or len({item["row"]["name"] for item in rows}) != 19747:
        raise ValueError("fixed Pylint case denominator differs")
    return rows, inventory


def stable_focus_files(row: dict, code: str) -> set[str]:
    by_target = [{item["path"] for item in expected if item["rule"] == code}
                 for expected in row["expected"]]
    return set.intersection(*by_target) & row["files"].keys()


def choose_witness(rows: list[dict], code: str, needs_control: bool) -> tuple[dict, str] | None:
    candidates = []
    for item in rows:
        row = item["row"]
        if row["name"] in FAILED_FIXED_CASES:
            continue
        focus_files = stable_focus_files(row, code)
        if not focus_files:
            continue
        if needs_control and any(REENABLE.search(source) for source in row["files"].values()):
            continue
        if PROBE_NAME in row["files"]:
            continue
        entries = [name for name in row["files"] if name.endswith((".py", ".pyi"))]
        if not entries:
            continue
        score = (sum(len(text.encode()) for text in row["files"].values()),
                 len(row["files"]), row["name"], item["path"], item["index"])
        candidates.append((score, item, sorted(focus_files)[0]))
    if not candidates:
        return None
    _, item, focus_file = min(candidates, key=lambda value: value[0])
    return item, focus_file


def source_provenance(item: dict, code: str, focus_file: str) -> dict:
    row = item["row"]
    pointers = {}
    for target_index, target in enumerate(TARGETS):
        pointers[target] = [f'/{item["index"]}/expected/{target_index}/{index}'
                            for index, diagnostic in enumerate(row["expected"][target_index])
                            if diagnostic["rule"] == code and diagnostic["path"] == focus_file]
    return {"kind": "derived-public-fixed-catalog-input", "original_case": row["name"],
            "catalog_path": item["path"], "catalog_sha256": item["container_sha256"],
            "catalog_row_pointer": f'/{item["index"]}',
            "original_source_sha256": row["source_sha256"],
            "original_focus_expectation_pointers": pointers,
            "source_derivation": "Original public source files copied byte-for-byte. New observations are not copied expectations."}


def guard_description(code: str, guard: dict) -> tuple[str, str]:
    kind = guard["guard"]
    if kind == "version_range":
        return "outside-supported-target-range", "The message definition maximum Python version is below every supported target."
    if code == "E1131":
        return "fixed-callback-guard", "visit_binop only calls the alternative-union detector; its _py310_plus guard returns for every supported target. The arithmetic visitors are named _visit_binop/_visit_augassign."
    if code == "F0202":
        return "fixed-callback-guard", "The sole _check_signature caller is visit_functiondef and it rejects a non-FunctionDef parent before calling. _check_signature emits F0202 only for a non-FunctionDef argument. Standard callbacks and no plugins are the scope."
    if kind == "fixed_default_options":
        return "fixed-default-option-guard", "The fixed default option disables this emission path. This is not a statement that the code can never be emitted under another configuration."
    if kind == "fixed_configuration_entry":
        return "fixed-valid-configuration-guard", "The fixed valid configuration has no invalid plugin, unknown section/option, or parse-error entry. This is not an unsupported-code claim."
    if kind == "supported_target_floor":
        return "supported-target-floor-guard", "The feature-version check is false for the supported Python target floor."
    if kind == "parser_entry_guards":
        return "parser-entry-preemption", "The ordinary full-tool parser rejects the trigger before the later checker callback. A fresh parser-boundary absence probe is included."
    raise ValueError("unclassified guard: " + code)


def guard_source_refs(repository: Path, target: dict, code: str) -> list[dict]:
    guard = target["rules"][code]["guard"]
    if guard == "version_range":
        paths = ["pylint/message/message_definition.py"]
    elif code == "C0328":
        paths = ["pylint/checkers/format.py"]
    elif code in {"C0401", "C0402", "C0403"}:
        paths = ["pylint/checkers/spelling.py"]
    elif code == "W0407":
        paths = ["pylint/checkers/imports.py"]
    elif code == "W1405":
        paths = ["pylint/checkers/strings.py"]
    elif code == "E1131":
        paths = ["pylint/checkers/typecheck.py"]
    elif code == "F0202":
        paths = ["pylint/checkers/classes/class_checker.py"]
    elif guard == "supported_target_floor":
        paths = ["pylint/checkers/unsupported_version.py"]
    elif guard == "parser_entry_guards":
        paths = ["pylint/lint/pylinter.py"]
        if code == "W2607":
            paths.append("pylint/checkers/unsupported_version.py")
    else:
        paths = []
    refs = []
    for path in paths:
        source = target["source_guards"][path]
        refs.append({"upstream_path": path, "sha256": source["sha256"],
                     "guard_file_pointer": "/0/source_guards/" + path.replace("~", "~0").replace("/", "~1"),
                     "methods": [{"name": name, "line": method["line"], "end_line": method["end_line"]}
                                 for name, methods in source["methods"].items() for method in methods]})
    if guard == "fixed_configuration_entry":
        path = repository / "tools/ci/q0_pylint_reference.py"
        refs.append({"repository_path": path.relative_to(repository).as_posix(),
                     "sha256": digest(path.read_bytes()), "scope": "reference_argv fixed valid configuration"})
    return refs


GUARD_INPUTS = {
    "C0328": '"""LF input with no configured expected line-ending format."""\nVALUE = 1\n',
    "C0401": '# Thiss iz wrnggg.\nVALUE = 1\n',
    "C0402": '"""Thiss iz wrnggg."""\nVALUE = 1\n',
    "C0403": '"""Thiss iz wrnggg."""\nVALUE = 1\n',
    "W0407": '"""No preferred-module mapping is configured."""\nimport sys\nprint(sys.version)\n',
    "W1405": '"""Quote consistency is disabled."""\nFIRST = \'one\'\nSECOND = "two"\n',
    "E0013": '"""No plugin is configured."""\nVALUE = 1\n',
    "E0014": '"""The runner uses its fixed valid configuration."""\nVALUE = 1\n',
    "E0015": '"""The runner uses only known options."""\nVALUE = 1\n',
    "F0011": '"""The runner uses its fixed valid configuration."""\nVALUE = 1\n',
    "E0106": '"""Generator returns are supported by the target floor."""\ndef values():\n    yield 1\n    return 2\n',
    "W1502": '"""Datetime truth values use the supported target floor."""\nimport datetime\nVALUE = bool(datetime.datetime(2020, 1, 1))\n',
    "E1131": '"""Alternative union syntax is supported by the target floor."""\nVALUE = int | str\n',
    "F0202": '"""Both signature callback arguments are function definitions."""\nclass Parent:\n    def value(self):\n        return 1\nclass Child(Parent):\n    def value(self):\n        return 2\n',
    "W2601": '"""Formatted strings are supported."""\nNAME = "value"\nVALUE = f"{NAME}"\n',
    "W2602": '"""Final decorators are supported."""\nfrom typing import final\n@final\nclass FinalValue:\n    pass\n',
    "W2603": '"""Exception groups are supported."""\ntry:\n    raise ExceptionGroup("values", [ValueError()])\nexcept* ValueError:\n    pass\n',
    "W2604": '"""Generic type syntax is supported."""\ntype Value[T] = list[T]\n',
    "W2605": '"""Assignment expressions are supported."""\nif (VALUE := 1):\n    print(VALUE)\n',
    "W2606": '"""Positional-only arguments are supported."""\ndef value(item, /):\n    return item\n',
    "E2501": '# coding: utf-16\nVALUE = 1\n',
    "E2514": '# zero\x00byte\nVALUE = 1\n',
    "W2607": '[*item for item in values]\n',
}


def build(repository: Path) -> dict:
    profile_path = repository / "crates/mamba-quality/assets/profile.json"
    profile_raw = profile_path.read_bytes()
    if digest(profile_raw) != PROFILE_SHA256:
        raise ValueError("fixed profile changed")
    profile = json.loads(profile_raw)
    codes = {row["code"] for row in profile["pylint"]["rules"]}
    if len(codes) != 383 or len(NEGATIVE_GAPS) != 160 or len(INTERACTION_GAPS) != 45 or len(HIT_GAPS) != 24:
        raise ValueError("fixed rule or gap count differs")
    rows, inventory = catalog(repository)
    guard_path = repository / "crates/mamba-quality/assets/pylint-rule-guards.json"
    guard_raw = guard_path.read_bytes()
    guard_targets = json.loads(guard_raw)
    guarded = HIT_GAPS - {"F0001"}
    if set(GUARD_INPUTS) != guarded:
        raise ValueError("guard input denominator differs")
    cases, dispositions, unresolved = [], [], []
    active_needs = (NEGATIVE_GAPS | INTERACTION_GAPS) - HIT_GAPS
    for code in sorted(active_needs):
        chosen = choose_witness(rows, code, code in NEGATIVE_GAPS)
        if chosen is None:
            unresolved.append({"behavior": code, "python_targets": TARGETS,
                               "status": "unresolved-active-witness-selection",
                               "reason": "No all-target hit witness without a Pylint re-enable directive was selected."})
            continue
        item, focus_file = chosen
        row = item["row"]
        group = "q016_pylint_" + code.lower() + "_paired"
        common = {"behavior": code, "group": group, "files": row["files"],
                  "entrypoints": sorted(name for name in row["files"] if name.endswith((".py", ".pyi"))),
                  "python_targets": TARGETS, "provenance": source_provenance(item, code, focus_file)}
        cases.append({**common, "id": group + "_hit", "role": "hit",
                      "requires": [{"rule": code, "presence": "present", "path": focus_file}],
                      "intent": "Fresh default-profile hit baseline for the relevant paired control."})
        if code in NEGATIVE_GAPS:
            cases.append({**common, "id": group + "_disabled", "role": "non_trigger",
                          "disabled_rules": [code], "negative_kind": "cli-rule-suppression",
                          "requires": [{"rule": code, "presence": "absent"}],
                          "intent": "Identical source and entrypoints under the default profile with only the exact focus code disabled. Other diagnostic changes are retained, not assumed invariant."})
        if code in INTERACTION_GAPS:
            if code == "E0602":
                raise ValueError("boundary probe must use a distinct focus rule")
            files = {**row["files"], PROBE_NAME: PROBE_SOURCE}
            cases.append({**common, "id": group + "_project_interaction", "role": "interaction",
                          "files": files, "entrypoints": sorted([*common["entrypoints"], PROBE_NAME]),
                          "interaction_kind": "project-boundary-co-emission",
                          "requires": [{"rule": code, "presence": "present", "path": focus_file},
                                       {"rule": "E0602", "presence": "present", "path": PROBE_NAME}],
                          "intent": "The existing focus project and a distinct valid lint probe share one original-tool invocation. This is project-scope coexistence, not same-file or causal-interference proof."})
    group = "q016_pylint_f0001_missing_entry"
    missing = "q0_16_missing_input.py"
    files = {PROBE_NAME: PROBE_SOURCE}
    provenance = {"kind": "new-input-boundary-control", "source": "Current separate F0001 entry-input contracts; new isolated missing path, not an old environment path."}
    common = {"behavior": "F0001", "group": group, "files": files,
              "entrypoints": [missing], "python_targets": TARGETS,
              "input_kind": "missing-entry", "provenance": provenance}
    cases.extend([
        {**common, "id": group + "_hit", "role": "hit", "missing_entrypoints": [missing],
         "requires": [{"rule": "F0001", "presence": "present"}],
         "intent": "A missing input path reaches the public fatal input boundary."},
        {**common, "id": group + "_present", "role": "non_trigger",
         "files": {**files, missing: '"""The named entrypoint now exists."""\nVALUE = 1\n'},
         "input_kind": "existing-entry", "negative_kind": "entry-presence-control",
         "requires": [{"rule": "F0001", "presence": "absent"}],
         "intent": "The same named entrypoint now exists as valid UTF-8 Python. This probes the missing-input boundary without suppressing the message."},
        {**common, "id": group + "_project_interaction", "role": "interaction",
         "entrypoints": sorted([missing, PROBE_NAME]), "interaction_kind": "project-boundary-co-emission",
         "missing_entrypoints": [missing],
         "requires": [{"rule": "F0001", "presence": "present"},
                      {"rule": "E0602", "presence": "present", "path": PROBE_NAME}],
         "intent": "One invocation receives a valid lint source and a missing input in sorted argument order. Both messages must be observed; this case does not assert processing after a fatal diagnostic."},
    ])
    for code in sorted(guarded):
        reasons = []
        for index, target in enumerate(guard_targets):
            guard = target["rules"][code]
            status, reason = guard_description(code, guard)
            reasons.append({"target": TARGETS[index], "declared_guard": guard,
                            "defaults": target["defaults"], "callback_flags": target["flags"],
                            "guard_pointer": f"/{index}/rules/{code}", "scope_status": status,
                            "reason": reason})
        dispositions.append({"behavior": code, "python_targets": TARGETS, "status": "guarded-in-fixed-scope",
                             "reason": reasons[0]["reason"],
                             "source": {"path": guard_path.relative_to(repository).as_posix(),
                                        "sha256": digest(guard_raw), "target_conditions": reasons,
                                        "upstream_conditions": guard_source_refs(repository, guard_targets[0], code)},
                             "positive_or_interaction_pass_claim": False,
                             "other_configurations_or_internal_API_coverage_claim": False})
        requires = [{"rule": code, "presence": "absent"}]
        if code in {"E2501", "E2514", "W2607"}:
            requires.append({"rule": "E0001", "presence": "present"})
        cases.append({"id": "q016_pylint_" + code.lower() + "_guard_probe", "behavior": code,
                      "role": "non_trigger", "group": "q016_pylint_" + code.lower() + "_guard",
                      "files": {"main.py": GUARD_INPUTS[code]}, "entrypoints": ["main.py"],
                      "python_targets": TARGETS, "requires": requires, "guard_probe": True,
                      "intent": "Probe the declared fixed-scope guard or parser boundary. Absence is not a positive-emission pass.",
                      "provenance": {"kind": "new-fixed-scope-guard-input", "guard_source": dispositions[-1]["source"]}})
    cases.sort(key=lambda case: case["id"])
    packet = {"schema": "mamba.q0.16.supplement-inputs.v1", "tool": "pylint", "cases": cases,
              "dispositions": [*dispositions, *unresolved],
              "metadata": {"fixed_profile_sha256": PROFILE_SHA256, "fixed_enabled_codes": 383,
                           "fixed_main_cases": 19747, "source_catalog_files": len(inventory),
                           "source_inventory_sha256": digest(json.dumps(inventory, sort_keys=True).encode()),
                           "initial_gap_counts": {"hit": 24, "non_trigger": 160, "interaction": 45},
                           "new_cases": len(cases), "new_target_cells": len(cases) * 3,
                           "guarded_codes": sorted(guarded),
                           "unresolved_active_witness_selection_codes": [row["behavior"] for row in unresolved],
                           "disabled_control_mode": "default-profile-minus-exact-focus-rule",
                           "old_strict_failure_cases_preserved": sorted(FAILED_FIXED_CASES),
                           "intended_observations_only": True, "reference_executed": False,
                           "native_executed": False, "acceptance_claim": False}}
    validate(packet, codes)
    return packet


def validate(packet: dict, enabled_codes: set[str] | None = None) -> None:
    ids = set()
    by_group = {}
    for case in packet["cases"]:
        if case["id"] in ids or not re.fullmatch(r"q016_[a-z0-9_]+", case["id"]):
            raise ValueError("duplicate or invalid supplemental ID")
        ids.add(case["id"])
        if case["role"] not in {"hit", "non_trigger", "interaction"} or case["python_targets"] != TARGETS:
            raise ValueError("invalid role or target boundary")
        if enabled_codes is not None and case["behavior"] not in enabled_codes:
            raise ValueError("focus rule is outside the fixed enabled set")
        for name, text in case["files"].items():
            if not safe_relative(name) or not isinstance(text, str):
                raise ValueError("unsafe file layout")
            text.encode()
        if not case["entrypoints"] or any(not safe_relative(name) for name in case["entrypoints"]):
            raise ValueError("unsafe or empty entrypoint list")
        missing = set(case["entrypoints"]) - case["files"].keys()
        if missing != set(case.get("missing_entrypoints", [])):
            raise ValueError("missing entrypoint declaration differs")
        if missing and not any(row["rule"] == "F0001" and row["presence"] == "present" for row in case["requires"]):
            raise ValueError("missing entrypoint requires an explicit fatal observation")
        if not case["requires"] or any(row["presence"] not in {"present", "absent"} for row in case["requires"]):
            raise ValueError("invalid observation intent")
        for requirement in case["requires"]:
            if enabled_codes is not None and requirement["rule"] not in enabled_codes:
                raise ValueError("observation rule is outside the fixed enabled set")
            if "path" in requirement and (not safe_relative(requirement["path"]) or requirement["path"] not in case["files"]):
                raise ValueError("observation source path is not a declared file")
        if case.get("disabled_rules") not in (None, [case["behavior"]]):
            raise ValueError("only exact focus disable is allowed")
        if case.get("disabled_rules") and (case["role"] != "non_trigger" or case.get("negative_kind") != "cli-rule-suppression"):
            raise ValueError("disabled rule is not an explicit negative control")
        by_group.setdefault(case["group"], {})[case["role"]] = case
    for group in by_group.values():
        negative = group.get("non_trigger")
        if negative is not None and negative.get("disabled_rules"):
            hit = group.get("hit")
            if hit is None or any(hit[key] != negative[key] for key in ["files", "entrypoints", "python_targets"]):
                raise ValueError("disabled control lacks byte-identical hit partner")
            if any(REENABLE.search(source) for source in negative["files"].values()):
                raise ValueError("disabled control contains a re-enable directive")


def encoded(packet: dict) -> bytes:
    return (json.dumps(packet, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--check", type=Path)
    args = parser.parse_args()
    packet = build(args.repository)
    raw = encoded(packet)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("xb") as stream:
            stream.write(raw)
    elif args.check.read_bytes() != raw:
        raise ValueError("supplemental packet differs from deterministic source derivation")
    print(json.dumps({"bytes": len(raw), "sha256": digest(raw), **packet["metadata"]}, sort_keys=True))


if __name__ == "__main__":
    main()
