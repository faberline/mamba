"""Prepare only the fixed public Pylint inputs and original JSON2 projection."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

CASE_COUNT = 19747
CHUNK_COUNT = 1235
GROUP_COUNT = 78
INPUT_SHA256 = "56bd6674a569c788470395ecb2af55419d655f2a7906397a460d39c153990a17"
CORPUS_SHA256 = "d595c77c2ad00d3175ef447ebf4afa11cb2cadcb57c82b670e12d4cf4c1093d4"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def relative_file(name: str) -> Path:
    path = Path(name)
    if not name or path.is_absolute() or ".." in path.parts or str(path) == ".":
        raise ValueError("fixture file must remain inside its case project")
    return path


def source_inventory(repo: Path) -> list[dict]:
    root = repo / "crates/mamba-quality/src/infrastructure/pylint/tests"
    paths = [root / "catalog.rs", *sorted((root / "catalog").rglob("*.rs"))]
    return [{"path": str(p.relative_to(repo)), "sha256": digest(p.read_bytes())} for p in paths]


def published(repo: Path) -> list[dict]:
    root = repo / "crates/mamba-quality/src/infrastructure/pylint/tests"
    header = (root / "catalog.rs").read_text()
    if f"CASE_COUNT: usize = {CASE_COUNT};" not in header or CORPUS_SHA256 not in header:
        raise ValueError("published Pylint index identity differs")
    groups = re.findall(r"(group_\d+)::CHUNKS", header)
    declarations = re.findall(r"mod (group_\d+);", header)
    if (len(groups) != GROUP_COUNT or len(set(groups)) != GROUP_COUNT
            or len(declarations) != GROUP_COUNT or set(groups) != set(declarations)):
        raise ValueError("published Pylint group denominator differs")
    rows, paths = [], []
    for group in groups:
        text = (root / "catalog" / (group + ".rs")).read_text()
        # rustfmt sorts module declarations lexically. References retain input order.
        parts = re.findall(r"(part_\d+)::CASES", text)
        declared = re.findall(r"mod (part_\d+);", text)
        if not parts or len(set(parts)) != len(parts) or len(declared) != len(parts) or set(parts) != set(declared):
            raise ValueError("published Pylint part denominator differs")
        for part in parts:
            path = root / "catalog" / group / (part + ".rs")
            paths.append(path)
            match = re.search(r'pub const CASES: &str = r(?P<h>#+)"(?P<body>.*)"(?P=h);',
                              path.read_text(), re.S)
            if match is None:
                raise ValueError("invalid published Pylint input container")
            value = json.loads(match["body"])
            if not isinstance(value, list) or not value:
                raise ValueError("empty published Pylint input container")
            rows.extend(value)
    if len(paths) != CHUNK_COUNT or set(paths) != set((root / "catalog").glob("group_*/part_*.rs")):
        raise ValueError("published Pylint chunk denominator differs")
    if len(rows) != CASE_COUNT or len({row["name"] for row in rows}) != CASE_COUNT:
        raise ValueError("published Pylint case denominator differs")
    for row in rows:
        if not isinstance(row["files"], dict) or not row["files"]:
            raise ValueError("Pylint case has no source files")
        hashes = {}
        for name, contents in row["files"].items():
            relative_file(name)
            hashes[name] = digest(contents.encode())
        if hashes != row["source_sha256"]:
            raise ValueError("Pylint fixture source hashes differ")
        if len(row["expected"]) != 3 or len(row["reference_exits"]) != 3:
            raise ValueError("Pylint target denominator differs")
        if any(type(code) is not int or code < 0 or code & 32 for code in row["reference_exits"]):
            raise ValueError("invalid frozen Pylint reference exit")
    return rows


def prepare_inputs(repo: Path) -> tuple[list[dict], dict]:
    rows = published(repo)
    inputs = [{key: value for key, value in row.items() if key in {"name", "files"}}
              for row in rows]
    raw = (json.dumps(inputs, indent=2) + "\n").encode()
    if len(raw) != 5911082 or digest(raw) != INPUT_SHA256:
        raise ValueError("original Pylint input-only bytes differ")
    lock = json.loads((repo / "tools/ci/q0_pylint_reference_lock.json").read_bytes())
    sources = source_inventory(repo)
    if digest(json.dumps(sources, sort_keys=True).encode()) != lock["published_catalog_inventory_sha256"]:
        raise ValueError("published Pylint catalog source inventory differs")
    for name, expected in lock["comparator_sources"].items():
        if digest((repo / name).read_bytes()) != expected:
            raise ValueError("owning Pylint comparator source differs: " + name)
    cases = [{**data, "source_sha256": {name: digest(source.encode()) for name, source in data["files"].items()},
              "index": index} for index, data in enumerate(inputs)]
    files = [name for case in cases for name in case["files"]]
    if len(files) != 20651 or sum(name.endswith(".py") for name in files) != 20500:
        raise ValueError("Pylint fixture file denominator differs")
    proof = {"case_count": CASE_COUNT, "target_count": 3, "target_cells": 59241,
             "source_file_count": len(files), "python_file_count": 20500,
             "input_bytes": len(raw), "input_sha256": digest(raw),
             "group_count": GROUP_COUNT, "chunk_count": CHUNK_COUNT,
             "published_sources": sources, "input_fields_only": True,
             "all_published_input_fields_match": True,
             "module_declaration_order_is_not_input_order": True,
             "historical_original_evidence_recovered": False}
    return cases, proof


def materialize_case(case: dict, project: Path) -> dict:
    if not case["files"]:
        raise ValueError("Pylint materialization requires fixture files")
    project.mkdir(parents=True, exist_ok=False)
    manifest = {}
    for name, contents in case["files"].items():
        path = project / relative_file(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        raw = contents.encode()
        if digest(raw) != case["source_sha256"][name]:
            raise ValueError("Pylint source bytes differ before materialization")
        with path.open("xb") as stream:
            stream.write(raw)
        manifest[name] = {"bytes": len(raw), "sha256": digest(raw)}
    python_files = sorted(name for name in case["files"] if name.endswith(".py"))
    if not python_files:
        raise ValueError("Pylint reference command has no Python files")
    return {"files": manifest, "python_files": python_files}


def project_diagnostics(stdout: bytes, project: Path) -> list[dict]:
    document = json.loads(stdout)
    messages = document["messages"]
    if not isinstance(messages, list):
        raise ValueError("Pylint JSON2 messages must be a list")
    diagnostics = []
    for message in messages:
        path = Path(message["absolutePath"])
        if not path.is_absolute():
            path = project / path
        relative = path.relative_to(project)
        if ".." in relative.parts:
            raise ValueError("Pylint diagnostic is outside the case project")
        diagnostics.append({"rule": message["messageId"], "message": message["message"],
                            "hint": None, "path": relative.as_posix(), "severity": message["type"],
                            "location": {"line": message["line"], "column": message["column"],
                                         "end_line": message["endLine"], "end_column": message["endColumn"]}})
    return diagnostics
