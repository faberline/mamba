#!/usr/bin/env python3
"""Retain the fixed mypy reference run on hosted Linux without issue acceptance."""
from __future__ import annotations

import argparse
import inspect
import json
import os
from pathlib import Path
import sys

from q0_ruff_reference import checked_bytes, checked_command, digest, download, inventory, run_command, write_json

VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
CASE_COUNT = 22898
EXPECTED_COUNTS = {12: 25951, 13: 25811, 14: 25811}
PINNED_VERSIONS = {
    "mypy": "2.4.0", "mypy_extensions": "1.1.0", "pathspec": "1.1.1",
    "librt": "0.16.0", "ast_serialize": "0.12.1", "typing_extensions": "4.16.0",
    "pydantic": "2.13.5", "pydantic_core": "2.46.5", "annotated-types": "0.8.0",
    "typing-inspection": "0.4.4",
}


def validate_dependencies(pins: list[dict], distributions: dict[str, dict],
                          environment: dict | None = None) -> dict:
    """Check every exact pin, payload, and active runtime requirement."""
    from pip._vendor.packaging.markers import default_environment
    from pip._vendor.packaging.requirements import Requirement
    from pip._vendor.packaging.utils import canonicalize_name
    from pip._vendor.packaging.version import Version

    locked = {}
    for pin in pins:
        name = canonicalize_name(pin["name"])
        if name in locked:
            raise ValueError(f"duplicate dependency pin: {name}")
        Version(pin["version"])
        locked[name] = pin["version"]
    if not locked:
        raise ValueError("empty dependency pin denominator")
    installed = {canonicalize_name(name): row for name, row in distributions.items()}
    if len(installed) != len(distributions):
        raise ValueError("duplicate installed dependency identity")
    for name, version in locked.items():
        if name not in installed or installed[name].get("missing_distribution"):
            raise ValueError(f"pinned dependency is missing: {name}")
        row = installed[name]
        if canonicalize_name(row["name"]) != name or row["version"] != version:
            raise ValueError(f"installed dependency version or name differs from pin: {name}")
        if row.get("missing_distribution_files") or not row.get("distribution_files"):
            raise ValueError(f"installed dependency payload is incomplete: {name}")
    markers = default_environment()
    markers.update(environment or {})
    markers["extra"] = ""
    active, inactive = [], []
    for parent in locked:
        for text in installed[parent].get("requires_dist", []):
            requirement = Requirement(text)
            record = {"distribution": parent, "requirement": text,
                      "name": canonicalize_name(requirement.name)}
            if requirement.marker is not None and not requirement.marker.evaluate(markers):
                inactive.append(record)
                continue
            if requirement.url is not None or requirement.extras:
                raise ValueError(f"unsupported active dependency requirement: {text}")
            name = record["name"]
            if name not in locked:
                raise ValueError(f"active dependency is not pinned: {name}")
            actual = installed[name]["version"]
            if not requirement.specifier.contains(actual, prereleases=True):
                raise ValueError(f"dependency constraint is not satisfied: {text}")
            record.update(version=actual, specifier=str(requirement.specifier), satisfied=True)
            active.append(record)
    return {"pinned_versions": locked, "active_requirements": active,
            "inactive_requirements": inactive, "marker_environment": markers,
            "dependencies_verified": True}


PACKAGE_PROBE = inspect.getsource(validate_dependencies) + """
import hashlib, importlib.metadata, json, pathlib, sys
import mypy.main, mypy.version, pydantic
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
        p = pathlib.Path(d.locate_file(relative))
        if p.is_file():
            raw = p.read_bytes()
            files[str(relative)] = {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
        else:
            missing.append(str(relative))
    distributions[name] = {'name':d.metadata['Name'], 'version':d.version,
        'requires_dist':d.requires or [], 'distribution_files':files,
        'missing_distribution_files':missing}
try:
    verification = validate_dependencies(pins, distributions)
except Exception as error:
    verification = {'dependencies_verified':False, 'error_class':type(error).__name__,
                    'message':str(error)}
modules = {}
for name, module in [('mypy.main',mypy.main),('mypy.version',mypy.version),('pydantic',pydantic)]:
    path = pathlib.Path(module.__file__)
    raw = path.read_bytes()
    modules[name] = {'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
print(json.dumps({'python':list(sys.version_info[:3]), 'executable':sys.executable,
    'isolated':sys.flags.isolated,
    'mypy_version':mypy.version.__version__, 'pydantic_version':pydantic.__version__,
    'strict_flags':mypy.main.define_options()[1], 'modules':modules,
    'distributions':distributions, 'dependency_verification':verification}))
"""


