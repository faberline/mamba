#!/usr/bin/env python3
"""Validate authored Q0 mypy input data without running its Python sources."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

SCHEMA = "mamba.q0.16.supplement-inputs.v1"
IDENTITY_FIELDS = (
    "behavior", "role", "group", "files", "entrypoints", "python_targets", "requires"
)
CASE_FIELDS = {*IDENTITY_FIELDS, "id", "intent", "provenance"}
ROLES = {"hit", "non_trigger", "interaction"}
TARGETS = ["3.12", "3.13", "3.14"]


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def case_id(case: dict) -> str:
    """Bind the source layout and intended predicates to a stable input ID."""
    identity = {key: case[key] for key in IDENTITY_FIELDS}
    raw = json.dumps(identity, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return digest(raw)


def safe_path(value: object) -> str:
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError("file path must be nonempty canonical POSIX text")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in value.split("/")):
        raise ValueError(f"unsafe file path: {value!r}")
    if path.as_posix() != value or path.suffix != ".py":
        raise ValueError(f"file path must be a canonical relative .py path: {value!r}")
    return value


def validate_packet(packet: dict, strict_flags: list[str]) -> dict:
    if set(packet) != {"schema", "tool", "cases", "dispositions"}:
        raise ValueError("unexpected packet fields")
    if packet["schema"] != SCHEMA or packet["tool"] != "mypy":
        raise ValueError("unexpected packet schema or tool")
    if not isinstance(packet["cases"], list) or not packet["cases"]:
        raise ValueError("cases must be a nonempty list")
    if not isinstance(packet["dispositions"], list):
        raise ValueError("dispositions must be a list")
    seen = set()
    rows = []
    for case in packet["cases"]:
        if not isinstance(case, dict) or set(case) != CASE_FIELDS:
            raise ValueError("unexpected case fields")
        if case["behavior"] not in strict_flags or case["role"] not in ROLES:
            raise ValueError("unknown strict flag or role")
        if not isinstance(case["group"], str) or not case["group"]:
            raise ValueError("group must be nonempty text")
        if not isinstance(case["intent"], str) or not case["intent"]:
            raise ValueError("intent must be nonempty text")
        if case["python_targets"] != TARGETS:
            raise ValueError("case must name all three ordered Python targets")
        files = case["files"]
        if not isinstance(files, dict) or not files:
            raise ValueError("files must be a nonempty path-to-source object")
        hashes = {}
        for filename, source in files.items():
            safe_path(filename)
            if not isinstance(source, str) or not source.endswith("\n"):
                raise ValueError("source must be UTF-8 text with its intended final newline")
            encoded = source.encode("utf-8")
            ast.parse(source, filename=filename)
            hashes[filename] = digest(encoded)
        entries = case["entrypoints"]
        if (not isinstance(entries, list) or not entries or len(entries) != len(set(entries))
                or any(safe_path(name) not in files for name in entries)):
            raise ValueError("entrypoints must be unique materialized source paths")
        if not isinstance(case["requires"], list) or not case["requires"]:
            raise ValueError("requires must contain intended observation predicates")
        for predicate in case["requires"]:
            if (not isinstance(predicate, dict) or not {"rule", "presence"} <= set(predicate)
                    or set(predicate) - {"rule", "presence", "path", "line", "message_contains"}):
                raise ValueError("invalid intended observation predicate")
            if not isinstance(predicate["rule"], str) or not re.fullmatch(r"[a-z][a-z0-9-]*", predicate["rule"]):
                raise ValueError("invalid diagnostic code")
            if predicate["presence"] not in {"present", "absent"}:
                raise ValueError("presence must be present or absent")
            if "path" in predicate and safe_path(predicate["path"]) not in files:
                raise ValueError("predicate path is outside the materialized source layout")
            if "line" in predicate:
                if ("path" not in predicate or type(predicate["line"]) is not int
                        or not 1 <= predicate["line"] <= len(files[predicate["path"]].splitlines())):
                    raise ValueError("predicate line is outside its source file")
            if "message_contains" in predicate:
                if not isinstance(predicate["message_contains"], str) or not predicate["message_contains"]:
                    raise ValueError("message_contains must be nonempty text")
        provenance = case["provenance"]
        if (not isinstance(provenance, dict) or provenance.get("kind") != "new-authored-input"
                or provenance.get("source_sha256") != hashes
                or provenance.get("reference_executed_at_authoring") is not False
                or provenance.get("requires_are_intended_predicates") is not True):
            raise ValueError("source hashes or authoring provenance differ")
        actual_id = case_id(case)
        if case["id"] != actual_id or not re.fullmatch(r"[0-9a-f]{64}", case["id"]) or actual_id in seen:
            raise ValueError("case ID is duplicated or differs from its source/predicate identity")
        seen.add(actual_id)
        rows.append({"id":actual_id, "behavior":case["behavior"], "role":case["role"],
                     "group":case["group"], "source_sha256":hashes, "entrypoints":entries,
                     "python_targets":case["python_targets"], "predicate_count":len(case["requires"])})
    return {"schema":SCHEMA, "tool":"mypy", "case_count":len(rows), "cases":rows,
            "validation_scope":"JSON, safe paths, IDs, UTF-8 hashes and static Python syntax only",
            "reference_native_or_fixture_executed":False, "acceptance_claim":False}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path(__file__).with_name("q0_16_cases") / "mypy.json")
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[2]
    profile = json.loads((repository / "crates/mamba-quality/assets/profile.json").read_bytes())
    raw = args.input.read_bytes()
    report = validate_packet(json.loads(raw), profile["mypy"]["strict_flags"])
    report.update(input_path=str(args.input), input_bytes=len(raw), input_sha256=digest(raw))
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
