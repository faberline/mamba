"""Read the fixed public mypy inputs and compare retained JSON diagnostics."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

CASE_COUNT = 22898
CHUNK_COUNT = 716
INPUT_SHA256 = "34a18754668b79baa71a16314d4eac07209c9a03111850546a78cc29ff711637"
CORPUS_SHA256 = "e09035932af5208a25229a95eb5986bec8500408cb65d74daf02d3249d1a3dd3"
CONFIG_BYTES = (b"[mypy]\nplugins = pydantic.mypy\n[pydantic-mypy]\n"
                b"init_typed = true\ninit_forbid_extra = true\n"
                b"warn_required_dynamic_aliases = true\n")
STRICT_FLAGS = [
    "--disallow-any-generics", "--disallow-subclassing-any", "--disallow-untyped-calls",
    "--disallow-untyped-defs", "--disallow-incomplete-defs", "--check-untyped-defs",
    "--disallow-untyped-decorators", "--warn-redundant-casts", "--warn-unused-ignores",
    "--warn-return-any", "--no-implicit-reexport", "--strict-equality", "--extra-checks",
]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_inventory(repo: Path) -> list[dict]:
    root = repo / "crates/mamba-quality/src/infrastructure/mypy/tests"
    paths = [root / "catalog.rs", *sorted((root / "catalog").rglob("*.rs"))]
    return [{"path": str(p.relative_to(repo)), "sha256": digest(p.read_bytes())} for p in paths]


def published(repo: Path) -> list[dict]:
    root = repo / "crates/mamba-quality/src/infrastructure/mypy/tests"
    index = (root / "catalog.rs").read_text()
    if (f"CASE_COUNT: usize = {CASE_COUNT};" not in index
            or CORPUS_SHA256 not in index
            or f"[&'static str; {CHUNK_COUNT}]" not in index):
        raise ValueError("published mypy index identity differs")
    groups = re.findall(r"(group_\d{3})::CHUNKS", index)
    if groups != re.findall(r"mod (group_\d{3});", index) or len(groups) != 12:
        raise ValueError("published mypy group order differs")
    rows, paths = [], []
    for group in groups:
        text = (root / "catalog" / (group + ".rs")).read_text()
        parts = re.findall(r"(part_\d{3})::CASES", text)
        if not parts or parts != re.findall(r"mod (part_\d{3});", text):
            raise ValueError("published mypy part order differs")
        for part in parts:
            path = root / "catalog" / group / (part + ".rs")
            paths.append(path)
            match = re.search(r'pub const CASES: &str = r(?P<h>#+)"(?P<body>.*)"(?P=h);',
                              path.read_text(), re.S)
            if match is None:
                raise ValueError("invalid published mypy input container")
            value = json.loads(match["body"])
            if not isinstance(value, list) or not value:
                raise ValueError("empty published mypy input container")
            rows.extend(value)
    if len(paths) != CHUNK_COUNT or set(paths) != set((root / "catalog").glob("group_*/part_*.rs")):
        raise ValueError("published mypy chunk denominator differs")
    if len(rows) != CASE_COUNT or len({r["name"] for r in rows}) != CASE_COUNT:
        raise ValueError("published mypy case denominator differs")
    for index, row in enumerate(rows):
        extension = row.get("extension", ".py")
        if extension not in (".py", ".pyi") or row["filename"] != f"case_{index:04}{extension}":
            raise ValueError("published mypy filename formula differs")
        if digest(row["source"].encode()) != row["source_sha256"] or len(row["expected"]) != 3:
            raise ValueError("published mypy source/target identity differs")
    return rows


def prepare_inputs(repo: Path) -> tuple[list[dict], dict]:
    rows = published(repo)
    # Twelve original rows put extension before source. Preserve their key order.
    allowed = {"name", "source", "extension"}
    inputs = [{k: v for k, v in row.items() if k in allowed} for row in rows]
    original_bytes = (json.dumps(inputs, indent=2) + "\n").encode()
    if len(original_bytes) != 6335487 or digest(original_bytes) != INPUT_SHA256:
        raise ValueError("original mypy input-only bytes differ")
    lock = json.loads((repo / "tools/ci/q0_mypy_reference_lock.json").read_bytes())
    sources = source_inventory(repo)
    if digest(json.dumps(sources, sort_keys=True).encode()) != lock["published_catalog_inventory_sha256"]:
        raise ValueError("published mypy source inventory differs")
    cases = [{**value, "filename": row["filename"], "source_sha256": row["source_sha256"]}
             for value, row in zip(inputs, rows)]
    proof = {"case_count": CASE_COUNT, "target_count": 3, "chunk_count": CHUNK_COUNT,
             "original_input_bytes": len(original_bytes), "original_input_sha256": INPUT_SHA256,
             "input_fields_only": True, "all_published_input_fields_match": True,
             "py_files": sum(c["filename"].endswith(".py") for c in cases),
             "pyi_files": sum(c["filename"].endswith(".pyi") for c in cases),
             "unique_source_hashes": len({c["source_sha256"] for c in cases}),
             "published_sources": sources, "historical_original_evidence_recovered": False}
    if (proof["py_files"], proof["pyi_files"], proof["unique_source_hashes"]) != (18822, 4076, 21652):
        raise ValueError("mypy input type/duplicate denominator differs")
    return cases, proof


def materialize(cases: list[dict], project: Path) -> dict:
    if len(cases) != CASE_COUNT:
        raise ValueError("materialization requires every fixed mypy case")
    project.mkdir(parents=True, exist_ok=False)
    manifest = {}
    for index, case in enumerate(cases):
        filename = case["filename"]
        if filename != f"case_{index:04}{case.get('extension', '.py')}":
            raise ValueError("case order or filename differs")
        raw = case["source"].encode()
        if digest(raw) != case["source_sha256"]:
            raise ValueError("source bytes differ before materialization")
        with (project / filename).open("xb") as stream:
            stream.write(raw)
        manifest[filename] = {"bytes": len(raw), "sha256": digest(raw)}
    with (project / "mypy.ini").open("xb") as stream:
        stream.write(CONFIG_BYTES)
    manifest["mypy.ini"] = {"bytes": len(CONFIG_BYTES), "sha256": digest(CONFIG_BYTES)}
    return {"case_filenames": [case["filename"] for case in cases], "files": manifest,
            "case_count": CASE_COUNT, "config_sha256": digest(CONFIG_BYTES)}


def parse_diagnostics(stdout: bytes, filenames: set[str], project: Path) -> dict[str, list[dict]]:
    if not filenames:
        raise ValueError("diagnostic parsing requires a nonempty input denominator")
    grouped = {name: [] for name in filenames}
    prefix = str(project) + "/"
    for line in stdout.splitlines():
        item = json.loads(line)
        path = item["file"]
        if not isinstance(path, str):
            raise ValueError("diagnostic file is not a string")
        if Path(path).is_absolute():
            if not path.startswith(prefix):
                raise ValueError("diagnostic is outside the fixed project")
            path = path[len(prefix):]
        if path not in grouped:
            raise ValueError("diagnostic names an unknown input")
        grouped[path].append({"rule": item["code"], "message": item["message"],
                              "hint": item["hint"], "path": path, "severity": item["severity"],
                              "location": {"line": item["line"], "column": item["column"] + 1,
                                           "end_line": item["end_line"], "end_column": item["end_column"]}})
    return grouped


def compare_output(repo: Path, cases: list[dict], stdout: bytes, project: Path, minor: int) -> dict:
    if minor not in (12, 13, 14):
        raise ValueError("unexpected target")
    expected_cases = published(repo)
    if (len(cases) != CASE_COUNT or [(c["name"], c["filename"], c["source_sha256"]) for c in cases]
            != [(c["name"], c["filename"], c["source_sha256"]) for c in expected_cases]):
        raise ValueError("comparison input denominator/order differs")
    grouped = parse_diagnostics(stdout, {c["filename"] for c in cases}, project)
    results = []
    negative = zero = diagnostics = 0
    for case, original in zip(cases, expected_cases):
        actual = grouped[case["filename"]]
        expected = original["expected"][minor - 12]
        equal = actual == expected
        row = {"name": case["name"], "filename": case["filename"],
               "source_sha256": case["source_sha256"], "diagnostics": actual,
               "matches_expected": equal, "diagnostic_count": len(actual)}
        if not equal:
            row["expected"] = expected
        results.append(row)
        diagnostics += len(actual)
        negative += sum(d["location"]["end_column"] < 0 for d in actual)
        zero += sum(d["location"]["end_column"] == 0 for d in actual)
    counts_match = diagnostics == {12: 25951, 13: 25811, 14: 25811}[minor] and negative == 12 and zero == 9
    return {"python_minor": minor, "case_count": len(results), "diagnostic_count": diagnostics,
            "negative_end_columns": negative, "zero_end_columns": zero,
            "comparisons_passed": all(r["matches_expected"] for r in results) and counts_match,
            "matching_case_count": sum(r["matches_expected"] for r in results),
            "cases": results, "issue_accepted": False, "whole_suite_accepted": False,
            "historical_original_evidence_recovered": False}