def require_receipt(receipt: dict, exits: tuple[int, ...] = (0,)) -> None:
    pid = receipt.get("pid")
    if (type(pid) is not int or pid <= 0 or receipt.get("waited_pid") != pid
            or receipt.get("wait_completed") is not True or receipt.get("timed_out") is not False
            or receipt.get("launch_error") is not None
            or type(receipt.get("actual_exit")) is not int
            or receipt["actual_exit"] not in exits or receipt.get("passed") is not True):
        raise ValueError("process lacks a successful terminal receipt")


def command(argv: list[str], cwd: Path, output: Path, **options) -> dict:
    receipt = checked_command(argv, cwd, output, **options)
    require_receipt(receipt, options.get("expected_exits", (0,)))
    return receipt


def require_python_identity(identity: dict, expected: str) -> None:
    if (identity.get("python") != [int(part) for part in expected.split(".")]
            or type(identity.get("isolated")) is not int or identity["isolated"] != 1):
        raise ValueError(f"expected isolated Python {expected}")


def reference_argv(executable: Path, project: Path, names: list[str], minor: int) -> list[str]:
    if minor not in VERSIONS or len(names) != CASE_COUNT or len(set(names)) != CASE_COUNT:
        raise ValueError("reference command requires every ordered fixed case")
    if any(Path(name).is_absolute() or len(Path(name).parts) != 1 for name in names):
        raise ValueError("reference case names must be relative filenames")
    return [str(executable), "-I", "-m", "mypy", "--strict", "--config-file",
            str(project / "mypy.ini"), "--python-version", f"3.{minor}",
            "--cache-dir=/dev/null", "--no-incremental", "--output=json", *names]


def require_comparison(comparison: dict, cases: list[dict], expected_count: int) -> None:
    rows = comparison.get("cases")
    if (len(cases) != CASE_COUNT or not isinstance(rows, list) or len(rows) != CASE_COUNT
            or comparison.get("case_count") != CASE_COUNT
            or comparison.get("matching_case_count") != CASE_COUNT
            or comparison.get("diagnostic_count") != expected_count
            or comparison.get("comparisons_passed") is not True):
        raise ValueError("reference comparison is incomplete or differs")
    identities = lambda values: [(r.get("name"), r.get("filename"), r.get("source_sha256")) for r in values]
    if identities(rows) != identities(cases) or any(r.get("matches_expected") is not True for r in rows):
        raise ValueError("reference result case identity, order, or match differs")
    if any(not isinstance(r.get("diagnostics"), list)
           or r.get("diagnostic_count") != len(r["diagnostics"]) for r in rows):
        raise ValueError("reference result diagnostic row count differs")
    if sum(r["diagnostic_count"] for r in rows) != expected_count:
        raise ValueError("reference result diagnostic total differs")


def require_unchanged(before: dict, after: dict) -> None:
    if not before or before != after:
        raise ValueError("reference project files changed")


