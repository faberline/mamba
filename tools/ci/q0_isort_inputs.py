"""Prepare fixed isort inputs from public sources, without expected outputs."""
from __future__ import annotations

import ast
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import warnings

INPUT_FIELDS = ("name", "source", "files", "directories", "extension")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def published(repo: Path) -> tuple[dict, dict]:
    root = repo / "crates/mamba-quality/src/infrastructure/isort/tests"
    accepted = {}
    for path in sorted((root / "catalog").glob("part_*.rs")):
        match = re.search(r'pub const CASES: &str = r(?P<marks>#+)"(?P<body>.*)"(?P=marks);', path.read_text(), re.S)
        if match is None:
            raise ValueError(f"invalid published input container: {path.name}")
        for case in json.loads(match["body"]):
            if case["id"] in accepted:
                raise ValueError("duplicate published case")
            accepted[case["id"]] = case
    excluded = {case["source_sha256"]: case for case in json.loads((root / "excluded_sources.json").read_text())}
    if len(accepted) != 1155 or len(excluded) != 200 or accepted.keys() & excluded.keys():
        raise ValueError("published case denominator differs")
    return accepted, excluded


def construct(sdist_bytes: bytes, repo: Path) -> tuple[list[dict], dict]:
    lock = json.loads((repo / "tools/ci/q0_isort_reference_lock.json").read_text())
    if digest(sdist_bytes) != lock["sdist"]["sha256"]:
        raise ValueError("source archive hash mismatch")
    cases, test_sources = {}, []
    with tarfile.open(fileobj=io.BytesIO(sdist_bytes), mode="r:gz") as archive:
        for member in sorted(archive.getmembers(), key=lambda row: row.name):
            name = PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts or not (member.isfile() or member.isdir()):
                raise ValueError(f"unsafe source member: {member.name}")
            if not member.isfile() or "/tests/" not in member.name or not member.name.endswith(".py"):
                continue
            raw = archive.extractfile(member).read()
            test_sources.append({"path": member.name, "sha256": digest(raw)})
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", SyntaxWarning)
                tree = ast.parse(raw.decode("utf-8"))
            for node in ast.walk(tree):
                if (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                        and node.func.attr == "assignment" and len(node.args) >= 2
                        and isinstance(node.func.value, ast.Attribute) and node.func.value.attr == "literal"):
                    try:
                        text, kind = ast.literal_eval(node.args[0]), ast.literal_eval(node.args[1])
                    except (ValueError, TypeError):
                        continue
                    if isinstance(text, str) and kind in ("list", "tuple", "set", "dict", "unique-list", "unique-tuple", "assignments"):
                        source = f"# isort: {kind}\n{text}"
                        identity = digest(source.encode())
                        case = cases.setdefault(identity, {"id": identity, "source": source, "origins": [], "explicit_directive_input": True})
                        case["origins"].append({"path": member.name, "line": node.lineno,
                                                "derivation": "original literal.assignment input under its public inline directive"})
                if isinstance(node, ast.Constant) and isinstance(node.value, str):
                    source = node.value
                    if not any(marker in source for marker in ("import ", "import(", "isort:")):
                        continue
                    identity = digest(source.encode())
                    case = cases.setdefault(identity, {"id": identity, "source": source, "origins": []})
                    case["origins"].append({"path": member.name, "line": node.lineno})
    if test_sources != lock["test_sources"] or len(test_sources) != 59:
        raise ValueError("test-source hash inventory differs")
    packet_path = repo / "tools/ci/q0_isort_project_inputs.json"
    if digest(packet_path.read_bytes()) != lock["project_inputs_sha256"]:
        raise ValueError("published project-input packet hash differs")
    extras = json.loads(packet_path.read_text())["cases"]
    if len(extras) != 288 or sum(case["name"].startswith("generated_") for case in extras) != 256:
        raise ValueError("project-input denominator differs")
    for extra in extras:
        if set(extra) - set(INPUT_FIELDS):
            raise ValueError("non-input field in project-input packet")
        identity = digest(json.dumps(extra, sort_keys=True).encode())
        if identity in cases:
            raise ValueError("duplicate constructed case")
        cases[identity] = {"id": identity, **extra,
                           "source_sha256": digest(extra["source"].encode()),
                           "origins": [{"path": "tools/ci/q0_isort_project_inputs.json", "case": extra["name"]}]}
    accepted, excluded = published(repo)
    if set(cases) != set(accepted) | set(excluded) or len(cases) != 1355:
        raise ValueError("constructed input identity set differs")
    for identity, case in cases.items():
        declared = accepted.get(identity, excluded.get(identity))
        fields = INPUT_FIELDS if identity in accepted else ("source", "files", "directories", "extension")
        if {k: case[k] for k in fields if k in case} != {k: declared[k] for k in fields if k in declared}:
            raise ValueError(f"constructed input bytes/layout differ: {identity}")
    proof = {"candidate_count": 1355, "accepted_ids": sorted(accepted), "excluded_ids": sorted(excluded),
             "test_sources": test_sources, "all_published_input_fields_match": True,
             "project_inputs_provenance": "already-published input fields only; no expected outcomes or protected helper copied",
             "historical_original_evidence_recovered": False}
    return sorted(cases.values(), key=lambda case: case["id"]), proof


