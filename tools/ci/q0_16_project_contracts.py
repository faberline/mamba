#!/usr/bin/env python3
"""Capture the finite fixed named-project contracts on a pinned hosted Mac.

These are complete project observations, not authored semantic roles. Frozen
JSON diagnostics and whole-process exits are compared without severity rules.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import sys
import unicodedata

from q0_16_packet import CONFIG_BYTES, canonical, read_json, safe_path, source_facts
import q0_16_reference as capture
from q0_mypy_reference import require_receipt
from q0_ruff_reference import checked_bytes, digest, download, run_command, write_json, write_sources

BASE = "crates/mamba-quality/src/infrastructure/mypy/tests"
LOCK_PATH = "tools/ci/q0_16_project_contracts_lock.json"
LOCK_SHA = "1131897c100bb5766d267be8441fc48ecdcb8d800b758763785720ec85294fd9"
VERSIONS = capture.VERSIONS
FAMILIES = {"mypy_projects": 318, "pydantic_config": 257}
PYPROJECT = b'[project]\nname = "fixture"\nversion = "0.0.0"\nrequires-python = ">=3.12"\n'
SELECTED_NAMES = {"namespace_model_parent", "nested_namespace_model", "regular_package_inside_namespace",
    "namespace_regular_relative_import", "same_named_namespace_models", "namespace_stub_model", "namespace_package_reexport"}
PROTECTED = {"mypy", "pydantic", "pydantic_core", "typing_extensions", "pathspec", "librt",
             "ast_serialize", "mypy_extensions", "annotated_types", "typing_inspection"}
VALUE_RECIPE = 'json.dumps(selected_value, ensure_ascii=True, separators=(",", ":")); original object key order'


def value_digest(value: object) -> str:
    return digest(json.dumps(value, ensure_ascii=True, separators=(",", ":"), allow_nan=False).encode())


def pointer(*parts) -> str:
    return "".join("/" + str(part).replace("~", "~0").replace("/", "~1") for part in parts)


def binding(path: str, parts: tuple, value: object) -> dict:
    return {"path": path, "constant": "CASES" if path.endswith(".rs") else None,
            "pointer": pointer(*parts), "value_sha256": value_digest(value)}


def source_json(repository: Path, relative: str) -> object:
    path = repository / safe_path(relative)
    capture.fact(path)
    raw = path.read_bytes()
    if relative.endswith(".rs"):
        matches = list(re.finditer(rb'pub const CASES: &str = r(?P<h>#+)"(?P<body>.*?)"(?P=h);', raw, re.S))
        if len(matches) != 1:
            raise ValueError("exactly one fixed Rust CASES JSON constant is required")
        raw = matches[0]["body"]
    return read_json(raw)


def validate_case(row: dict, family: str, entries: dict) -> list[str]:
    if (type(row) is not dict or set(row) != {"name", "files", "source_sha256", "expected", "reference_exits"}
            or type(row["name"]) is not str or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", row["name"])
            or family not in FAMILIES or type(row["files"]) is not dict or not row["files"]):
        raise ValueError("named project lacks its fixed source/expected/exit contract")
    files = row["files"]
    paths = []
    for name, text in files.items():
        safe_path(name)
        head = PurePosixPath(name).parts[0].casefold()
        if (not name.endswith((".py", ".pyi")) or type(text) is not str
                or head.removesuffix(".py").removesuffix(".pyi") in PROTECTED
                or head in {"mypy.ini", "pyproject.toml"}):
            raise ValueError("project sources cannot replace configuration or prepared reference packages")
        text.encode("utf-8")
        paths.append(PurePosixPath(unicodedata.normalize("NFD", name).casefold()))
    if (len(set(paths)) != len(files)
            or any(parent in paths for path in paths for parent in path.parents)
            or {name: item["sha256"] for name, item in source_facts(files).items()} != row["source_sha256"]):
        raise ValueError("project source paths collide or their exact bytes differ")
    if (type(row["expected"]) is not list or len(row["expected"]) != 3
            or type(row["reference_exits"]) is not list or len(row["reference_exits"]) != 3
            or any(type(code) is not int or code not in (0, 1) for code in row["reference_exits"])):
        raise ValueError("all three ordered diagnostic and actual exit targets are required")
    for diagnostics in row["expected"]:
        if type(diagnostics) is not list:
            raise ValueError("expected project diagnostics must be complete ordered arrays")
        for item in diagnostics:
            if (type(item) is not dict or set(item) != {"rule", "message", "hint", "path", "severity", "location"}
                    or item["path"] not in files or item["severity"] not in {"error", "note", "warning"}
                    or type(item["message"]) is not str
                    or type(item["location"]) is not dict
                    or set(item["location"]) != {"line", "column", "end_line", "end_column"}):
                raise ValueError("expected project diagnostic shape or source boundary differs")
    selected = entries.get(row["name"]) if family == "pydantic_config" else None
    if selected is None:
        if family == "pydantic_config" and row["name"] in SELECTED_NAMES:
            raise ValueError("fixed main.py selection is missing")
        return ["."]
    if (row["name"] not in SELECTED_NAMES or set(selected) != {"entrypoints", "source_sha256"}
            or selected["entrypoints"] != ["main.py"] or "main.py" not in files
            or selected["source_sha256"] != row["source_sha256"]):
        raise ValueError("explicit main.py selection lacks its complete fixed source binding")
    return ["main.py"]


def read_cases(repository: Path) -> tuple[list[dict], list[str]]:
    catalog_path = BASE + "/projects/catalog.rs"
    catalog = (repository / catalog_path).read_text()
    modules = re.findall(r"^mod (part_[0-9]{3});$", catalog, re.M)
    if modules != re.findall(r"(part_[0-9]{3})::CASES", catalog) or len(modules) != 20 or len(set(modules)) != 20:
        raise ValueError("project catalog module order or denominator differs")
    entry_path = BASE + "/pydantic_config_entrypoints.json"
    entries = source_json(repository, entry_path)
    if type(entries) is not dict or set(entries) != SELECTED_NAMES:
        raise ValueError("exactly the seven fixed main.py project selections are required")
    origins = [("mypy_projects", BASE + "/projects/catalog/" + part + ".rs") for part in modules]
    origins.append(("pydantic_config", BASE + "/pydantic_config.json"))
    result, identities = [], set()
    for family, path in origins:
        rows = source_json(repository, path)
        if type(rows) is not list or not rows:
            raise ValueError("fixed source catalog must have a nonzero project array")
        for index, row in enumerate(rows):
            entrypoints = validate_case(row, family, entries)
            identity = (family, row["name"])
            if identity in identities:
                raise ValueError("duplicate family/project identity")
            identities.add(identity)
            result.append({"family": family, "name": row["name"], "id": digest(canonical(identity)),
                "files": row["files"], "expected": row["expected"], "reference_exits": row["reference_exits"],
                "entrypoints": entrypoints, "source_path": path, "source_index": index,
                "selection": binding(entry_path, (row["name"],), entries[row["name"]]) if entrypoints != ["."] else None})
    if ({family: sum(case["family"] == family for case in result) for family in FAMILIES} != FAMILIES
            or sum(len(case["files"]) for case in result) != 1069
            or sum(case["entrypoints"] == ["main.py"] for case in result) != 7):
        raise ValueError("fixed family, full source-file or entry-selection denominators differ")
    return result, [catalog_path, entry_path, *[path for _, path in origins]]


def build_lock(repository: Path) -> tuple[dict, list[dict]]:
    cases, data_paths = read_cases(repository)
    context_paths = [BASE + "/projects.rs", BASE + "/pydantic_config.rs",
        "crates/mamba-quality/assets/profile.json", "tools/ci/q0_mypy_reference_lock.json",
        "tools/ci/q0_pydantic_reference_lock.json", *["tools/ci/" + name for name in (
            "q0_16_reference.py", "q0_16_packet.py", "q0_mypy_inputs.py", "q0_mypy_reference.py",
            "q0_pydantic_reference.py", "q0_pydantic_inputs.py", "q0_pylint_reference.py", "q0_ruff_reference.py")]]
    facts = {path: capture.fact(repository / path) for path in sorted(set(data_paths + context_paths))}
    profile = source_json(repository, "crates/mamba-quality/assets/profile.json")
    strict = profile["mypy"]["strict_flags"]
    if (len(strict) != 13 or len(set(strict)) != 13 or any(type(flag) is not str or not flag.startswith("--") for flag in strict)
            or profile["pydantic"] != {"init_typed": True, "init_forbid_extra": True, "warn_required_dynamic_aliases": True}):
        raise ValueError("fixed strict or three-true plugin profile differs")
    rows = []
    for case in cases:
        path, index = case["source_path"], case["source_index"]
        semantic = {"family": case["family"], "name": case["name"], "source_files": source_facts(case["files"]),
                    "entrypoints": case["entrypoints"], "mypy_ini_sha256": digest(CONFIG_BYTES), "pyproject_sha256": digest(PYPROJECT)}
        row = {"id": case["id"], "family": case["family"], "name": case["name"],
            "source_files": semantic["source_files"], "input_sha256": digest(canonical(semantic)),
            "input_reference": binding(path, (index, "files"), case["files"]),
            "entrypoints": case["entrypoints"], "selection_reference": case["selection"],
            "targets": [{"python_version": version, "expected_reference": binding(path, (index, "expected", target), case["expected"][target]),
                         "exit_reference": binding(path, (index, "reference_exits", target), case["reference_exits"][target])}
                        for target, version in enumerate(VERSIONS.values())]}
        rows.append(row); case["binding"] = row
    lock = {"schema": "mamba.q0.16.named-project-contract-lock.v1", "python_versions": list(VERSIONS.values()),
        "case_counts": FAMILIES, "project_count": 575, "source_file_count": 1069, "target_cell_count": 1725,
        "directory_root_projects": 568, "explicit_main_projects": 7, "source_files": facts,
        "selected_value_hash_recipe": VALUE_RECIPE, "strict_flags": strict,
        "mypy_ini": {"utf8": CONFIG_BYTES.decode(), "sha256": digest(CONFIG_BYTES)},
        "pyproject": {"utf8": PYPROJECT.decode(), "sha256": digest(PYPROJECT)}, "cases": rows}
    return lock, cases


def frozen_lock(repository: Path) -> dict:
    capture.fact(repository / LOCK_PATH)
    raw = (repository / LOCK_PATH).read_bytes()
    return read_json(checked_bytes(raw, LOCK_SHA))


def load_inputs(repository: Path) -> tuple[dict, list[dict]]:
    frozen = frozen_lock(repository)
    actual, cases = build_lock(repository)
    if capture.strict_json_bytes(actual) != capture.strict_json_bytes(frozen):
        raise ValueError("fixed project lock, selected source/expected/exit bindings or owner sources drifted")
    return frozen, cases


def host_guard() -> None:
    capture.pylint_reference.require_host_context(capture.pylint_reference.HOST_CONTEXT)
    if platform.machine() != "arm64" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise ValueError("named-project reference capture requires the hosted Mac ARM64 job")


def argv_for(executable: Path, project: Path, entrypoints: list[str], minor: int, flags: list[str]) -> list[str]:
    if minor not in VERSIONS or entrypoints not in (["."], ["main.py"]) or len(flags) != 13:
        raise ValueError("named-project target, strict profile or source selector differs")
    return [str(executable), "-I", "-B", "-m", "mypy", "--strict", *flags,
        "--config-file", str(project / "mypy.ini"), "--python-version", f"3.{minor}",
        "--cache-dir=/dev/null", "--no-incremental", "--output=json", *entrypoints]


def base_row(case: dict, version: str) -> dict:
    target = list(VERSIONS.values()).index(version) if version in VERSIONS.values() else None
    return {"schema": "mamba.q0.16.named-project-observation.v1", "observation_kind": "named-project-reference-contract",
        "family": case["family"], "name": case["name"], "id": case["id"], "python_version": version,
        "project_lock_sha256": LOCK_SHA,
        "input_binding": case["binding"], "target_binding": case["binding"]["targets"][target] if target is not None else None,
        "entrypoints": case["entrypoints"], "status": "not_run", "process_launched": False,
        "capture_process_fact": None, "returned_receipt": None, "receipt": None, "receipt_reconciled": False,
        "diagnostics": None, "errors": [], "issue_accepted": False, "native_parity": "not_run", "som_accepted": False,
        "exit_scope": "one whole project process; no per-file exit", "source_programs_executed": False}


def run_case(case: dict, version: str, executable: Path, environment: dict, flags: list[str], root: Path) -> dict:
    row = base_row(case, version); row["status"] = "failed"; row["environment"] = environment
    root.mkdir(parents=True, exist_ok=False)
    project, command = root / "work", root / "command"
    expected_sources = {**case["files"], "mypy.ini": CONFIG_BYTES.decode(), "pyproject.toml": PYPROJECT.decode()}
    manifest = source_facts(expected_sources)
    try:
        write_sources(project, {name: text.encode() for name, text in expected_sources.items()})
        write_sources(root / "input-snapshot", {name: text.encode() for name, text in expected_sources.items()})
        write_json(root / "source-before.json", manifest)
        minor = int(version.split(".")[1]); target = minor - 12
        argv = argv_for(executable, project, case["entrypoints"], minor, flags); row["argv"] = argv
        inputs = {"input_binding": case["binding"], "target_binding": row["target_binding"],
                  "source_files": manifest, "environment": environment, "entrypoints": case["entrypoints"]}
        returned = run_command(argv, project, command, expected_exits=(0, 1), timeout=120, inputs=inputs)
        # Preserve known child facts before any stored-file or serialization check.
        row["capture_process_fact"] = {key: returned.get(key) for key in
            ("pid", "waited_pid", "wait_completed", "actual_exit", "timed_out", "launch_error")}
        row["process_launched"] = type(returned.get("pid")) is int and returned["pid"] > 0
        write_json(command / "returned-receipt.json", returned)
        row["returned_receipt"] = read_json((command / "returned-receipt.json").read_bytes())
        row["receipt"] = read_json((command / "receipt.json").read_bytes())
        receipt = capture.require_saved_receipt(returned, row["receipt"], (0, 1))
        if type(receipt.get("waited_pid")) is not int:
            raise ValueError("actual waited child identity must be an integer PID")
        require_receipt(receipt, (0, 1)); row["receipt_reconciled"] = True
        stdout, stderr = (command / "stdout.raw").read_bytes(), (command / "stderr.raw").read_bytes()
        if (digest(stdout) != receipt["stdout_sha256"] or digest(stderr) != receipt["stderr_sha256"]
                or digest((command / "inputs.json").read_bytes()) != receipt["inputs_sha256"]
                or capture.strict_json_bytes(read_json((command / "inputs.json").read_bytes())) != capture.strict_json_bytes(inputs)
                or receipt["argv"] != argv or receipt["cwd"] != str(project)):
            raise ValueError("complete raw/input/argv/cwd facts differ from the actual process receipt")
        if stderr:
            raise ValueError("named-project reference produced unexpected stderr")
        actual = capture.ordered_mypy_diagnostics(stdout, set(case["files"]), project)
        row["diagnostics"] = actual
        row["expected_actual_exit"] = case["reference_exits"][target]
        row["expected_diagnostic_count"] = len(case["expected"][target])
        row["diagnostics_match"] = capture.strict_json_bytes(actual) == capture.strict_json_bytes(case["expected"][target])
        row["actual_exit_matches"] = receipt["actual_exit"] == case["reference_exits"][target]
        if not row["diagnostics_match"] or not row["actual_exit_matches"]:
            raise ValueError("complete ordered diagnostics or direct frozen actual exit differ")
        row["status"] = "passed"
    except Exception as error:
        row["errors"].append({"stage": "project_observation", "type": type(error).__name__, "message": str(error)})
    try:
        after = capture.artifact_inventory(project, [])
        write_json(root / "source-after.json", after)
        row["source_files_unchanged"] = after == {name: {"kind": "regular", **item} for name, item in manifest.items()}
        if not row["source_files_unchanged"]:
            raise ValueError("full project source/config bytes or entries changed")
    except Exception as error:
        row["status"] = "failed"; row["source_files_unchanged"] = False
        row["errors"].append({"stage": "source_preservation", "type": type(error).__name__, "message": str(error)})
    row["raw_pointers"] = {name: "command/" + name for name in
        ("receipt.json", "returned-receipt.json", "inputs.json", "stdin.raw", "stdout.raw", "stderr.raw")
        if (command / name).is_file()}
    write_json(root / "observation.json", row)
    return row


def process_counts(rows: list[dict]) -> dict:
    facts = [row.get("capture_process_fact") or {} for row in rows]
    known = lambda row: type(row.get("pid")) is int and row["pid"] > 0
    return {"actual_project_processes": sum(known(row) for row in facts),
        "waited_project_processes": sum(known(row) and row.get("wait_completed") is True
            and type(row.get("waited_pid")) is int and row.get("waited_pid") == row["pid"]
            and type(row.get("actual_exit")) is int for row in facts),
        "reconciled_project_receipts": sum(row.get("receipt_reconciled") is True for row in rows)}


def execute_plan(planned: list[dict], version: str, executable: Path, identity: dict,
                 flags: list[str], output: Path) -> list[dict]:
    rows = []
    for case in planned:
        folder = output / "cases" / case["family"] / case["id"]
        try:
            row = run_case(case, version, executable, identity, flags, folder)
        except Exception as error:
            row = base_row(case, version); row["status"] = "failed"
            row["errors"].append({"stage": "case_controller", "type": type(error).__name__, "message": str(error)})
            for key, filename in (("returned_receipt", "returned-receipt.json"), ("receipt", "receipt.json")):
                try:
                    path = folder / "command" / filename
                    if path.is_file(): row[key] = read_json(path.read_bytes())
                except Exception as receipt_error:
                    row["errors"].append({"stage": "receipt_recovery", "type": type(receipt_error).__name__, "message": str(receipt_error)})
            known = row["returned_receipt"] or row["receipt"] or {}
            if type(known) is dict:
                row["capture_process_fact"] = {key: known.get(key) for key in
                    ("pid", "waited_pid", "wait_completed", "actual_exit", "timed_out", "launch_error")}
                row["process_launched"] = type(known.get("pid")) is int and known["pid"] > 0
        rows.append(row)
    return rows


def publish(rows: list[dict], planned: list[dict], output: Path, context_passed: bool) -> None:
    if (len(rows) != len(planned) or [(row["family"], row["id"]) for row in rows] !=
            [(case["family"], case["id"]) for case in planned] or len({row["id"] for row in rows}) != len(rows)):
        raise ValueError("complete named-project final result identities/order differ")
    for row in rows:
        if row["family"] not in FAMILIES or not re.fullmatch(r"[0-9a-f]{64}", row["id"]):
            raise ValueError("unsafe final project result identity")
        folder = output / "cases" / row["family"] / row["id"]
        folder.mkdir(parents=True, exist_ok=True)
        initial = folder / "observation.json"
        row["initial_observation_pointer"] = initial.relative_to(output).as_posix() if initial.is_file() else None
        row["initial_observation_sha256"] = capture.fact(initial)["sha256"] if initial.is_file() else None
        row["case_observation_status"] = row["status"]; row["capture_context_passed"] = context_passed
        if row["status"] == "passed" and not context_passed:
            row["status"] = "failed"
            row["errors"].append({"stage": "capture_context", "message": "environment or repository custody is incomplete"})
        path = folder / "final-result.json"; row["result_pointer"] = path.relative_to(output).as_posix()
        capture.strict_json_bytes(row); write_json(path, row)
    write_json(output / "case-results.json", rows)
    with (output / "raw-index.jsonl").open("x") as stream:
        for row in rows: stream.write(json.dumps(row, ensure_ascii=False, allow_nan=False) + "\n")
    if (capture.strict_json_bytes(read_json((output / "case-results.json").read_bytes())) != capture.strict_json_bytes(rows)
            or capture.strict_json_bytes([read_json(line) for line in (output / "raw-index.jsonl").read_bytes().splitlines()]) != capture.strict_json_bytes(rows)
            or any(capture.strict_json_bytes(read_json((output / row["result_pointer"]).read_bytes())) != capture.strict_json_bytes(row) for row in rows)):
        raise ValueError("final project files and both complete result indexes disagree")


def run(repository: Path, output: Path, python: Path, version: str) -> int:
    output.mkdir(parents=True, exist_ok=False)
    lock, planned, rows, common_error, before = None, [], [], None, None
    environment = {"prepared": False}; repository_unchanged = False
    try:
        host_guard()  # First admission check, before any target work or download.
        if os.environ.get("MYPYPATH"):
            raise ValueError("ambient MYPYPATH is outside the fixed project environment")
        if version not in VERSIONS.values() or not python.is_absolute():
            raise ValueError("exact absolute pinned base interpreter is required")
        lock, planned = load_inputs(repository)
        paths = [*lock["source_files"], LOCK_PATH, "tools/ci/q0_16_project_contracts.py", "tools/ci/test_q0_16_project_contracts.py"]
        before = {path: capture.fact(repository / path) for path in paths}
        write_json(output / "source-before.json", before)
        write_sources(output / "locked-sources", {path: (repository / path).read_bytes() for path in paths})
        profile = source_json(repository, "crates/mamba-quality/assets/profile.json")
        pydantic = source_json(repository, "tools/ci/q0_pydantic_reference_lock.json")
        mypy = source_json(repository, "tools/ci/q0_mypy_reference_lock.json")
        capture.pydantic_reference.validate_lock(pydantic)
        checked_bytes(python.resolve(strict=True).read_bytes(), pydantic["interpreters"][version]["executable_sha256"])
        for relative, expected in pydantic["source_files"].items():
            checked_bytes((repository / safe_path(relative)).read_bytes(), expected)
        for tool, dependency in (("pydantic", pydantic), ("mypy", mypy)):
            checked_bytes((repository / "crates/mamba-quality/assets/profile.json").read_bytes(), dependency["profile_sha256"])
            download(dependency["sdist"], output / "downloads/sdist" / tool)
        write_json(output / "run-identity.json", {"schema": "mamba.q0.16.named-project-run.v1",
            "source_sha": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "python_version": version,
            "host_context": capture.pylint_reference.HOST_CONTEXT, "lock_sha256": LOCK_SHA,
            "planned_project_commands": len(planned), "observation_kind": "named-project-reference-contract",
            "issue_accepted": False, "native_parity": "not_run", "source_programs_executed": False})
        try:
            executable, identity = capture.prepare_environment(repository, python, version, output, "mypy", profile, {"pydantic": pydantic})
            tree_before = capture.artifact_inventory(output / "environments/mypy/venv", [])
            if not tree_before: raise ValueError("prepared environment tree is empty")
            write_json(output / "environments/mypy/tree-before.json", tree_before)
            environment = {"prepared": True, "identity": identity, "executable": str(executable)}
        except Exception as error:
            environment["preparation_error"] = {"type": type(error).__name__, "message": str(error)}
        if environment["prepared"]:
            rows = execute_plan(planned, version, executable, identity, lock["strict_flags"], output)
        else:
            for case in planned:
                row = base_row(case, version); row["errors"].append({"stage": "environment_preparation", **environment["preparation_error"]}); rows.append(row)
    except Exception as error:
        common_error = {"type": type(error).__name__, "message": str(error)}
    if not planned:
        try:
            lock = frozen_lock(repository)
            planned = [{"family": row["family"], "id": row["id"], "name": row["name"],
                        "entrypoints": row["entrypoints"], "binding": row} for row in lock["cases"]]
        except Exception as error:
            if common_error is None: common_error = {"type": type(error).__name__, "message": str(error)}
            else: common_error["fallback_error"] = {"type": type(error).__name__, "message": str(error)}
    existing = {row["id"] for row in rows}
    for case in planned:
        if case["id"] not in existing:
            row = base_row(case, version); row["errors"].append({"stage": "common_prerequisite", "message": "capture did not reach this planned project"}); rows.append(row)
    if environment["prepared"]:
        try:
            root = output / "environments/mypy"; package = read_json((root / "identity.json").read_bytes())
            records = capture.capture_installed_records(root, package, "mypy", "after")
            payloads = capture.environment_payloads(root, package, "mypy")
            write_json(root / "payloads-after.json", payloads)
            tree_after = capture.artifact_inventory(root / "venv", [])
            write_json(root / "tree-after.json", tree_after)
            environment["unchanged"] = (records["all_sources_match"] and payloads == read_json((root / "payloads-before.json").read_bytes())
                and tree_after == read_json((root / "tree-before.json").read_bytes())
                and digest((root / "identity.json").read_bytes()) == environment["identity"]["identity_sha256"]
                and digest(python.resolve(strict=True).read_bytes()) == environment["identity"]["base_executable_sha256"])
        except Exception as error:
            environment["unchanged"] = False; environment["post_capture_error"] = {"type": type(error).__name__, "message": str(error)}
    if before is not None:
        try:
            after = {path: capture.fact(repository / path) for path in before}
            write_json(output / "source-after.json", after); repository_unchanged = after == before
        except Exception as error:
            if common_error is None: common_error = {"type": type(error).__name__, "message": str(error)}
            else: common_error["source_after_error"] = {"type": type(error).__name__, "message": str(error)}
    context = common_error is None and repository_unchanged and environment.get("unchanged") is True
    publication_error = None
    try: publish(rows, planned, output, context)
    except Exception as error: publication_error = {"type": type(error).__name__, "message": str(error)}
    counts = process_counts(rows)
    passed = (context and publication_error is None and len(planned) == len(rows) == 575
        and all(row["status"] == "passed" for row in rows) and all(value == 575 for value in counts.values()))
    summary = {"schema": "mamba.q0.16.named-project-reference-summary.v1", "comparison_passed": passed,
        "python_version": version, "planned_project_commands": 575, "planned_source_files": 1069,
        "planned_target_cells_all_versions": 1725, "result_count": len(rows), **counts,
        "passed_projects": sum(row["status"] == "passed" for row in rows), "failed_projects": sum(row["status"] == "failed" for row in rows),
        "not_run_projects": sum(row["status"] == "not_run" for row in rows), "environment": environment,
        "common_error": common_error, "publication_error": publication_error, "repository_sources_unchanged": repository_unchanged,
        "issue_accepted": False, "native_parity": "not_run", "som_accepted": False,
        "artifact_exclusions": ["environments/mypy/venv"]}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", capture.artifact_inventory(output, [output / "environments/mypy/venv"]))
    print(json.dumps({key: summary[key] for key in ("comparison_passed", "result_count", "actual_project_processes", "waited_project_processes", "issue_accepted")}))
    return 0 if passed else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True); parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True); parser.add_argument("--python-version", choices=VERSIONS.values(), required=True)
    args = parser.parse_args()
    if not args.repository.is_absolute() or not args.output.is_absolute() or not args.python.is_absolute():
        parser.error("repository, output and base interpreter paths must be absolute")
    return run(args.repository.resolve(strict=True), args.output.resolve(), args.python, args.python_version)


if __name__ == "__main__":
    raise SystemExit(main())
