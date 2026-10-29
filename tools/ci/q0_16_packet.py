"""Validate and materialize new Q0 input packets without executing their sources."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath
import re

SCHEMA = "mamba.q0.16.supplement-inputs.v1"
TARGETS = ("3.12", "3.13", "3.14")
TOOLS = {"mypy", "pylint", "pydantic"}
ROLES = {"hit", "non_trigger", "interaction"}
REFERENCE_FAILURE_KIND = "pylint-astroid-mixed-slots-v1"
REFERENCE_FAILURE_IDS = {"q016_pylint_f0002_paired_hit": "hit",
                         "q016_pylint_f0002_paired_disabled": "non_trigger",
                         "q016_pylint_f0002_paired_project_interaction": "interaction"}
REFERENCE_FAILURE_SOURCE = {"main.py": "887d71f5c0c0590172b39cc02c0aa019ce7d5eb228129e547a560ad4454b8fb0"}
CONFIG_BYTES = (b"[mypy]\nplugins = pydantic.mypy\n[pydantic-mypy]\n"
                b"init_typed = true\ninit_forbid_extra = true\n"
                b"warn_required_dynamic_aliases = true\n")


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def read_json(raw: bytes) -> object:
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON object key")
            result[key] = value
        return result
    def invalid_constant(value):
        raise ValueError("non-JSON numeric constant: " + value)
    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid_constant)


def safe_path(value: object) -> str:
    if (type(value) is not str or not value or value[0] in {"-", "@"} or "\\" in value
            or any(ord(char) < 32 for char in value)):
        raise ValueError("input path must be nonempty canonical POSIX text")
    path = PurePosixPath(value)
    if (path.is_absolute() or any(part in {"", ".", ".."} for part in value.split("/"))
            or path.as_posix() != value):
        raise ValueError("input path must remain inside its isolated project")
    return value


def source_facts(files: dict[str, str]) -> dict[str, dict]:
    return {name: {"bytes": len(text.encode()), "sha256": digest(text.encode())}
            for name, text in files.items()}


def reference_failure_contract(case: dict, tool: str) -> bool:
    """Admit one explicit known upstream error witness, never a general override."""
    fields = {name for name in case if name.startswith("reference_failure")}
    if not fields and case.get("id") not in REFERENCE_FAILURE_IDS:
        return False
    if (fields != {"reference_failure_kind", "reference_failure_source_sha256"}
            or tool != "pylint" or case.get("behavior") != "F0002"
            or case.get("role") != REFERENCE_FAILURE_IDS.get(case.get("id"))
            or case.get("group") != "q016_pylint_f0002_paired"
            or case["reference_failure_kind"] != REFERENCE_FAILURE_KIND
            or case["reference_failure_source_sha256"] != REFERENCE_FAILURE_SOURCE
            or case.get("missing_entrypoints")
            or "main.py" not in case.get("files", {})
            or digest(case["files"]["main.py"].encode()) != REFERENCE_FAILURE_SOURCE["main.py"]):
        raise ValueError("unknown or changed explicit reference-tool error contract")
    controlled = case["role"] == "non_trigger"
    if (case.get("disabled_rules", []) != (["F0002"] if controlled else [])
            or not any(item.get("rule") == "F0002" and item.get("presence") ==
                       ("absent" if controlled else "present") for item in case.get("requires", []))
            or case.get("entrypoints") != sorted(case["files"])
            or set(case["files"]) != ({"main.py", "q0_16_boundary_probe.py"}
                                      if case["role"] == "interaction" else {"main.py"})):
        raise ValueError("reference-tool error contract is outside its declared source and focus boundary")
    return True


def _targets(value: object) -> list[str]:
    if (type(value) is not list or not value
            or any(type(item) is not str or item not in TARGETS for item in value)
            or len(set(value)) != len(value)
            or value != [target for target in TARGETS if target in value]):
        raise ValueError("targets must be a nonempty ordered subset of the three exact targets")
    return value


def validate_case(case: dict, tool: str, profile: dict) -> dict:
    required = {"id", "behavior", "role", "group", "files", "entrypoints", "python_targets",
                "requires", "intent", "provenance"}
    if type(case) is not dict or not required <= set(case):
        raise ValueError("supplemental case lacks its declared input contract")
    canonical(case)  # Reject non-JSON numbers and invalid UTF-8 before materialization.
    if set(case) & {"expected", "diagnostics", "output", "actual_exit", "accepted"}:
        raise ValueError("input packets cannot carry frozen or claimed execution outcomes")
    if set(case) & {"argv", "command", "env", "environment", "extra_args", "config", "plugins",
                    "expected_exit", "expected_exits"}:
        raise ValueError("input packets cannot carry undeclared command or environment overrides")
    if (type(case["id"]) is not str
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", case["id"])):
        raise ValueError("case ID must be a safe single path component")
    if case["role"] not in ROLES:
        raise ValueError("unknown supplemental semantic role")
    for key in ("behavior", "group", "intent"):
        if type(case[key]) is not str or not case[key].strip():
            raise ValueError("case behavior, group and intent must be explicit")
    rules = {row["code"] for row in profile["pylint"]["rules"]}
    if (tool == "mypy" and case["behavior"] not in profile["mypy"]["strict_flags"]
            or tool == "pylint" and case["behavior"] not in rules
            or tool == "pydantic" and case["behavior"] not in
            {"pydantic." + name for name in profile["pydantic"]}):
        raise ValueError("case names a behavior outside the fixed profile")
    targets = _targets(case["python_targets"])
    files = case["files"]
    if type(files) is not dict or any(type(text) is not str for text in files.values()):
        raise ValueError("case sources must be a path-to-UTF8-text object")
    for name, text in files.items():
        safe_path(name)
        text.encode("utf-8")
    paths = {PurePosixPath(name) for name in files}
    if any(parent in paths for path in paths for parent in path.parents):
        raise ValueError("an input file collides with another input directory")
    if tool in {"mypy", "pydantic"} and "mypy.ini" in files:
        raise ValueError("case sources cannot override the pinned plugin configuration")
    if tool in {"mypy", "pydantic"}:
        protected = {"mypy", "pydantic", "pydantic_core", "typing_extensions", "pathspec",
                     "librt", "ast_serialize", "mypy_extensions", "annotated_types", "typing_inspection"}
        if any(PurePosixPath(name).parts[0] in protected or PurePosixPath(name).parts[0].removesuffix(".py") in protected
               for name in files):
            raise ValueError("input project cannot shadow a prepared reference package")
    entries = case["entrypoints"]
    if (type(entries) is not list or not entries or len(set(entries)) != len(entries)
            or any(type(name) is not str or not safe_path(name).endswith((".py", ".pyi"))
                   for name in entries)):
        raise ValueError("case must declare unique relative Python entrypoints")
    if tool == "pylint" and (entries != sorted(entries) or any(not name.endswith(".py") for name in entries)):
        raise ValueError("Pylint entrypoints must be sorted relative .py files")
    missing = case.get("missing_entrypoints", [])
    if (type(missing) is not list or len(set(missing)) != len(missing)
            or any(safe_path(name) not in entries or name in files for name in missing)):
        raise ValueError("missing entrypoint declarations differ from the source layout")
    if set(entries) - set(files) != set(missing):
        raise ValueError("every entrypoint must be materialized or an explicit missing-input boundary")
    if any(PurePosixPath(name) in path.parents or path in PurePosixPath(name).parents
           for name in missing for path in paths):
        raise ValueError("missing entrypoint collides with a materialized path")
    predicates = case["requires"]
    if type(predicates) is not list or not predicates:
        raise ValueError("each case requires nonempty intended observation predicates")
    for predicate in predicates:
        if (type(predicate) is not dict or not {"rule", "presence"} <= set(predicate)
                or set(predicate) - {"rule", "presence", "path", "line", "message_contains", "severity"}
                or type(predicate["rule"]) is not str or not predicate["rule"]
                or predicate["presence"] not in {"present", "absent"}):
            raise ValueError("invalid intended diagnostic predicate")
        pattern = r"[CEFIRW][0-9]{4}" if tool == "pylint" else r"[a-z][a-z0-9-]*"
        if predicate["rule"] == "*":
            if (tool not in {"mypy", "pydantic"}
                    or set(predicate) != {"rule", "presence", "severity", "path", "line"}
                    or predicate["presence"] != "absent" or predicate["severity"] != "error"
                    or type(predicate["line"]) is not int or predicate["line"] <= 0):
                raise ValueError("wildcard is only a bounded source-site error-absence control")
        elif re.fullmatch(pattern, predicate["rule"]) is None or "severity" in predicate:
            raise ValueError("predicate diagnostic code has an invalid form")
        if "path" in predicate and safe_path(predicate["path"]) not in set(files) | set(missing):
            raise ValueError("predicate path is outside the declared project")
        if "line" in predicate:
            if ("path" not in predicate or type(predicate["line"]) is not int
                    or predicate["line"] < (0 if tool == "pylint" else 1)):
                raise ValueError("predicate line must identify a source site")
            if predicate["path"] in files and predicate["line"] > max(1, len(files[predicate["path"]].splitlines()) + 1):
                raise ValueError("predicate site is outside its input source")
        if "message_contains" in predicate and (type(predicate["message_contains"]) is not str
                                                 or not predicate["message_contains"]):
            raise ValueError("message predicate must be nonempty exact text")
    missing_boundary = (tool == "pylint" and missing and any(
        item["rule"] == "F0001" and item["presence"] == "present" for item in predicates))
    if missing and not missing_boundary or not files and not missing_boundary:
        raise ValueError("only an explicit F0001 boundary can have missing or zero materialized files")
    disabled = case.get("disabled_rules", [])
    if (type(disabled) is not list or disabled and
            (tool != "pylint" or case["role"] != "non_trigger"
             or disabled != [case["behavior"]] or case["behavior"] not in rules
             or not any(item["rule"] == case["behavior"] and item["presence"] == "absent" for item in predicates))):
        raise ValueError("only a declared one-rule Pylint suppression control can alter default enablement")
    provenance = case["provenance"]
    if type(provenance) is not dict or not provenance:
        raise ValueError("case provenance must be explicit")
    if (provenance.get("reference_executed_at_authoring") not in (None, False)
            or provenance.get("full_diagnostic_outputs_frozen") not in (None, False)):
        raise ValueError("supplement input packet cannot claim prior target execution or frozen full output")
    hashes = {name: fact["sha256"] for name, fact in source_facts(files).items()}
    if "source_sha256" in provenance and provenance["source_sha256"] != hashes:
        raise ValueError("declared supplemental source hashes differ")
    failure_contract = reference_failure_contract(case, tool)
    semantic = {key: case.get(key, []) if key in {"disabled_rules", "missing_entrypoints"} else case[key]
                for key in ("behavior", "role", "group", "files", "entrypoints", "python_targets",
                            "requires", "disabled_rules", "missing_entrypoints")}
    if failure_contract:
        semantic.update({key: case[key] for key in ("reference_failure_kind", "reference_failure_source_sha256")})
    return {"id": case["id"], "behavior": case["behavior"], "role": case["role"],
            "targets": targets, "source_files": source_facts(files),
            "input_sha256": digest(canonical(semantic))}


def validate_packet(packet: dict, profile: dict) -> dict:
    if (type(packet) is not dict or packet.get("schema") != SCHEMA or packet.get("tool") not in TOOLS
            or type(packet.get("cases")) is not list or not packet["cases"]
            or type(packet.get("dispositions")) is not list):
        raise ValueError("packet needs its schema, tool, nonzero cases and separate dispositions")
    tool = packet["tool"]
    proofs = [validate_case(case, tool, profile) for case in packet["cases"]]
    if len({row["id"] for row in proofs}) != len(proofs):
        raise ValueError("duplicate supplemental case ID")
    pairs = {}
    for case in packet["cases"]:
        if not case.get("disabled_rules"):
            continue
        matches = [other for other in packet["cases"]
                   if other["role"] == "hit" and not other.get("disabled_rules")
                   and all(other[key] == case[key] for key in ("behavior", "group", "files", "entrypoints", "python_targets"))
                   and other.get("missing_entrypoints", []) == case.get("missing_entrypoints", [])
                   and all(other.get(key) == case.get(key) for key in
                           ("reference_failure_kind", "reference_failure_source_sha256"))
                   and any(item["rule"] == case["behavior"] and item["presence"] == "present" for item in other["requires"])]
        if len(matches) != 1:
            raise ValueError("one-rule suppression requires one identical-input, fresh hit partner")
        pairs[case["id"]] = matches[0]["id"]
    for item in packet["dispositions"]:
        if (type(item) is not dict or type(item.get("behavior")) is not str
                or not item.get("status") or not item.get("reason") or not item.get("source")):
            raise ValueError("disposition needs a named behavior, status, reason and source")
        _targets(item.get("python_targets"))
    return {"tool": tool, "case_count": len(proofs), "cases": proofs,
            "planned_target_cells": sum(len(row["targets"]) for row in proofs),
            "disposition_count": len(packet["dispositions"]), "disabled_rule_pairs": pairs,
            "target_program_executed": False}


def load_packets(repository: Path, profile: dict) -> list[dict]:
    root = repository / "tools/ci/q0_16_cases"
    paths = sorted(root.glob("*.json"))
    if not paths:
        raise ValueError("no supplemental packets")
    packets, identifiers, tools = [], set(), set()
    for path in paths:
        if path.is_symlink() or not path.is_file():
            raise ValueError("input packet must be a regular source file")
        raw = path.read_bytes()
        packet = read_json(raw)
        proof = validate_packet(packet, profile)
        if packet["tool"] in tools or identifiers.intersection(row["id"] for row in proof["cases"]):
            raise ValueError("duplicate packet tool or global case ID")
        tools.add(packet["tool"])
        identifiers.update(row["id"] for row in proof["cases"])
        packets.append({"packet": packet, "proof": proof,
                        "source": {"path": path.relative_to(repository).as_posix(), "bytes": len(raw), "sha256": digest(raw)}})
    return packets


def materialize(case: dict, tool: str, project: Path) -> dict:
    project.mkdir(parents=True, exist_ok=False)
    for parent in project.parents:
        if parent.is_symlink():
            raise ValueError("isolated project parent is a symlink")
    sources = dict(case["files"])
    if tool in {"mypy", "pydantic"}:
        sources["mypy.ini"] = CONFIG_BYTES.decode()
    manifest = source_facts(sources)
    for name, text in sources.items():
        target = project / safe_path(name)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(text.encode())
    return {"files": manifest, "entrypoints": list(case["entrypoints"]),
            "missing_entrypoints": list(case.get("missing_entrypoints", [])),
            "case_unit_denominator": 1, "entrypoint_denominator": len(case["entrypoints"]),
            "materialized_file_denominator": len(manifest)}


def evaluate_predicates(diagnostics: list[dict], predicates: list[dict]) -> list[dict]:
    if type(diagnostics) is not list or type(predicates) is not list or not predicates:
        raise ValueError("predicate evaluation requires observed diagnostics and nonempty predicates")
    results = []
    for predicate in predicates:
        indices = [index for index, item in enumerate(diagnostics)
                   if (predicate["rule"] == "*" or item["rule"] == predicate["rule"])
                   and ("severity" not in predicate or item["severity"] == predicate["severity"])
                   and ("path" not in predicate or item["path"] == predicate["path"])
                   and ("line" not in predicate or item["location"]["line"] == predicate["line"])
                   and ("message_contains" not in predicate or predicate["message_contains"] in item["message"])]
        results.append({"predicate": predicate, "matching_diagnostic_indices": indices,
                        "passed": bool(indices) if predicate["presence"] == "present" else not indices})
    return results