def verify_results(repo: Path, cases: list[dict], results: dict[int, list[dict]]) -> dict:
    accepted, excluded = published(repo)
    identities = [case["id"] for case in cases]
    by_id = {case["id"]: case for case in cases}
    mapped = {}
    for minor in (12, 13, 14):
        rows = results.get(minor, [])
        if [row["id"] for row in rows] != identities:
            raise ValueError(f"reference result denominator/order differs: {minor}")
        mapped[minor] = {row["id"]: row for row in rows}
    actual_excluded = {identity for identity in identities
                       if all(mapped[m][identity].get("target_syntax_hint") for m in (12, 13, 14))
                       and not by_id[identity].get("explicit_directive_input")}
    if actual_excluded != set(excluded):
        raise ValueError("excluded classification differs")
    targets = []
    for index, minor in enumerate((12, 13, 14)):
        counts = {"reference_errors": 0, "skipped": 0, "nonconverging": 0, "converging": 0}
        for identity, expected_case in accepted.items():
            actual = mapped[minor][identity]
            expected = expected_case["expected"][index]
            if any(actual.get(key) != value for key, value in expected.items()):
                raise ValueError(f"frozen API result differs: {minor}/{identity}")
            if "reference_error" in actual:
                counts["reference_errors"] += 1
            else:
                counts["skipped"] += int(actual.get("reference_skipped", False))
                counts["converging"] += int(actual["reference_converges"])
                counts["nonconverging"] += int(not actual["reference_converges"])
                if not actual.get("reference_skipped") and (actual["output"] == actual["repeat_output"]) != actual["reference_converges"]:
                    raise ValueError("repeat status contradicts actual output bytes")
        if counts != {"reference_errors": 2, "skipped": 3, "nonconverging": 36, "converging": 1117}:
            raise ValueError(f"reference status counts differ: {minor}/{counts}")
        targets.append({"python_minor": minor, "accepted": 1155, "excluded": 200, **counts})
    return {"reference_comparisons_passed": True, "targets": targets,
            "historical_original_evidence_recovered": False, "whole_suite_accepted": False}


def verify_excluded_api_results(repo: Path, excluded_cases: list[dict], results_by_minor: dict[int, list[dict]]) -> dict:
    _, excluded = published(repo)
    if {case["id"] for case in excluded_cases} != set(excluded):
        raise ValueError("excluded API input denominator differs")
    targets = []
    for index, minor in enumerate((12, 13, 14)):
        rows = results_by_minor.get(minor, [])
        if len(rows) != 200 or {row["id"] for row in rows} != set(excluded):
            raise ValueError("excluded API result denominator differs")
        errors, outputs = {}, 0
        for row in rows:
            expected = excluded[row["id"]]["expected"][index]
            for key, value in expected.items():
                if key != "reference_exit" and row.get(key) != value:
                    raise ValueError(f"excluded API field differs: {minor}/{row['id']}/{key}")
            if row.get("error_class"):
                errors[row["error_class"]] = errors.get(row["error_class"], 0) + 1
            else:
                outputs += 1
        if errors != {"ExistingSyntaxErrors": 5, "LiteralParsingFailure": 1} or outputs != 194:
            raise ValueError("excluded API error/output denominator differs")
        targets.append({"python_minor": minor, "cases": 200, "outputs": outputs, "errors": errors})
    return {"excluded_api_comparisons_passed": True, "targets": targets, "case_count": 600}


def verify_checker_results(repo: Path, excluded_cases: list[dict], results_by_minor: dict[int, list[dict]]) -> dict:
    _, excluded = published(repo)
    source_by_id = {case["id"]: digest(case["source"].encode()) for case in excluded_cases}
    if set(source_by_id) != set(excluded):
        raise ValueError("checker input denominator differs")
    targets = []
    for index, minor in enumerate((12, 13, 14)):
        rows = results_by_minor.get(minor, [])
        if len(rows) != 200 or {row["id"] for row in rows} != set(excluded):
            raise ValueError("checker result denominator differs")
        exits = {0: 0, 1: 0}
        for row in rows:
            if not row["wait_completed"] or not row["source_unchanged"] or row["source_sha256"] != source_by_id[row["id"]]:
                raise ValueError("checker wait/source proof differs")
            expected = excluded[row["id"]]["expected"][index]["reference_exit"]
            if row["actual_exit"] != expected:
                raise ValueError(f"checker exit differs: {minor}/{row['id']}")
            exits[row["actual_exit"]] += 1
        if exits != {0: 136, 1: 64}:
            raise ValueError("checker exit denominator differs")
        targets.append({"python_minor": minor, "checked": 200, "exit0": 136, "exit1": 64})
    return {"checker_comparisons_passed": True, "targets": targets, "process_count": 600}
