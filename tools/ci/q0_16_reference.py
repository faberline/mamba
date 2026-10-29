#!/usr/bin/env python3
"""Capture new Q0 supplemental CLI observations on a pinned hosted Mac only.

No Mamba or case-source program is executed. A successful lane establishes
only the authored observation predicates, not issue or native acceptance.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import stat
import sys

from q0_16_packet import (CONFIG_BYTES, TOOLS, digest, evaluate_predicates,
                         load_packets, materialize, safe_path)
from q0_mypy_inputs import parse_diagnostics
from q0_mypy_reference import require_receipt
from q0_pydantic_inputs import CONFIG
import q0_pydantic_reference as pydantic_reference
from q0_pylint_inputs import project_diagnostics
import q0_pylint_reference as pylint_reference
from q0_ruff_reference import checked_bytes, download, run_command, write_json

VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
TIMEOUT = 120
SEVERITY_BITS = {"fatal": 1, "error": 2, "warning": 4, "refactor": 8,
                 "convention": 16, "info": 0}
CODE_SEVERITIES = {"F": "fatal", "E": "error", "W": "warning", "R": "refactor",
                   "C": "convention", "I": "info"}


def fact(path: Path) -> dict:
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError("captured input must remain a regular file")
    raw = path.read_bytes()
    after = path.lstat()
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
            after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns):
        raise ValueError("file changed during its capture")
    return {"bytes": len(raw), "sha256": digest(raw)}


def source_manifest(repository: Path) -> dict:
    paths = {*repository.joinpath("tools/ci").glob("q0_*.py"),
             *repository.joinpath("tools/ci").glob("q0_*_lock.json"),
             *repository.joinpath("tools/ci/q0_16_cases").glob("*.json"),
             *repository.joinpath("tools/ci").glob("test_q0_16_*.py"),
             repository / "crates/mamba-quality/assets/profile.json",
             repository / ".github/workflows/q0-16-reference-coverage.yml"}
    return {path.relative_to(repository).as_posix(): fact(path) for path in sorted(paths)}


def artifact_inventory(root: Path, excluded_venvs: list[Path]) -> dict:
    """Exclude only the two declared venv trees, never arbitrary 'venv' inputs."""
    excluded = {path.absolute() for path in excluded_venvs}
    rows = {}
    for directory, names, filenames in os.walk(root, topdown=True, followlinks=False):
        parent = Path(directory)
        kept = []
        for name in sorted(names):
            path = parent / name
            if path.absolute() in excluded:
                continue
            if path.is_symlink():
                target = os.readlink(path).encode()
                rows[path.relative_to(root).as_posix()] = {"kind": "symlink", "target": os.readlink(path),
                                                          "bytes": len(target), "sha256": digest(target)}
            else:
                kept.append(name)
        names[:] = kept
        for name in sorted(filenames):
            path = parent / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                target = os.readlink(path).encode()
                rows[relative] = {"kind": "symlink", "target": os.readlink(path),
                                  "bytes": len(target), "sha256": digest(target)}
            else:
                rows[relative] = {"kind": "regular", **fact(path)}
    return dict(sorted(rows.items()))


def ordered_mypy_diagnostics(stdout: bytes, filenames: set[str], project: Path) -> list[dict]:
    # The unchanged projection groups by filename. Project each raw JSON line
    # separately so this supplemental index also retains global stream order.
    rows = []
    for line in stdout.splitlines():
        grouped = parse_diagnostics(line + b"\n", filenames, project)
        projected = [item for items in grouped.values() for item in items]
        if len(projected) != 1:
            raise ValueError("one mypy JSON line must project to exactly one diagnostic")
        rows.extend(projected)
    return rows


def expected_exit(tool: str, diagnostics: list[dict]) -> int:
    if tool in {"mypy", "pydantic"}:
        if any(item.get("severity") not in {"error", "note", "warning"} for item in diagnostics):
            raise ValueError("unknown mypy diagnostic severity")
        return int(any(item["severity"] == "error" for item in diagnostics))
    result = 0
    for item in diagnostics:
        code, severity = item.get("rule"), item.get("severity")
        if (type(code) is not str or not code or severity not in SEVERITY_BITS
                or CODE_SEVERITIES.get(code[0]) != severity):
            raise ValueError("Pylint diagnostic code and severity disagree")
        result |= SEVERITY_BITS[severity]
    return result


def argv_for(executable: Path, tool: str, case: dict, project: Path,
             minor: int, strict_flags: list[str]) -> list[str]:
    if minor not in VERSIONS:
        raise ValueError("unknown exact Python target")
    if tool == "pylint":
        argv = pylint_reference.reference_argv(executable, case["entrypoints"], minor)
        argv.insert(2, "-B")
        if case.get("disabled_rules"):
            argv.insert(len(argv) - len(case["entrypoints"]),
                        "--disable=" + ",".join(case["disabled_rules"]))
        return argv
    return [str(executable), "-I", "-B", "-m", "mypy", "--strict", *strict_flags,
            "--config-file", str(project / "mypy.ini"), "--python-version", f"3.{minor}",
            "--cache-dir=/dev/null", "--no-incremental", "--output=json", *case["entrypoints"]]


@contextmanager
def capture_environment(tool: str, cache: Path):
    previous = os.environ.get("PYLINTHOME")
    if tool == "pylint":
        cache.mkdir(parents=True, exist_ok=False)
        os.environ["PYLINTHOME"] = str(cache)
    try:
        yield
    finally:
        if tool == "pylint":
            if previous is None:
                os.environ.pop("PYLINTHOME", None)
            else:
                os.environ["PYLINTHOME"] = previous


def observe_sources(project: Path, declared: dict) -> dict:
    actual = {}
    for name in declared:
        try:
            actual[name] = fact(project / name)
        except Exception as error:
            actual[name] = {"observation_error": type(error).__name__, "message": str(error)}
    return actual


def require_source_preservation(project: Path, declared: dict, missing: list[str], actual: dict) -> None:
    if actual != declared or any((project / name).exists() or (project / name).is_symlink() for name in missing):
        raise ValueError("source bytes or intentional missing entrypoint changed")


def run_case(tool: str, case: dict, proof: dict, packet_source: dict,
             executable: Path, environment: dict, root: Path, minor: int,
             strict_flags: list[str]) -> dict:
    root.mkdir(parents=True, exist_ok=False)
    project, capture = root / "work", root / "command"
    row = {"schema": "mamba.q0.16.supplement-observation.v1", "tool": tool,
           "id": case["id"], "behavior": case["behavior"], "role": case["role"], "group": case["group"],
           "python_minor": minor, "python_version": VERSIONS[minor], "python_target": f"3.{minor}",
           "input_sha256": proof["input_sha256"], "packet_source": packet_source,
           "environment": environment, "requires": case["requires"], "status": "failed",
           "process_launched": False, "observation_status": "not_run", "observation_complete": False, "predicate_results": [],
           "receipt": None, "diagnostics": None, "errors": [], "issue_accepted": False,
           "native_parity": "not_run", "historical_evidence_recovered": False}
    before = None
    try:
        before = materialize(case, tool, project)
        row["input_proof"] = before
        write_json(root / "input-proof.json", before)
        write_json(root / "source-before.json", before["files"])
        argv = argv_for(executable, tool, case, project, minor, strict_flags)
        row["argv"] = argv
        cache = root / "pylint-cache"
        with capture_environment(tool, cache):
            write_json(root / "capture-environment.json", {
                "stdin": "DEVNULL", "PYLINTHOME": os.environ.get("PYLINTHOME"),
                "environment_overlay": {"PYTHONDONTWRITEBYTECODE": "1", "PYTHONNOUSERSITE": "1",
                    "RUFF_NO_CACHE": "1", "PIP_CONFIG_FILE": os.devnull, "PIP_DISABLE_PIP_VERSION_CHECK": "1"},
                "python_isolated": True, "python_dont_write_bytecode": True,
                "mypy_cache": os.devnull if tool != "pylint" else None})
            exits = tuple(range(32)) if tool == "pylint" else (0, 1)
            receipt = run_command(argv, project, capture, expected_exits=exits, timeout=TIMEOUT,
                                  inputs={"case": case, "input_proof": before, "packet_source": packet_source,
                                          "input_sha256": proof["input_sha256"], "environment": environment})
        row["receipt"] = receipt
        row["process_launched"] = type(receipt.get("pid")) is int and receipt["pid"] > 0
        row["observation_status"] = "observed" if row["process_launched"] else "launch_failed"
        require_receipt(receipt, exits)
        stdout, stderr = (capture / "stdout.raw").read_bytes(), (capture / "stderr.raw").read_bytes()
        if digest(stdout) != receipt["stdout_sha256"] or digest(stderr) != receipt["stderr_sha256"]:
            raise ValueError("captured raw hashes differ from the actual process receipt")
        # These CLI contracts produce JSON diagnostics on stdout. No stderr
        # allowlist or old whole-list variant table is used for new cases.
        if stderr:
            raise ValueError("new reference CLI produced stderr; retain and review before acceptance")
        diagnostics = (project_diagnostics(stdout, project) if tool == "pylint" else
                       ordered_mypy_diagnostics(stdout, set(case["files"]), project))
        row["diagnostics"] = diagnostics
        row["diagnostic_count"] = len(diagnostics)
        row["expected_exit_from_observed_diagnostics"] = expected_exit(tool, diagnostics)
        if receipt["actual_exit"] != row["expected_exit_from_observed_diagnostics"]:
            raise ValueError("actual exit does not match the full observed diagnostic severities")
        row["observation_complete"] = True
        row["predicate_results"] = evaluate_predicates(diagnostics, case["requires"])
        absence_only = tool in {"mypy", "pydantic"} and all(
            item["presence"] == "absent" for item in case["requires"])
        row["clean_absence_control"] = {"required": absence_only,
            "passed": not absence_only or (receipt["actual_exit"] == 0
                and not any(item["severity"] == "error" for item in diagnostics))}
        if not row["clean_absence_control"]["passed"]:
            raise ValueError("absence-only mypy control requires exit 0 and no unrelated error diagnostics")
        if not all(item["passed"] for item in row["predicate_results"]):
            raise ValueError("one or more intended observation predicates were not met")
        row["status"] = "passed"
    except Exception as error:
        row["errors"].append({"stage": "reference_observation", "error_class": type(error).__name__, "message": str(error)})
    if before is not None:
        try:
            after = observe_sources(project, before["files"])
            write_json(root / "source-after.json", after)
            require_source_preservation(project, before["files"], before["missing_entrypoints"], after)
            row["source_files_unchanged"] = True
        except Exception as error:
            row["source_files_unchanged"] = False
            row["status"] = "failed"
            row["errors"].append({"stage": "source_preservation", "error_class": type(error).__name__, "message": str(error)})
        try:
            generated = artifact_inventory(project, [])
            row["generated_project_entries"] = {name: item for name, item in generated.items() if name not in before["files"]}
            write_json(root / "generated-project-entries.json", row["generated_project_entries"])
        except Exception as error:
            row["status"] = "failed"
            row["errors"].append({"stage": "generated_project_inventory", "error_class": type(error).__name__, "message": str(error)})
    row["command_pointer"] = "command/receipt.json" if row["receipt"] is not None else None
    row["stdout_pointer"] = "command/stdout.raw" if row["receipt"] is not None else None
    row["stderr_pointer"] = "command/stderr.raw" if row["receipt"] is not None else None
    write_json(root / "result.json", row)
    return row


def bind_control_pairs(rows: list[dict], packets: list[dict], output: Path) -> None:
    """A silent control cannot pass if its fresh same-input hit did not pass."""
    indexed = {(row["tool"], row["id"]): row for row in rows}
    for item in packets:
        if item["packet"]["tool"] != "pylint":
            continue
        cases = {case["id"]: case for case in item["packet"]["cases"]}
        for control_id, hit_id in item["proof"]["disabled_rule_pairs"].items():
            control = indexed.get(("pylint", control_id))
            if control is None:
                continue  # The control is scoped to another Python target.
            baseline = indexed.get(("pylint", hit_id))
            argument = "--disable=" + cases[control_id]["behavior"]
            control_args = list(control.get("argv", []))
            exact_arguments = control_args.count(argument) == 1
            if exact_arguments:
                control_args.remove(argument)
                exact_arguments = control_args == (baseline or {}).get("argv")
            control_input, hit_input = control.get("input_proof"), (baseline or {}).get("input_proof")
            checks = {"baseline_observed_and_passed": baseline is not None and baseline["status"] == "passed",
                      "control_observed_and_passed": control["status"] == "passed",
                      "identical_materialized_inputs": control_input is not None and control_input == hit_input,
                      "only_declared_cli_disable_differs": exact_arguments,
                      "both_sources_unchanged": control.get("source_files_unchanged") is True
                          and (baseline or {}).get("source_files_unchanged") is True}
            pair = {"baseline_id": hit_id, "control_id": control_id, "disabled_rule": cases[control_id]["behavior"],
                    "checks": checks, "paired_control_passed": all(checks.values()),
                    "other_diagnostic_invariance_claimed": False, "native_parity": "not_run"}
            control["case_predicates_status"] = control["status"]
            control["paired_control"] = pair
            if not pair["paired_control_passed"]:
                if control["status"] != "not_run":
                    control["status"] = "failed"
                control["errors"].append({"stage": "paired_rule_control", "message": "fresh same-input hit/control proof is incomplete"})
            folder = output / "cases/pylint" / control_id
            folder.mkdir(parents=True, exist_ok=True)
            write_json(folder / "pair-result.json", pair)


def write_final_results(rows: list[dict], output: Path) -> None:
    """Preserve initial observations and publish one authoritative final row."""
    if len({(row["tool"], row["id"]) for row in rows}) != len(rows):
        raise ValueError("final result case identities are duplicated")
    for row in rows:
        if row["tool"] not in TOOLS or safe_path(row["id"]) != Path(row["id"]).name:
            raise ValueError("unsafe final result identity")
        folder = output / "cases" / row["tool"] / row["id"]
        if folder.is_symlink() or any(parent.is_symlink() for parent in folder.parents):
            raise ValueError("final result directory traverses a symlink")
        folder.mkdir(parents=True, exist_ok=True)
        initial = folder / "result.json"
        if initial.exists() or initial.is_symlink():
            initial_fact = fact(initial)
            initial_row = json.loads(initial.read_bytes())
            if initial_row.get("tool") != row["tool"] or initial_row.get("id") != row["id"]:
                raise ValueError("initial observation names a different case")
            row["initial_result_pointer"] = initial.relative_to(output).as_posix()
            row["initial_result_sha256"] = initial_fact["sha256"]
        else:
            row["initial_result_pointer"] = None
            row["initial_result_sha256"] = None
        final = folder / "final-result.json"
        row["final_result_pointer"] = final.relative_to(output).as_posix()
        row["result_pointer"] = row["final_result_pointer"]
        write_json(final, row)


def publish_case_results(rows: list[dict], packets: list[dict], output: Path) -> None:
    """Bind pairs before writing final files and both complete indexes."""
    bind_control_pairs(rows, packets, output)
    write_final_results(rows, output)
    write_json(output / "case-results.json", rows)
    with (output / "raw-index.jsonl").open("x") as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    if (json.loads((output / "case-results.json").read_bytes()) != rows
            or [json.loads(line) for line in (output / "raw-index.jsonl").read_bytes().splitlines()] != rows
            or any(json.loads((output / row["final_result_pointer"]).read_bytes()) != row for row in rows)):
        raise ValueError("final result pointers and complete indexes disagree")


def environment_payloads(root: Path, identity: dict, tool: str) -> dict:
    venv = (root / "venv").resolve(strict=True)
    site = (Path(identity["modules"]["pylint"]["path"]).parent.parent if tool == "pylint" else None)
    rows = {}
    for distribution in identity["distributions"].values():
        base = Path(distribution["metadata_path"]).parent if "metadata_path" in distribution else site
        if base is None:
            raise ValueError("distribution site-package root is unavailable")
        files = distribution["distribution_files"]
        if not files:
            raise ValueError("distribution payload denominator is empty")
        for relative, expected in files.items():
            path = (base / relative).resolve(strict=True)
            if not path.is_relative_to(venv) or str(path) in rows or fact(path) != expected:
                raise ValueError("installed package payload is outside its venv, duplicated or changed")
            rows[str(path)] = expected
    if not rows:
        raise ValueError("environment payload denominator is empty")
    return rows


def capture_installed_records(root: Path, identity: dict, tool: str, phase: str) -> dict:
    """Retain generated installed RECORD bytes outside the excluded venv."""
    if phase not in {"before", "after"}:
        raise ValueError("unknown installed-record capture phase")
    destination = root / ("installed-records-" + phase)
    destination.mkdir(parents=True, exist_ok=False)
    venv = (root / "venv").resolve(strict=True)
    site = Path(identity["modules"]["pylint"]["path"]).parent.parent if tool == "pylint" else None
    rows = []
    for name, distribution in identity["distributions"].items():
        row = {"distribution": name, "matches_original_package_probe": False}
        try:
            if type(name) is not str or Path(name).name != name or name in {".", ".."}:
                raise ValueError("unsafe installed distribution name")
            base = Path(distribution["metadata_path"]).parent if "metadata_path" in distribution else site
            records = [(relative, expected) for relative, expected in distribution["distribution_files"].items()
                       if relative.endswith(".dist-info/RECORD")]
            if len(records) != 1 or base is None:
                raise ValueError("every pinned distribution must have exactly one installed RECORD")
            relative, expected = records[0]
            source = (base / relative).resolve(strict=True)
            if not source.is_relative_to(venv):
                raise ValueError("installed RECORD escaped its prepared venv")
            observed = fact(source)
            raw = source.read_bytes()
            if {"bytes": len(raw), "sha256": digest(raw)} != observed:
                raise ValueError("installed RECORD changed during byte capture")
            target = destination / (name + ".RECORD")
            with target.open("xb") as stream:
                stream.write(raw)
            if fact(target) != observed:
                raise ValueError("installed RECORD copy differs")
            row.update(relative_path=relative, original_package_probe=expected, observed=observed,
                       copy=target.relative_to(root).as_posix(), matches_original_package_probe=observed == expected)
        except Exception as error:
            row["error"] = {"error_class": type(error).__name__, "message": str(error)}
        rows.append(row)
    report = {"phase": phase, "distribution_count": len(identity["distributions"]),
              "copied_record_count": sum("copy" in row for row in rows), "records": rows,
              "all_sources_match": bool(rows and all(row["matches_original_package_probe"] for row in rows))}
    write_json(root / ("installed-records-" + phase + ".json"), report)
    return report


def prepare_environment(repository: Path, python: Path, version: str, output: Path,
                        tool: str, profile: dict, locks: dict) -> tuple[Path, dict]:
    minor = int(version.split(".")[1])
    environment_root = output / "environments" / tool
    if tool == "mypy":
        lock = locks["pydantic"]
        pins = lock["dependencies"][version]
    else:
        lock = locks["pylint"]
        pins = [{"name": name, "version": item["version"], **item["wheels"][str(minor)]}
                for name, item in lock["dependencies"].items()]
    wheels = []
    for index, pin in enumerate(pins):
        folder = output / "downloads" / tool / f"wheel-{index:02}"
        download(pin, folder)
        wheels.append(folder / pin["filename"])
    if tool == "mypy":
        executable = pydantic_reference.prepare(python, version, environment_root, pins, wheels,
                                                lock["interpreters"][version], profile["mypy"]["strict_flags"])
    else:
        executable = pylint_reference.prepare(python, version, environment_root, pins, wheels, lock["rules"])
    identity_path = environment_root / "identity.json"
    identity = json.loads(identity_path.read_bytes())
    payloads = environment_payloads(environment_root, identity, tool)
    write_json(environment_root / "payloads-before.json", payloads)
    records = capture_installed_records(environment_root, identity, tool, "before")
    if not records["all_sources_match"]:
        raise ValueError("installed RECORD byte custody is incomplete before reference execution")
    return executable, {"environment_kind": tool, "identity_path": str(identity_path.relative_to(output)),
                        "identity_sha256": digest(identity_path.read_bytes()),
                        "executable": str(executable), "executable_sha256": digest(executable.read_bytes()),
                        "base_executable_sha256": digest(python.resolve(strict=True).read_bytes()),
                        "pinned_distribution_count": len(pins), "payload_file_count": len(payloads),
                        "installed_record_count": records["copied_record_count"]}


def not_run_row(tool: str, case: dict, proof: dict, source: dict, minor: int, reason: str) -> dict:
    return {"schema": "mamba.q0.16.supplement-observation.v1", "tool": tool,
            "id": case["id"], "behavior": case["behavior"], "role": case["role"], "group": case["group"],
            "python_minor": minor, "python_version": VERSIONS[minor], "python_target": f"3.{minor}",
            "input_sha256": proof["input_sha256"], "packet_source": source, "status": "not_run",
            "requires": case["requires"], "predicate_results": [], "process_launched": False,
            "observation_status": "not_run", "observation_complete": False, "receipt": None, "diagnostics": None,
            "errors": [{"stage": "environment_preparation", "message": reason}],
            "issue_accepted": False, "native_parity": "not_run", "historical_evidence_recovered": False}


def run(repository: Path, output: Path, python: Path, version: str) -> int:
    output.mkdir(parents=True, exist_ok=False)
    minor = int(version.split(".")[1])
    packets, planned, rows, environment_states, common_error = [], [], [], {}, None
    before_sources = None
    try:
        pylint_reference.require_host_context(pylint_reference.HOST_CONTEXT)
        if os.environ.get("MYPYPATH"):
            raise ValueError("ambient MYPYPATH is outside the declared reference environment")
        if version != VERSIONS[minor] or not python.is_absolute():
            raise ValueError("exact absolute base interpreter is required")
        if CONFIG.encode() != CONFIG_BYTES:
            raise ValueError("new CLI configuration differs from the pinned plugin configuration")
        profile_path = repository / "crates/mamba-quality/assets/profile.json"
        profile = json.loads(profile_path.read_bytes())
        locks = {name: json.loads((repository / f"tools/ci/q0_{name}_reference_lock.json").read_bytes())
                 for name in ("mypy", "pylint", "pydantic")}
        pydantic_reference.validate_lock(locks["pydantic"])
        pylint_reference.require_host_context(locks["pylint"]["platform"])
        for lock in locks.values():
            checked_bytes(profile_path.read_bytes(), lock["profile_sha256"])
        checked_bytes(python.resolve(strict=True).read_bytes(),
                      locks["pydantic"]["interpreters"][version]["executable_sha256"])
        for relative, expected in locks["pydantic"]["source_files"].items():
            checked_bytes((repository / safe_path(relative)).read_bytes(), expected)
        for relative, expected in locks["pylint"]["comparator_sources"].items():
            checked_bytes((repository / safe_path(relative)).read_bytes(), expected)
        before_sources = source_manifest(repository)
        write_json(output / "source-before.json", before_sources)
        packets = load_packets(repository, profile)
        if {item["packet"]["tool"] for item in packets} != TOOLS:
            raise ValueError("all three supplemental tool packets must be present")
        for item in packets:
            tool, packet = item["packet"]["tool"], item["packet"]
            packet_root = output / "packets"
            packet_root.mkdir(exist_ok=True)
            with (packet_root / f"{tool}.json").open("xb") as stream:
                stream.write((repository / item["source"]["path"]).read_bytes())
            write_json(packet_root / f"{tool}-proof.json", item["proof"])
            for case, proof in zip(packet["cases"], item["proof"]["cases"]):
                if f"3.{minor}" in case["python_targets"]:
                    planned.append((tool, case, proof, item["source"]))
        if not planned:
            raise ValueError("zero supplemental target cells is not a capture pass")
        write_json(output / "run-identity.json", {
            "schema": "mamba.q0.16.supplement-run-identity.v1", "source_sha": os.environ.get("GITHUB_SHA"),
            "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "host_context": pylint_reference.HOST_CONTEXT, "python_version": version,
            "controller_python": sys.version, "controller_executable": sys.executable,
            "profile": fact(profile_path), "planned_target_cells": len(planned),
            "source_files": before_sources, "packet_sources": [item["source"] for item in packets],
            "case_sources_executed": False, "native_parity": "not_run", "issue_accepted": False})
        for tool in ("mypy", "pylint", "pydantic"):
            download(locks[tool]["sdist"], output / "downloads" / "sdist" / tool)
        for tool in ("mypy", "pylint"):
            try:
                executable, identity = prepare_environment(repository, python, version, output, tool, profile, locks)
                environment_states[tool] = {"prepared": True, "executable": str(executable), "identity": identity}
            except Exception as error:
                environment_states[tool] = {"prepared": False, "error_class": type(error).__name__, "message": str(error)}
            write_json(output / f"{tool}-environment.json", environment_states[tool])
        for tool, case, proof, source in planned:
            lane = "pylint" if tool == "pylint" else "mypy"
            environment = environment_states[lane]
            if environment["prepared"]:
                try:
                    row = run_case(tool, case, proof, source, Path(environment["executable"]), environment["identity"],
                                   output / "cases" / tool / case["id"], minor, profile["mypy"]["strict_flags"])
                except Exception as error:
                    row = not_run_row(tool, case, proof, source, minor, type(error).__name__ + ": " + str(error))
                    row["errors"][0]["stage"] = "case_controller"
                    receipt_path = output / "cases" / tool / case["id"] / "command/receipt.json"
                    if receipt_path.is_file() and not receipt_path.is_symlink():
                        row["receipt"] = json.loads(receipt_path.read_bytes())
                        row["process_launched"] = type(row["receipt"].get("pid")) is int and row["receipt"]["pid"] > 0
                        row["observation_status"] = "observed" if row["process_launched"] else "launch_failed"
                        if row["process_launched"]:
                            row["status"] = "failed"
                        row["command_pointer"] = "command/receipt.json"
                        row["stdout_pointer"] = "command/stdout.raw"
                        row["stderr_pointer"] = "command/stderr.raw"
            else:
                row = not_run_row(tool, case, proof, source, minor,
                                  environment["error_class"] + ": " + environment["message"])
            rows.append(row)
        for tool, state in environment_states.items():
            if state["prepared"]:
                try:
                    root = output / "environments" / tool
                    identity = json.loads((root / "identity.json").read_bytes())
                    records = capture_installed_records(root, identity, tool, "after")
                    state["installed_records_unchanged"] = records["all_sources_match"]
                    after = environment_payloads(root, identity, tool)
                    before = json.loads((root / "payloads-before.json").read_bytes())
                    write_json(root / "payloads-after.json", after)
                    state["payload_files_unchanged"] = after == before
                    state["base_executable_unchanged"] = digest(python.resolve(strict=True).read_bytes()) == state["identity"]["base_executable_sha256"]
                    if (not state["payload_files_unchanged"] or not state["base_executable_unchanged"]
                            or not state["installed_records_unchanged"]):
                        raise ValueError("prepared reference environment changed during supplemental capture")
                except Exception as error:
                    state["payload_files_unchanged"] = False
                    state["post_capture_error"] = {"error_class": type(error).__name__, "message": str(error)}
        after_sources = source_manifest(repository)
        write_json(output / "source-after.json", after_sources)
        if before_sources != after_sources:
            raise ValueError("tracked supplemental inputs or capture sources changed")
    except Exception as error:
        common_error = {"error_class": type(error).__name__, "message": str(error)}
    existing = {(row["tool"], row["id"]) for row in rows}
    for tool, case, proof, source in planned:
        if (tool, case["id"]) not in existing:
            rows.append(not_run_row(tool, case, proof, source, minor, "common controller failure before this case"))
    try:
        publish_case_results(rows, packets, output)
    except Exception as error:
        publication_error = {"stage": "final_result_publication", "error_class": type(error).__name__, "message": str(error)}
        if common_error is None:
            common_error = publication_error
        else:
            common_error["final_result_publication_error"] = publication_error
    passed = bool(planned and common_error is None and len(rows) == len(planned)
                  and all(row["status"] == "passed" for row in rows)
                  and all(state.get("prepared") and state.get("payload_files_unchanged")
                          and state.get("base_executable_unchanged") and state.get("installed_records_unchanged")
                          for state in environment_states.values())
                  and set(environment_states) == {"mypy", "pylint"})
    summary = {"schema": "mamba.q0.16.supplement-lane-summary.v1", "issue": 16,
               "python_version": version, "python_minor": minor, "planned_case_count": len(planned),
               "result_count": len(rows), "passed_case_count": sum(row["status"] == "passed" for row in rows),
               "failed_case_ids": [row["id"] for row in rows if row["status"] == "failed"],
               "not_run_case_ids": [row["id"] for row in rows if row["status"] == "not_run"],
               "actual_process_count": sum(row.get("process_launched") is True for row in rows),
               "waited_process_count": sum((row.get("receipt") or {}).get("wait_completed") is True for row in rows),
               "packet_count": len(packets), "environments": environment_states, "common_error": common_error,
               "supplement_capture_passed": passed, "issue_accepted": False, "native_parity": "not_run",
               "full_semantic_map_accepted": False, "historical_evidence_recovered": False,
               "excluded_venv_trees": ["environments/mypy/venv", "environments/pylint/venv"]}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", artifact_inventory(output, [output / "environments" / tool / "venv" for tool in ("mypy", "pylint")]))
    print(json.dumps({"supplement_capture_passed": passed, "planned_case_count": len(planned), "issue_accepted": False}))
    return 0 if passed else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--python-version", choices=VERSIONS.values(), required=True)
    args = parser.parse_args()
    if not all(path.is_absolute() for path in (args.repository, args.output, args.python)):
        parser.error("repository, output and base interpreter must be absolute paths")
    if args.output.exists() or args.output.is_symlink():
        parser.error("output must be a fresh path")
    return run(args.repository.resolve(strict=True), args.output.resolve(), args.python, args.python_version)


if __name__ == "__main__":
    raise SystemExit(main())
