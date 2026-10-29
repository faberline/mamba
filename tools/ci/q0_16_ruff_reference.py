#!/usr/bin/env python3
"""Capture only the pinned 29-source Ruff interaction project on hosted Linux.

Each target has four whole-project commands. Rule/role rows reference those
commands; they do not invent per-file exits or establish native acceptance.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import sys
import tomllib
import zipfile

from q0_16_packet import read_json, safe_path
from q0_16_reference import artifact_inventory, fact, require_saved_receipt, strict_json_bytes
from q0_mypy_reference import require_receipt
from q0_ruff_inputs import SELECT
from q0_ruff_reference import VERSIONS, checked_bytes, digest, download, prepare, ruff, write_json, write_sources

CORPUS_PATH = "crates/mamba-quality/src/infrastructure/ruff/tests/interactions.json"
CORPUS_SHA = "a3d1af9e2d2aa79dc4b856d8f9d17f812619f113d03bd210a1cee41a4bb062ec"
LOCK_SHA = "bb06e0320e4e941085e3198e15be9ab5dc4bbd6406f2c71e3b52d249b282f88b"
PROFILE_SHA = "59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7"
RULES = ("B016", "B022", "B032", "B033", "B034", "SIM112", "E743", "F622", "F631",
         "UP005", "UP009", "UP021", "UP042", "UP047")
CONFIG = b"[project]\nrequires-python='>=3.12'\n"
STAGES = ("before", "fix-1", "fix-2", "after")


def host_guard() -> None:
    if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != "linux"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or os.environ.get("RUNNER_OS") != "Linux" or os.environ.get("RUNNER_ARCH") != "X64"
            or os.environ.get("ImageOS") != "ubuntu24" or platform.machine() != "x86_64"):
        raise ValueError("reference capture requires the hosted Ubuntu 24.04 X64 job")


def decode_sources(document: dict) -> dict[str, bytes]:
    sources = document.get("sources")
    if type(sources) is not dict or len(sources) != 29:
        raise ValueError("the fixed interaction source denominator must be 29")
    result = {}
    for name, row in sources.items():
        safe_path(name)
        if "/" in name or not name.endswith(".py") or type(row) is not dict:
            raise ValueError("interaction sources must be declared top-level Python files")
        text = row.get("source_hex")
        if type(text) is not str or len(text) % 2 or any(char not in "0123456789abcdef" for char in text):
            raise ValueError("source bytes must be canonical lower-case hex")
        result[name] = checked_bytes(bytes.fromhex(text), row["source_sha256"])
        result[name].decode("utf-8")
    return dict(sorted(result.items()))


def validate_inputs(corpus: bytes, lock_raw: bytes, profile_raw: bytes) -> tuple[dict, dict, dict]:
    checked_bytes(corpus, CORPUS_SHA)
    checked_bytes(lock_raw, LOCK_SHA)
    checked_bytes(profile_raw, PROFILE_SHA)
    data, lock, profile = read_json(corpus), read_json(lock_raw), read_json(profile_raw)
    sources = validate_contract(data, lock, profile)
    return data, sources, lock


def validate_contract(data: dict, lock: dict, profile: dict) -> dict:
    sources = decode_sources(data)
    if (SELECT != "E4,E7,E9,F,UP,B,SIM,RUF100" or lock["profile_sha256"] != PROFILE_SHA
            or lock["python_versions"] != list(VERSIONS.values()) or lock["ruff_version"] != "0.16.10"
            or len(profile["ruff"]["rules"]) != 174
            or len({row["code"] for row in profile["ruff"]["rules"]}) != 174):
        raise ValueError("the pinned full rule profile or exact target set differs")
    declarations = data.get("rules")
    if type(declarations) is not dict or set(declarations) != set(RULES):
        raise ValueError("the interaction declaration denominator must be 14")
    for rule, declaration in declarations.items():
        if (set(declaration) != {"positive", "negative", "interaction_rule"}
                or any(declaration[key] not in sources for key in ("positive", "negative"))
                or declaration["positive"] == declaration["negative"]
                or declaration["interaction_rule"] not in set(RULES) - {rule}):
            raise ValueError("interaction controls need distinct declared sources and a distinct enabled partner")
    versions = data.get("versions")
    if type(versions) is not list or [row.get("python") for row in versions] != ["3.12", "3.13", "3.14"]:
        raise ValueError("all three ordered frozen target sections are required")
    for target in versions:
        if (target.get("same_file_rule_controls") != list(RULES) or target.get("stable") is not True
                or set(target.get("fixed_hex", {})) != set(sources)
                or any(type(target.get(key)) is not int or target[key] not in (0, 1)
                       for key in ("before_exit", "fix_exit", "repeat_exit", "after_exit"))):
            raise ValueError("frozen stage, full fixed-byte or same-file contracts differ")
        for raw in target["fixed_hex"].values():
            if type(raw) is not str or len(raw) % 2 or any(char not in "0123456789abcdef" for char in raw):
                raise ValueError("fixed source bytes must be canonical lower-case hex")
        if not all(row["role_observations_passed"] for row in rule_rows(target["before"], declarations, 12)):
            raise ValueError("a frozen interaction declaration lacks both positive co-emission and clean negatives")
    return sources


def project_messages(messages: list[dict], project: Path, sources: dict[str, bytes]) -> list[dict]:
    """Match Rust tests/reports.rs; only strip the known project path prefix.

    JSON fix locations become UTF-8 byte offsets. Messages, order, positions,
    applicability and replacement text are otherwise unchanged.
    """
    if type(messages) is not list:
        raise ValueError("Ruff stdout must contain a diagnostic array")
    def offset(raw, point):
        if (type(point) is not dict or set(point) != {"row", "column"}
                or any(type(point[key]) is not int or point[key] < 1 for key in point)):
            raise ValueError("Ruff edit position must have positive integer coordinates")
        lines = raw.decode("utf-8").splitlines(keepends=True)
        if not lines or lines[-1].endswith(("\n", "\r")):
            lines.append("")
        row, column = point["row"] - 1, point["column"] - 1
        if row >= len(lines) or column > len(lines[row].rstrip("\r\n")):
            raise ValueError("Ruff edit position is outside its actual source")
        return len("".join(lines[:row]).encode()) + len(lines[row][:column].encode())
    result = []
    for message in messages:
        path = Path(message["filename"])
        if not path.is_absolute():
            raise ValueError("Ruff diagnostic path must identify its absolute project source")
        name = path.relative_to(project).as_posix()
        if safe_path(name) not in sources:
            raise ValueError("Ruff diagnostic is outside the exact declared sources")
        fix = message["fix"]
        if fix is not None:
            edits = []
            for edit in fix["edits"]:
                start, end = offset(sources[name], edit["location"]), offset(sources[name], edit["end_location"])
                if end < start or type(edit["content"]) is not str:
                    raise ValueError("Ruff fix edit range or replacement is invalid")
                edits.append({"start": start, "end": end, "replacement": edit["content"]})
            fix = {"applicability": fix["applicability"], "message": fix["message"], "edits": edits}
        result.append({"rule": message["code"], "message": message["message"], "path": name,
            "severity": "error", "location": {"line": message["location"]["row"],
                "column": message["location"]["column"], "end_line": message["end_location"]["row"],
                "end_column": message["end_location"]["column"]}, "fix": fix})
    return result


def rule_rows(diagnostics: list[dict], declarations: dict, minor: int) -> list[dict]:
    rows = []
    for rule, declaration in declarations.items():
        partner = declaration["interaction_rule"]
        roles = {}
        for role in ("positive", "negative"):
            path = declaration[role]
            focus = [i for i, row in enumerate(diagnostics) if row["path"] == path and row["rule"] == rule]
            other = [i for i, row in enumerate(diagnostics) if row["path"] == path and row["rule"] == partner]
            roles[role] = {"path": path, "focus_diagnostic_indices": focus,
                "partner_diagnostic_indices": other, "passed": bool(focus) and bool(other) if role == "positive" else not focus and not other,
                "per_file_process_exit": None, "exit_scope": "whole 29-source project only"}
        rows.append({"rule": rule, "interaction_rule": partner, "python_version": VERSIONS[minor],
            "python_target": f"3.{minor}", "roles": roles,
            "role_observations_passed": all(row["passed"] for row in roles.values()),
            "issue_accepted": False, "native_parity": "not_run"})
    return rows


def source_state(project: Path, names: list[str], destination: Path) -> dict[str, bytes]:
    raw = {name: (project / name).read_bytes() for name in [*names, "pyproject.toml"]}
    if artifact_inventory(project, []) != {name: {"kind": "regular", "bytes": len(content), "sha256": digest(content)}
                                          for name, content in sorted(raw.items())}:
        raise ValueError("project entries escaped the exact source/config boundary")
    write_sources(destination, raw)
    return raw


def stage(binary: Path, project: Path, names: list[str], minor: int, target: dict, name: str, root: Path) -> dict:
    capture = root / name
    initial, current = None, None
    row = {"stage": name, "status": "failed", "receipt": None, "diagnostics": None, "errors": [],
           "process_launched": False,
           "receipt_pointer": f"{name}/receipt.json", "stdout_pointer": f"{name}/stdout.raw",
           "stderr_pointer": f"{name}/stderr.raw"}
    try:
        initial = source_state(project, names, root / "states" / (name + "-input"))
        row["source_inputs"] = {path: {"bytes": len(raw), "sha256": digest(raw)} for path, raw in initial.items()}
        messages, returned = ruff(binary, project, names, minor, capture, fixing=name.startswith("fix"))
        row["process_launched"] = type(returned.get("pid")) is int and returned["pid"] > 0
        returned_path = capture / "returned-receipt.json"
        write_json(returned_path, returned)
        returned_json = read_json(returned_path.read_bytes())
        row["returned_receipt_pointer"] = f"{name}/returned-receipt.json"
        row["returned_receipt_sha256"] = digest(returned_path.read_bytes())
        row["capture_process_fact"] = {key: returned_json.get(key) for key in (
            "pid", "waited_pid", "wait_completed", "actual_exit", "timed_out", "launch_error")}
        row["receipt"] = read_json((capture / "receipt.json").read_bytes())
        receipt = require_saved_receipt(returned, row["receipt"], (0, 1))
        require_receipt(receipt, (0, 1))
        stdout, stderr = (capture / "stdout.raw").read_bytes(), (capture / "stderr.raw").read_bytes()
        if (digest(stdout) != receipt["stdout_sha256"] or digest(stderr) != receipt["stderr_sha256"]
                or digest((capture / "inputs.json").read_bytes()) != receipt["inputs_sha256"]
                or strict_json_bytes(read_json((capture / "inputs.json").read_bytes())) != strict_json_bytes(
                    {path: {"bytes": len(initial[path]), "sha256": digest(initial[path])} for path in names})
                or strict_json_bytes(read_json(stdout)) != strict_json_bytes(messages)):
            raise ValueError("raw diagnostic/input bytes differ from the complete actual receipt")
        if stderr:
            raise ValueError("Ruff reference produced unexpected stderr")
        current = source_state(project, names, root / "states" / (name + "-output"))
        row["source_outputs"] = {path: {"bytes": len(raw), "sha256": digest(raw)} for path, raw in current.items()}
        if current["pyproject.toml"] != CONFIG or initial["pyproject.toml"] != CONFIG:
            raise ValueError("the exact pinned project configuration changed")
        fixed = {path: bytes.fromhex(raw) for path, raw in target["fixed_hex"].items()}
        if name == "before":
            if current != initial:
                raise ValueError("check changed original source bytes")
        elif {path: current[path] for path in names} != fixed:
            raise ValueError("full actual fixed bytes differ from the frozen 29-source map")
        if name in {"fix-2", "after"} and current != initial:
            raise ValueError("repeat safe fix or after-check changed source bytes")
        projected = project_messages(messages, project, {path: current[path] for path in names})
        row["diagnostics"] = projected
        expected = target["before" if name == "before" else "after"]
        expected_exit = target[{"before": "before_exit", "fix-1": "fix_exit", "fix-2": "repeat_exit", "after": "after_exit"}[name]]
        if strict_json_bytes(projected) != strict_json_bytes(expected) or receipt["actual_exit"] != expected_exit:
            raise ValueError("complete ordered diagnostics or full actual project exit differ from the frozen target")
        row["status"] = "passed"
    except Exception as error:
        row["errors"].append({"type": type(error).__name__, "message": str(error)})
        if row["receipt"] is None and (capture / "receipt.json").is_file():
            try:
                row["receipt"] = read_json((capture / "receipt.json").read_bytes())
            except Exception as receipt_error:
                row["errors"].append({"stage": "receipt_recovery", "type": type(receipt_error).__name__,
                                      "message": str(receipt_error)})
        recorded = row.get("capture_process_fact") or row.get("receipt") or {}
        if type(recorded.get("pid")) is int and recorded["pid"] > 0:
            row["process_launched"] = True
    if initial is not None and current is None:
        try:
            current = source_state(project, names, root / "states" / (name + "-output"))
            row["source_outputs"] = {path: {"bytes": len(raw), "sha256": digest(raw)} for path, raw in current.items()}
        except Exception as source_error:
            row["errors"].append({"stage": "failed_output_custody", "type": type(source_error).__name__,
                                  "message": str(source_error)})
    write_json(root / (name + "-result.json"), row)
    return row


def environment_state(root: Path, python: Path, binary: Path, phase: str) -> dict:
    venv = root / "venv"
    payloads = artifact_inventory(venv, [])
    records = sorted(venv.rglob("*.dist-info/RECORD"))
    if not payloads or not records or len([path for path in records if path.parent.name == "ruff-0.16.10.dist-info"]) != 1:
        raise ValueError("prepared environment lacks its pinned Ruff RECORD")
    destination = root / ("package-records-" + phase)
    write_sources(destination, {path.relative_to(venv).as_posix(): path.read_bytes() for path in records})
    state = {"base_python": fact(python.resolve(strict=True)), "ruff_binary": fact(binary),
             "venv_python": fact(venv / "bin/python"), "payloads": payloads,
             "records": {path.relative_to(venv).as_posix(): fact(path) for path in records}}
    write_json(root / ("environment-" + phase + ".json"), state)
    return state


def run_target(python: Path, minor: int, root: Path, wheel: Path, data: dict, sources: dict) -> dict:
    result = {"python_version": VERSIONS[minor], "status": "failed", "stages": {}, "errors": []}
    binary = None
    try:
        if not python.is_absolute():
            raise ValueError("interpreter paths must be absolute")
        binary = prepare(python, VERSIONS[minor], root, wheel)
        with zipfile.ZipFile(wheel) as archive:
            members = [name for name in archive.namelist() if name.endswith(".data/scripts/ruff")]
            if len(members) != 1 or digest(archive.read(members[0])) != fact(binary)["sha256"]:
                raise ValueError("prepared Ruff binary differs from the exact pinned wheel member")
        before = environment_state(root, python, binary, "before")
        project = root / "project"
        write_sources(project, {**sources, "pyproject.toml": CONFIG})
        target = data["versions"][minor - 12]
        for name in STAGES:
            result["stages"][name] = stage(binary, project, sorted(sources), minor, target, name, root)
        after = environment_state(root, python, binary, "after")
        result["environment_unchanged"] = strict_json_bytes(before) == strict_json_bytes(after)
        if not result["environment_unchanged"]:
            raise ValueError("prepared package payloads, records or interpreter/binary changed")
        result["status"] = "passed" if all(row["status"] == "passed" for row in result["stages"].values()) else "failed"
    except Exception as error:
        result["errors"].append({"type": type(error).__name__, "message": str(error)})
    root.mkdir(parents=True, exist_ok=True)
    for name in STAGES:
        result["stages"].setdefault(name, {"stage": name, "status": "not_run", "receipt": None,
                                         "diagnostics": None, "reason": "target prerequisite failed"})
    write_json(root / "target-result.json", result)
    return result


def source_manifest(repository: Path) -> dict:
    paths = [repository / CORPUS_PATH, repository / "crates/mamba-quality/assets/profile.json",
             repository / "Cargo.lock", *repository.joinpath("tools/ci").glob("q0_*.py"),
             repository / "tools/ci/q0_ruff_reference_lock.json",
             repository / "tools/ci/test_q0_16_ruff_reference.py",
             repository / "crates/mamba-quality/src/infrastructure/ruff/tests/interactions.rs",
             repository / "crates/mamba-quality/src/infrastructure/ruff/tests/reports.rs"]
    return {path.relative_to(repository).as_posix(): fact(path) for path in sorted(set(paths))}


def run(repository: Path, output: Path, pythons: dict[int, Path]) -> int:
    output.mkdir(parents=True, exist_ok=False)
    data, sources, targets, common_error, manifest = None, None, {}, None, None
    sources_unchanged, source_error = None, None
    try:
        host_guard()  # No download, prep or target work is admitted before this guard.
        if set(pythons) != set(VERSIONS):
            raise ValueError("all three exact interpreter paths must be supplied")
        manifest = source_manifest(repository)
        write_json(output / "source-before.json", manifest)
        corpus = (repository / CORPUS_PATH).read_bytes()
        lock_raw = (repository / "tools/ci/q0_ruff_reference_lock.json").read_bytes()
        profile_raw = (repository / "crates/mamba-quality/assets/profile.json").read_bytes()
        data, sources, lock = validate_inputs(corpus, lock_raw, profile_raw)
        cargo = tomllib.loads((repository / "Cargo.lock").read_text())
        matches = [row for row in cargo["package"] if row["name"] == "ruff_linter" and row["version"] == lock["ruff_version"]]
        if len(matches) != 1 or matches[0]["checksum"] != lock["fixtures"]["sha256"]:
            raise ValueError("current Cargo fixture package differs from the unchanged Ruff lock")
        (output / "frozen-interactions.json").write_bytes(corpus)
        (output / "frozen-lock.json").write_bytes(lock_raw)
        (output / "frozen-profile.json").write_bytes(profile_raw)
        write_sources(output / "inputs", {**sources, "pyproject.toml": CONFIG})
        write_json(output / "input-manifest.json", artifact_inventory(output / "inputs", []))
        write_json(output / "run-identity.json", {"source_sha": os.environ.get("GITHUB_SHA"),
            "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "runner_os": os.environ.get("RUNNER_OS"), "runner_arch": os.environ.get("RUNNER_ARCH"),
            "image_os": os.environ.get("ImageOS"), "corpus_sha256": digest(corpus), "lock_sha256": digest(lock_raw),
            "profile_sha256": digest(profile_raw), "SELECT": SELECT, "historical_environment": False})
        for name in ("sdist", "fixtures", "wheel"):
            download(lock[name], output / "downloads" / name)
        wheel = output / "downloads/wheel" / lock["wheel"]["filename"]
        for minor in VERSIONS:
            targets[minor] = run_target(pythons[minor], minor, output / f"python3{minor}", wheel, data, sources)
    except Exception as error:
        common_error = {"type": type(error).__name__, "message": str(error)}
    if manifest is not None:
        try:
            after = source_manifest(repository)
            write_json(output / "source-after.json", after)
            sources_unchanged = strict_json_bytes(after) == strict_json_bytes(manifest)
            if not sources_unchanged:
                raise ValueError("repository inputs or capture sources changed during execution")
        except Exception as error:
            source_error = {"type": type(error).__name__, "message": str(error)}
    index = []
    for minor in VERSIONS:
        target = targets.setdefault(minor, {"status": "not_run", "stages": {}, "reason": "common prerequisite failed"})
        before = target["stages"].get("before", {})
        diagnostics = before.get("diagnostics")
        rows = rule_rows(diagnostics, data["rules"], minor) if diagnostics is not None and data else [
            {"rule": rule, "python_version": VERSIONS[minor], "python_target": f"3.{minor}",
             "roles": None, "role_observations_passed": False, "issue_accepted": False, "native_parity": "not_run"} for rule in RULES]
        for row in rows:
            row.update(status="not_run" if diagnostics is None and not before.get("process_launched") else
                "passed" if target["status"] == "passed" and row["role_observations_passed"]
                and common_error is None and source_error is None and sources_unchanged is True else "failed",
                project_status=target["status"], project_before_exit=(before.get("receipt") or {}).get("actual_exit"),
                role_observation_status="projected" if diagnostics is not None else "not_projected",
                before_result_pointer=f"python3{minor}/before-result.json" if (output / f"python3{minor}/before-result.json").is_file() else None,
                target_result_pointer=f"python3{minor}/target-result.json" if (output / f"python3{minor}/target-result.json").is_file() else None)
        index.extend(rows)
    receipts = [row.get("capture_process_fact") or row.get("receipt")
                for target in targets.values() for row in target["stages"].values()]
    terminal = sum(type(row) is dict and row.get("wait_completed") is True and type(row.get("pid")) is int
        and row["pid"] > 0 and row.get("waited_pid") == row["pid"] and type(row.get("actual_exit")) is int for row in receipts)
    passed = common_error is None and source_error is None and sources_unchanged is True and len(index) == 42 and terminal == 12 and all(row["status"] == "passed" for row in index)
    write_json(output / "rule-target-index.json", index)
    summary = {"schema": "mamba.q0.16.ruff-interaction-reference.v1", "comparison_passed": passed,
        "planned_source_files_per_project": 29, "planned_rule_target_rows": 42, "planned_role_cells": 84,
        "project_commands_per_target": 4, "planned_project_commands": 12, "terminal_project_commands": terminal,
        "targets": {VERSIONS[minor]: row for minor, row in targets.items()}, "common_error": common_error,
        "repository_sources_unchanged": sources_unchanged, "source_preservation_error": source_error,
        "issue_accepted": False, "native_parity": "not_run", "whole_ruff_compatibility_claimed": False,
        "venv_exclusions": [f"python3{minor}/venv" for minor in VERSIONS]}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", artifact_inventory(output, [output / f"python3{minor}/venv" for minor in VERSIONS]))
    print(json.dumps({key: summary[key] for key in ("comparison_passed", "planned_rule_target_rows", "terminal_project_commands", "issue_accepted")}))
    return 0 if passed else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    for minor in VERSIONS:
        parser.add_argument(f"--python3{minor}", type=Path, required=True)
    args = parser.parse_args()
    return run(args.repository.resolve(strict=True), args.output.resolve(),
               {minor: getattr(args, f"python3{minor}") for minor in VERSIONS})


if __name__ == "__main__":
    raise SystemExit(main())