def prepare(python: Path, expected: str, root: Path, pins: list[dict],
            wheels: list[Path], strict_flags: list[str]) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated}))"
    command([str(python), "-I", "-B", "-c", probe], root, root / "base-identity")
    base = json.loads((root / "base-identity/stdout.raw").read_bytes())
    require_python_identity(base, expected)
    command([str(python), "-I", "-B", "-m", "venv", "--copies", str(root / "venv")],
            root, root / "create-venv")
    executable = root / "venv/bin/python"
    if len(pins) != 10 or len(wheels) != len(pins):
        raise ValueError("dependency artifact denominator differs")
    artifacts = []
    for pin, path in zip(pins, wheels):
        raw = checked_bytes(path.read_bytes(), pin["sha256"])
        artifacts.append({"name": pin["name"], "version": pin["version"], "filename": path.name,
                          "bytes": len(raw), "sha256": digest(raw)})
    command([str(executable), "-I", "-B", "-m", "pip", "install", "--no-index", "--no-deps",
             "--no-compile", *map(str, wheels)], root, root / "install", inputs={"wheels": artifacts})
    command([str(executable), "-I", "-B", "-c", PACKAGE_PROBE, json.dumps(pins)],
            root, root / "package-identity", inputs={"wheels": artifacts})
    identity = json.loads((root / "package-identity/stdout.raw").read_bytes())
    require_python_identity(identity, expected)
    if (identity["mypy_version"] != "2.4.0"
            or identity["pydantic_version"] != "2.13.5" or identity["strict_flags"] != strict_flags):
        raise ValueError("installed original version or strict flags differ")
    markers = identity["dependency_verification"].get("marker_environment", {})
    if (markers.get("python_full_version") != expected
            or markers.get("python_version") != ".".join(expected.split(".")[:2])
            or markers.get("sys_platform") != "linux" or markers.get("platform_system") != "Linux"):
        raise ValueError("dependency marker environment differs from target")
    verification = validate_dependencies(pins, identity["distributions"], markers)
    if identity["dependency_verification"] != verification:
        raise ValueError("installed dependencies were not verified")
    for name, module in identity["modules"].items():
        path = Path(module["path"])
        if not path.is_relative_to(root / "venv") or digest(path.read_bytes()) != module["sha256"]:
            raise ValueError(f"module identity is outside the isolated environment or differs: {name}")
    command([str(executable), "-I", "-B", "-m", "pip", "list", "--format=json"],
            root, root / "installed-distributions")
    command([str(executable), "-I", "-B", "-m", "pip", "freeze", "--all"],
            root, root / "pip-freeze")
    identity.update(base_identity=base, base_executable_sha256=digest(python.read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()),
                    wheel_artifacts=artifacts, historical_environment=False)
    for pin, path in zip(pins, wheels):
        checked_bytes(path.read_bytes(), pin["sha256"])
    write_json(root / "identity.json", identity)
    return executable


def run_target(executable: Path, repository: Path, root: Path, minor: int,
               cases: list[dict], lock: dict) -> dict:
    from q0_mypy_inputs import compare_output, materialize
    project = root / "project"
    materialized = materialize(cases, project)
    before = inventory(project)
    write_json(root / "source-before.json", before)
    if (materialized["case_count"] != CASE_COUNT or materialized["files"] != before
            or materialized["case_filenames"] != [c["filename"] for c in cases]
            or materialized["config_sha256"] != lock["config_sha256"]):
        raise ValueError("materialized source or configuration differs")
    argv = reference_argv(executable, project, materialized["case_filenames"], minor)
    receipt_root = root / "reference"
    receipt = run_command(argv, project, receipt_root, expected_exits=(0, 1),
                          timeout=lock["reference_timeout_seconds"],
                          inputs={"files": before, "case_count": CASE_COUNT,
                                  "executable_sha256": digest(executable.read_bytes()),
                                  "identity_sha256": digest((root / "identity.json").read_bytes())})
    after = inventory(project)
    write_json(root / "source-after.json", after)
    require_receipt(receipt, (0, 1))
    require_unchanged(before, after)
    comparison = compare_output(repository, cases, (receipt_root / "stdout.raw").read_bytes(), project, minor)
    write_json(root / "comparison.json", comparison)
    require_comparison(comparison, cases, lock["expected_diagnostic_counts"][str(minor)])
    return {"status": "passed", "case_count": CASE_COUNT,
            "matching_case_count": comparison["matching_case_count"],
            "diagnostic_count": comparison["diagnostic_count"], "source_unchanged": True,
            "receipt": receipt, "comparison_sha256": digest((root / "comparison.json").read_bytes())}


