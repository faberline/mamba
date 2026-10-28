#!/usr/bin/env python3
"""Retain one fixed hosted Pylint shard; full reference acceptance is external."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import inspect
import json
import os
from pathlib import Path
import sys

from q0_mypy_reference import command, require_python_identity, require_receipt, validate_dependencies
from q0_pylint_normalize import PylintComparator
from q0_ruff_reference import checked_bytes, digest, download, inventory, run_command, write_json

VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
CASE_COUNT = 19747
SHARD_COUNT = 4
WORKERS = 4
HOST_CONTEXT = {"sys_platform": "darwin", "platform_system": "Darwin", "runner_os": "macOS",
                "runner_arch": "ARM64", "runner_label": "macos-15"}
REFERENCE_EXITS = tuple(range(32))
PINNED_VERSIONS = {
    "pylint": "4.1.2", "astroid": "4.3.3", "dill": "0.4.1", "isort": "9.0.2",
    "mccabe": "0.7.0", "platformdirs": "4.12.3", "tomlkit": "0.15.1", "mypy_extensions": "1.1.0",
    "typing_extensions": "4.16.0",
}
COMPARATOR_SOURCES = {
    "crates/mamba-quality/src/infrastructure/pylint/tests.rs",
    "crates/mamba-quality/src/infrastructure/pylint/tests/path_prefix.rs",
    "crates/mamba-quality/src/infrastructure/pylint/tests/crash_filename.rs",
    "crates/mamba-quality/src/infrastructure/pylint/tests/variants.rs",
    "crates/mamba-quality/src/infrastructure/pylint/tests/variants.json",
    "tools/ci/q0_pylint_normalize.py",
}

PACKAGE_PROBE = inspect.getsource(validate_dependencies) + """
import builtins, hashlib, importlib.metadata, importlib.util, json, pathlib, sys
import pylint, astroid
from pylint.lint import PyLinter
from pip._vendor.packaging.utils import canonicalize_name
pins = json.loads(sys.argv[1])
distributions = {}
for pin in pins:
    name = canonicalize_name(pin['name'])
    try:
        d = importlib.metadata.distribution(pin['name'])
    except importlib.metadata.PackageNotFoundError:
        distributions[name] = {'name':pin['name'], 'missing_distribution':True}
        continue
    files, missing = {}, []
    for relative in d.files or []:
        path = pathlib.Path(d.locate_file(relative))
        if path.is_file():
            raw = path.read_bytes()
            files[str(relative)] = {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
        else:
            missing.append(str(relative))
    distributions[name] = {'name':d.metadata['Name'], 'version':d.version,
        'requires_dist':d.requires or [], 'distribution_files':files,
        'missing_distribution_files':missing}
try:
    verification = validate_dependencies(pins, distributions)
except Exception as error:
    verification = {'dependencies_verified':False, 'error_class':type(error).__name__, 'message':str(error)}
linter = PyLinter()
linter.load_default_plugins()
rules = [message.msgid for message in sorted(linter.msgs_store.messages, key=lambda message:message.msgid)
         if linter.is_message_enabled(message.msgid)]
modules = {}
for name, module in [('pylint',pylint),('astroid',astroid)]:
    path = pathlib.Path(module.__file__)
    raw = path.read_bytes()
    modules[name] = {'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
print(json.dumps({'python':list(sys.version_info[:3]), 'executable':sys.executable,
    'cache_tag':sys.implementation.cache_tag, 'magic_number_hex':importlib.util.MAGIC_NUMBER.hex(),
    'isolated':sys.flags.isolated, 'pylint':importlib.metadata.version('pylint'),
    'astroid':importlib.metadata.version('astroid'), 'rules':rules, 'builtins':sorted(dir(builtins)),
    'modules':modules, 'distributions':distributions, 'dependency_verification':verification}))
"""


def select_shard(cases: list[dict], shard_index: int, shard_count: int) -> list[dict]:
    if type(shard_index) is not int or shard_count != SHARD_COUNT or not 0 <= shard_index < SHARD_COUNT:
        raise ValueError("fixed Pylint partition requires shard index 0..3 and shard count 4")
    if (len(cases) != CASE_COUNT or [row.get("index") for row in cases] != list(range(CASE_COUNT))
            or any(type(row.get("index")) is not int for row in cases)
            or len({row["name"] for row in cases}) != CASE_COUNT):
        raise ValueError("complete ordered Pylint input coverage differs")
    selected = [case for case in cases if case["index"] % SHARD_COUNT == shard_index]
    if not selected:
        raise ValueError("Pylint shard cannot be empty")
    return selected


def reference_argv(executable: Path, names: list[str], minor: int) -> list[str]:
    if (minor not in VERSIONS or not names or names != sorted(names) or len(set(names)) != len(names)
            or any(Path(name).is_absolute() or ".." in Path(name).parts or not name.endswith(".py") for name in names)):
        raise ValueError("Pylint command requires every sorted relative .py filename")
    return [str(executable), "-I", "-m", "pylint", "--rcfile=/dev/null", "--persistent=n",
            "--max-line-length=88", f"--py-version=3.{minor}", "--output-format=json2", *names]


def source_manifest(case: dict) -> dict:
    if set(case["files"]) != set(case["source_sha256"]):
        raise ValueError("case source hash denominator differs")
    files = {}
    for name, source in case["files"].items():
        path = Path(name)
        if path.is_absolute() or not path.parts or ".." in path.parts:
            raise ValueError("unsafe Pylint source path")
        raw = checked_bytes(source.encode(), case["source_sha256"][name])
        files[name] = {"bytes": len(raw), "sha256": digest(raw)}
    if not files:
        raise ValueError("Pylint source denominator is empty")
    return files


def require_bytecode_identity(environment: dict, minor: int) -> tuple[str, bytes]:
    tag, magic_hex = environment.get("cache_tag"), environment.get("magic_number_hex")
    if tag != f"cpython-3{minor}" or not isinstance(magic_hex, str):
        raise ValueError("target bytecode identity is missing or differs")
    try:
        magic = bytes.fromhex(magic_hex)
    except ValueError as error:
        raise ValueError("invalid target bytecode magic number") from error
    if len(magic) != 4 or magic.hex() != magic_hex:
        raise ValueError("invalid target bytecode magic number")
    return tag, magic


def fixture_file_state(case: dict, before: dict, after: dict, project: Path,
                       environment: dict, minor: int) -> dict:
    """Keep every inventory row; permit only identified caches for declared .py files."""
    tag, magic = require_bytecode_identity(environment, minor)
    fixtures = source_manifest(case)
    missing = sorted(set(fixtures) - set(after))
    changed = sorted(name for name in fixtures if name in after and fixtures[name] != after[name])
    allowed = {str(Path(name).parent / "__pycache__" / f"{Path(name).stem}.{tag}.pyc"): name
               for name in fixtures if name.endswith(".py")}
    generated, invalid, unexpected = {}, {}, []
    for name in sorted(set(after) - set(fixtures)):
        if name not in allowed:
            unexpected.append(name)
            continue
        try:
            raw = (project / name).read_bytes()
            if raw[:4] != magic:
                raise ValueError("cache magic number differs from target")
            if after[name] != {"bytes": len(raw), "sha256": digest(raw)}:
                raise ValueError("cache bytes differ from retained inventory")
            generated[name] = {"source": allowed[name], "bytes": len(raw), "sha256": digest(raw),
                               "cache_tag": tag, "magic_number_hex": raw[:4].hex()}
        except (OSError, ValueError) as error:
            invalid[name] = {"error_class": type(error).__name__, "message": str(error)}
    fixture_unchanged = before == fixtures and not missing and not changed
    return {"fixture_files_unchanged": fixture_unchanged, "project_tree_unchanged": before == after,
            "source_before_matches_fixture": before == fixtures,
            "generated_cache_files": generated, "missing_fixture_files": missing,
            "changed_fixture_files": changed, "unexpected_project_files": unexpected,
            "invalid_cache_files": invalid,
            "project_files_acceptable": fixture_unchanged and not unexpected and not invalid}


def require_pins(pins: list[dict]) -> None:
    if (len(pins) != len(PINNED_VERSIONS)
            or {pin["name"]: pin["version"] for pin in pins} != PINNED_VERSIONS):
        raise ValueError("Pylint runtime and fixture-import pins differ")


def require_host_context(context: dict) -> None:
    if context != HOST_CONTEXT:
        raise ValueError("Pylint locked host context differs")
    if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != HOST_CONTEXT["sys_platform"]
            or os.environ.get("RUNNER_ARCH") != HOST_CONTEXT["runner_arch"]
            or os.environ.get("RUNNER_OS") != HOST_CONTEXT["runner_os"]):
        raise ValueError("original execution requires the GitHub-hosted macos-15 macOS ARM64 job")


def require_package_identity(identity: dict, version: str, pins: list[dict], rules: list[str], venv: Path) -> None:
    require_pins(pins)
    require_python_identity(identity, version)
    require_bytecode_identity(identity, int(version.split(".")[1]))
    if (identity["pylint"] != "4.1.2" or identity["astroid"] != "4.3.3"
            or len(identity["rules"]) != 383 or identity["rules"] != rules):
        raise ValueError("Pylint version or enabled default rule identity differs")
    markers = identity["dependency_verification"].get("marker_environment", {})
    if (markers.get("python_full_version") != version or markers.get("python_version") != ".".join(version.split(".")[:2])
            or markers.get("sys_platform") != HOST_CONTEXT["sys_platform"]
            or markers.get("platform_system") != HOST_CONTEXT["platform_system"]
            or markers.get("platform_machine") != "arm64"):
        raise ValueError("Pylint dependency marker environment differs from target")
    if validate_dependencies(pins, identity["distributions"], markers) != identity["dependency_verification"]:
        raise ValueError("Pylint dependency verification differs")
    if set(identity["modules"]) != {"pylint", "astroid"}:
        raise ValueError("Pylint module identity denominator differs")
    for name, module in identity["modules"].items():
        path = Path(module["path"])
        if not path.is_relative_to(venv) or digest(path.read_bytes()) != module["sha256"]:
            raise ValueError(f"module identity is outside the isolated environment or differs: {name}")


def require_venv_link(executable: Path, base: Path) -> dict:
    if not executable.is_symlink() or executable.resolve(strict=True) != base.resolve(strict=True):
        raise ValueError("Mac venv interpreter must link to the exact pinned base")
    return {"venv_creation_mode": "symlinks", "venv_executable_is_symlink": True,
            "venv_executable_resolved": str(executable.resolve(strict=True)),
            "base_executable_resolved": str(base.resolve(strict=True))}


def prepare(python: Path, version: str, root: Path, pins: list[dict], wheels: list[Path], rules: list[str]) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated}))"
    command([str(python), "-I", "-B", "-c", probe], root, root / "base-identity")
    base = json.loads((root / "base-identity/stdout.raw").read_bytes())
    require_python_identity(base, version)
    command([str(python), "-I", "-B", "-m", "venv", "--symlinks", str(root / "venv")], root, root / "create-venv")
    executable = root / "venv/bin/python"
    require_venv_link(executable, python)
    require_pins(pins)
    if len(wheels) != len(PINNED_VERSIONS):
        raise ValueError("Pylint dependency artifact denominator differs")
    artifacts = []
    for pin, wheel in zip(pins, wheels):
        raw = checked_bytes(wheel.read_bytes(), pin["sha256"])
        artifacts.append({"name": pin["name"], "version": pin["version"], "filename": wheel.name,
                          "bytes": len(raw), "sha256": digest(raw)})
    command([str(executable), "-I", "-B", "-m", "pip", "install", "--no-index", "--no-deps",
             "--no-compile", *map(str, wheels)], root, root / "install", inputs={"wheels": artifacts})
    command([str(executable), "-I", "-B", "-c", PACKAGE_PROBE, json.dumps(pins)], root,
            root / "package-identity", inputs={"wheels": artifacts})
    identity = json.loads((root / "package-identity/stdout.raw").read_bytes())
    require_package_identity(identity, version, pins, rules, root / "venv")
    command([str(executable), "-I", "-B", "-m", "pip", "list", "--format=json"], root, root / "installed-distributions")
    command([str(executable), "-I", "-B", "-m", "pip", "freeze", "--all"], root, root / "pip-freeze")
    identity.update(base_identity=base, base_executable_sha256=digest(python.read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()), wheel_artifacts=artifacts,
                    historical_environment=False, **require_venv_link(executable, python))
    for pin, wheel in zip(pins, wheels):
        checked_bytes(wheel.read_bytes(), pin["sha256"])
    write_json(root / "identity.json", identity)
    return executable


def run_case(executable: Path, root: Path, case: dict, expected: dict, minor: int,
             comparator: PylintComparator, environment_identity: dict) -> dict:
    from q0_pylint_inputs import materialize_case, project_diagnostics
    index = case["index"]
    project = root / f"case_{index:04}"
    output = root / "commands" / f"case_{index:04}"
    result = {"index": index, "name": case["name"], "python_minor": minor,
              "source_sha256": case["source_sha256"], "project": str(project),
              "status": "failed", "failures": [], "source_unchanged": False,
              "fixture_files_unchanged": False, "project_tree_unchanged": False, "generated_cache_files": {},
              "environment_identity": environment_identity,
              "receipt": None, "comparison": None, "program_success": False,
              "canonical_diagnostic_count": len(expected["expected"][minor - 12])}
    before = None
    try:
        if (case["name"], case["files"], case["source_sha256"]) != (expected["name"], expected["files"], expected["source_sha256"]):
            raise ValueError("input case differs from the published comparison case")
        manifest = source_manifest(case)
        require_bytecode_identity(environment_identity, minor)
        materialized = materialize_case(case, project)
        before = inventory(project)
        result["source_before"] = before
        names = sorted(name for name in case["files"] if name.endswith(".py"))
        if before != manifest or materialized["files"] != before or materialized["python_files"] != names:
            raise ValueError("Pylint materialization omitted or changed fixture files")
        receipt = run_command(reference_argv(executable, names, minor), project, output,
                              expected_exits=REFERENCE_EXITS, timeout=120,
                              inputs={"index": index, "name": case["name"], "files": before,
                                      "source_sha256": case["source_sha256"], "environment": environment_identity,
                                      "published_case_sha256": digest(json.dumps(expected, sort_keys=True).encode())})
        result["receipt"] = receipt
        result["program_success"] = receipt["actual_exit"] == 0
        require_receipt(receipt, REFERENCE_EXITS)
        projected = project_diagnostics((output / "stdout.raw").read_bytes(), project)
        result["projected_diagnostics"] = projected
        result["observed_diagnostic_count"] = len(projected)
        result["reference_outcome"] = ("fatal" if any(d["severity"] == "fatal" for d in projected)
                                       else "diagnostics" if projected else "clean")
        result["comparison"] = comparator.compare(expected, minor, projected, receipt["actual_exit"])
        if not result["comparison"]["matched"]:
            result["failures"].append({"error_class": "ComparisonMismatch", "message": "Pylint diagnostics or actual exit differ"})
    except Exception as error:
        result["failures"].append({"error_class": type(error).__name__, "message": str(error)})
    finally:
        if project.exists():
            try:
                after = inventory(project)
                result["source_after"] = after
                state = fixture_file_state(case, before or {}, after, project, environment_identity, minor)
                result.update(state)
                # This legacy field now means declared fixture files, not the whole tree.
                result["source_unchanged"] = state["fixture_files_unchanged"]
                if not state["project_files_acceptable"]:
                    result["failures"].append({"error_class": "SourceChanged", "message": "Pylint fixture bytes or generated-file contract changed"})
            except Exception as error:
                result["failures"].append({"error_class": type(error).__name__, "message": str(error)})
    if not result["failures"] and result["source_unchanged"] and result["comparison"]["matched"]:
        result["status"] = "passed"
    write_json(root / "results" / f"case_{index:04}.json", result)
    return result


def execute_cases(executable: Path, root: Path, selected: list[dict], expected: list[dict], minor: int,
                  comparator: PylintComparator, environment_identity: dict) -> list[dict]:
    (root / "results").mkdir(exist_ok=False)
    def execute(case):
        try:
            return run_case(executable, root, case, expected[case["index"]], minor, comparator, environment_identity)
        except Exception as error:
            return {"index": case["index"], "name": case["name"], "status": "failed",
                    "source_sha256": case["source_sha256"], "receipt": None, "source_unchanged": False,
                    "failures": [{"error_class": type(error).__name__, "message": str(error)}]}
    # Only the dispatch threads are reused. run_case starts a fresh OS process.
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        return list(pool.map(execute, selected))


def require_shard_results(selected: list[dict], rows: list[dict], expected: list[dict], minor: int,
                          executable: Path, root: Path, environment_identity: dict) -> None:
    if not selected or len(rows) != len(selected):
        raise ValueError("Pylint shard result coverage is empty or incomplete")
    if [(r.get("index"), r.get("name"), r.get("source_sha256")) for r in rows] != [
            (c["index"], c["name"], c["source_sha256"]) for c in selected]:
        raise ValueError("Pylint shard result index, name, source identity, or order differs")
    for case, row in zip(selected, rows):
        require_receipt(row.get("receipt") or {}, REFERENCE_EXITS)
        receipt = row["receipt"]
        project = root / f"case_{case['index']:04}"
        after = inventory(project)
        state = fixture_file_state(case, row.get("source_before") or {}, after, project, environment_identity, minor)
        names = sorted(name for name in case["files"] if name.endswith(".py"))
        comparison = row.get("comparison") or {}
        if (row.get("status") != "passed" or row.get("failures") or row.get("source_unchanged") is not True
                or row.get("environment_identity") != environment_identity
                or row.get("source_after") != after or not state["project_files_acceptable"]
                or any(row.get(key) != value for key, value in state.items())
                or receipt.get("argv") != reference_argv(executable, names, minor) or receipt.get("cwd") != str(project)
                or receipt["actual_exit"] != expected[case["index"]]["reference_exits"][minor - 12]
                or comparison.get("matched") is not True
                or comparison.get("matched_by") not in {"canonical", "approved_whole_list_variant"}
                or comparison.get("actual_exit") != receipt["actual_exit"]
                or comparison.get("reference_exit") != receipt["actual_exit"]):
            raise ValueError(f"Pylint shard case did not pass every gate: {case['index']}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--python-version", choices=VERSIONS.values(), required=True)
    parser.add_argument("--shard-index", type=int, choices=range(SHARD_COUNT), required=True)
    parser.add_argument("--shard-count", type=int, choices=[SHARD_COUNT], required=True)
    args = parser.parse_args()
    if not all(path.is_absolute() for path in [args.repository, args.output, args.python]):
        parser.error("repository, output and interpreter paths must be absolute")
    repository, output = args.repository.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    minor = int(args.python_version.split(".")[1])
    root = output / f"python3{minor}"
    rows, selected, expected, error, passed = [], [], [], None, False
    try:
        require_host_context(HOST_CONTEXT)
        from q0_pylint_inputs import prepare_inputs, published
        lock_path = repository / "tools/ci/q0_pylint_reference_lock.json"
        lock = json.loads(lock_path.read_bytes())
        require_host_context(lock["platform"])
        if (lock["python_versions"] != list(VERSIONS.values()) or lock["version"] != "4.1.2" or lock["astroid"] != "4.3.3"
                or lock["case_count"] != CASE_COUNT or lock["source_file_count"] != 20651
                or lock["shard_count"] != SHARD_COUNT or lock["workers"] != WORKERS
                or lock["reference_timeout_seconds"] != 120 or len(lock["rules"]) != 383
                or lock["rules"] != sorted(set(lock["rules"]))
                or {k: v["version"] for k, v in lock["dependencies"].items()} != PINNED_VERSIONS
                or not COMPARATOR_SOURCES.issubset(lock["comparator_sources"])):
            raise ValueError("fixed Pylint reference lock differs")
        profile = repository / "crates/mamba-quality/assets/profile.json"
        checked_bytes(profile.read_bytes(), lock["profile_sha256"])
        for relative, sha in lock["comparator_sources"].items():
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("unsafe comparator source path")
            checked_bytes((repository / path).read_bytes(), sha)
        write_json(output / "run-identity.json", {
            "source_sha": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "runner_os": os.environ.get("RUNNER_OS"),
            "runner_arch": os.environ.get("RUNNER_ARCH"), "repository": str(repository),
            "host_context": HOST_CONTEXT, "sys_platform": sys.platform,
            "python_version": args.python_version, "python_minor": minor,
            "shard_index": args.shard_index, "shard_count": SHARD_COUNT,
            "reference_timeout_seconds": 120, "workers": WORKERS,
            "lock_sha256": digest(lock_path.read_bytes()), "comparator_sources": lock["comparator_sources"],
            "source_files": {name: digest((repository / "tools/ci" / name).read_bytes()) for name in
                             ["q0_pylint_reference.py", "q0_pylint_inputs.py", "q0_pylint_normalize.py",
                              "q0_ruff_reference.py", "q0_mypy_reference.py"]}})
        download(lock["sdist"], output / "downloads/sdist")
        cases, proof = prepare_inputs(repository)
        if (proof["input_sha256"] != lock["input_sha256"] or proof["case_count"] != CASE_COUNT
                or proof["source_file_count"] != 20651):
            raise ValueError("Pylint fixed input identity differs")
        selected = select_shard(cases, args.shard_index, args.shard_count)
        expected = published(repository)
        if len(expected) != CASE_COUNT:
            raise ValueError("Pylint published comparison case coverage differs")
        write_json(output / "input-proof.json", proof)
        write_json(output / "selected-inputs.json", selected)
        pins, wheels = [], []
        for index, (name, item) in enumerate(lock["dependencies"].items()):
            artifact = item["wheels"][str(minor)]
            destination = output / "downloads" / f"wheel-{index:02}"
            download(artifact, destination)
            pins.append({"name": name, "version": item["version"], **artifact})
            wheels.append(destination / artifact["filename"])
        executable = prepare(args.python, args.python_version, root, pins, wheels, lock["rules"])
        environment_identity = {"identity_sha256": digest((root / "identity.json").read_bytes()),
                                "executable_sha256": digest(executable.read_bytes())}
        package_identity = json.loads((root / "identity.json").read_bytes())
        environment_identity.update(cache_tag=package_identity["cache_tag"], magic_number_hex=package_identity["magic_number_hex"])
        rows = execute_cases(executable, root, selected, expected, minor, PylintComparator(repository), environment_identity)
        write_json(output / "case-results.json", rows)
        require_shard_results(selected, rows, expected, minor, executable, root, environment_identity)
        passed = True
    except Exception as failure:
        error = {"error_class": type(failure).__name__, "message": str(failure)}
    summary = {"schema": "mamba.q0.pylint.hosted-reference-shard.v1", "issue": 162,
               "python_version": args.python_version, "python_minor": minor,
               "host_context": HOST_CONTEXT,
               "shard_index": args.shard_index, "shard_count": SHARD_COUNT, "workers": WORKERS,
               "full_case_count": CASE_COUNT, "assigned_case_count": len(selected), "result_count": len(rows),
               "global_indices": [c["index"] for c in selected],
               "passed_case_count": sum(r.get("status") == "passed" for r in rows),
               "actual_process_count": sum(type((r.get("receipt") or {}).get("pid")) is int for r in rows),
               "waited_process_count": sum((r.get("receipt") or {}).get("wait_completed") is True for r in rows),
               "observed_diagnostic_count": sum(r.get("observed_diagnostic_count", 0) for r in rows),
               "canonical_diagnostic_count": (sum(len(expected[c["index"]]["expected"][minor - 12]) for c in selected)
                                              if len(expected) == CASE_COUNT else None),
               "unparsed_case_count": sum("observed_diagnostic_count" not in r for r in rows),
               "failed_indices": [r["index"] for r in rows if r.get("status") != "passed"],
               "shard_execution_passed": passed, "error": error, "issue_accepted": False,
               "full_reference_accepted": False, "historical_original_evidence_recovered": False,
               "whole_suite_accepted": False}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", inventory(output, exclude_venvs=True))
    print(json.dumps({"shard_execution_passed": passed, "full_reference_accepted": False}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