def terminal_summary(targets: dict[int, dict], common_error: dict | None = None) -> dict:
    terminal = all(targets.get(minor, {}).get("status") in {"passed", "failed", "not_run"}
                   for minor in VERSIONS)
    passed = terminal and common_error is None
    for minor in VERSIONS:
        target = targets.get(minor, {})
        try:
            require_receipt(target.get("receipt", {}), (0, 1))
        except ValueError:
            passed = False
        passed = (passed and target.get("status") == "passed"
                  and target.get("case_count") == CASE_COUNT
                  and target.get("matching_case_count") == CASE_COUNT
                  and target.get("diagnostic_count") == EXPECTED_COUNTS[minor]
                  and target.get("source_unchanged") is True)
    return {"schema": "mamba.q0.mypy.hosted-reference.v1", "issue": 161,
            "targets": {VERSIONS[m]: targets.get(m, {"status": "not_run"}) for m in VERSIONS},
            "common_error": common_error, "all_targets_terminal": terminal,
            "execution_passed": bool(passed), "issue_accepted": False,
            "historical_original_evidence_recovered": False, "whole_suite_accepted": False}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    for minor in VERSIONS:
        parser.add_argument(f"--python3{minor}", type=Path, required=True)
    args = parser.parse_args()
    if not args.output.is_absolute() or not args.repository.is_absolute():
        parser.error("repository and output paths must be absolute")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    repository = args.repository.resolve()
    targets, common_error = {}, None
    try:
        if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != "linux"
                or os.environ.get("RUNNER_ARCH") != "X64"):
            raise ValueError("original execution requires the GitHub-hosted Linux X64 job")
        from q0_mypy_inputs import CONFIG_BYTES, STRICT_FLAGS, prepare_inputs
        lock_path = repository / "tools/ci/q0_mypy_reference_lock.json"
        lock = json.loads(lock_path.read_bytes())
        if (lock["python_versions"] != list(VERSIONS.values()) or lock["version"] != "2.4.0"
                or lock["pydantic"] != "2.13.5" or lock["case_count"] != CASE_COUNT
                or lock["reference_timeout_seconds"] != 300 or lock["strict_flags"] != STRICT_FLAGS
                or lock["config_sha256"] != digest(CONFIG_BYTES)
                or lock["expected_diagnostic_counts"] != {str(m): n for m, n in EXPECTED_COUNTS.items()}
                or {k: v["version"] for k, v in lock["dependencies"].items()} != PINNED_VERSIONS):
            raise ValueError("fixed reference lock differs")
        profile = repository / "crates/mamba-quality/assets/profile.json"
        checked_bytes(profile.read_bytes(), lock["profile_sha256"])
        write_json(output / "run-identity.json", {
            "source_sha": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "runner_os": os.environ.get("RUNNER_OS"),
            "runner_arch": os.environ.get("RUNNER_ARCH"), "repository": str(repository),
            "lock_sha256": digest(lock_path.read_bytes()), "profile_sha256": digest(profile.read_bytes()),
            "source_files": {name: digest((repository / "tools/ci" / name).read_bytes()) for name in
                             ["q0_mypy_reference.py", "q0_mypy_inputs.py", "q0_ruff_reference.py"]}})
        download(lock["sdist"], output / "downloads/sdist")
        cases, proof = prepare_inputs(repository)
        if (len(cases) != CASE_COUNT or proof["case_count"] != CASE_COUNT
                or proof["original_input_sha256"] != lock["original_input_sha256"]
                or len(proof["published_sources"]) != 729):
            raise ValueError("fixed input proof differs")
        write_json(output / "input-proof.json", proof)
        write_json(output / "inputs.json", cases)
        cached = {}
        for minor, version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                python = getattr(args, f"python3{minor}")
                if not python.is_absolute():
                    raise ValueError("interpreter path must be absolute")
                pins, wheels = [], []
                for name, item in lock["dependencies"].items():
                    artifact = item["wheels"][str(minor)]
                    key = (artifact["filename"], artifact["sha256"])
                    if key not in cached:
                        destination = output / "downloads" / f"wheel-{len(cached):02}"
                        download(artifact, destination)
                        cached[key] = destination / artifact["filename"]
                    wheels.append(cached[key])
                    pins.append({"name": name, "version": item["version"], **artifact})
                executable = prepare(python, version, root, pins, wheels, lock["strict_flags"])
                targets[minor] = run_target(executable, repository, root, minor, cases, lock)
            except Exception as error:
                targets[minor] = {"status": "failed", "error_class": type(error).__name__, "message": str(error)}
                write_json(output / f"python3{minor}-failure.json", targets[minor])
    except Exception as error:
        common_error = {"error_class": type(error).__name__, "message": str(error)}
    for minor in VERSIONS:
        targets.setdefault(minor, {"status": "not_run", "reason": "common prerequisite failed"})
    summary = terminal_summary(targets, common_error)
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", inventory(output, exclude_venvs=True))
    print(json.dumps({"execution_passed": summary["execution_passed"], "issue_accepted": False}))
    return 0 if summary["execution_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
