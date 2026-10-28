#!/usr/bin/env python3
"""Record fixed isort originals on hosted Linux; never accept an issue."""
from __future__ import annotations

import argparse
import inspect
import json
import os
from pathlib import Path
import sys

from q0_ruff_reference import checked_bytes, checked_command, digest, download, inventory, write_json

VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}


def validate_dependencies(pins: list[dict], distributions: dict[str, dict],
                          environment: dict | None = None) -> dict:
    """Validate only pinned runtime distributions and their active requirements."""
    from pip._vendor.packaging.markers import default_environment
    from pip._vendor.packaging.requirements import Requirement
    from pip._vendor.packaging.utils import canonicalize_name

    locked = {"isort": "9.0.2"}
    for pin in pins:
        name = canonicalize_name(pin["name"])
        if name in locked:
            raise ValueError(f"duplicate dependency pin: {name}")
        locked[name] = pin["version"]
    installed = {canonicalize_name(name): row for name, row in distributions.items()}
    if len(installed) != len(distributions):
        raise ValueError("duplicate installed dependency identity")
    for name, version in locked.items():
        if name not in installed:
            raise ValueError(f"pinned dependency is missing: {name}")
        if installed[name]["version"] != version:
            raise ValueError(f"installed dependency version differs from pin: {name}")
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
import isort
from pip._vendor.packaging.utils import canonicalize_name
pins = json.loads(sys.argv[1])
distributions = {}
for name in ['isort'] + [pin['name'] for pin in pins]:
    d = importlib.metadata.distribution(name)
    files, missing = {}, []
    for relative in d.files or []:
        p = pathlib.Path(d.locate_file(relative))
        if p.is_file():
            raw = p.read_bytes()
            files[str(relative)] = {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
        else:
            missing.append(str(relative))
    distributions[canonicalize_name(name)] = {
        'name':d.metadata['Name'], 'version':d.version, 'requires_dist':d.requires or [],
        'distribution_files':files, 'missing_distribution_files':missing}
verification = validate_dependencies(pins, distributions)
original = distributions['isort']
module = pathlib.Path(isort.__file__)
print(json.dumps({'python':list(sys.version_info[:3]), 'executable':sys.executable,
    'version':isort.__version__, 'module':str(module),
    'module_sha256':hashlib.sha256(module.read_bytes()).hexdigest(),
    'requires_dist':original['requires_dist'],
    'distribution_files':original['distribution_files'],
    'missing_distribution_files':original['missing_distribution_files'],
    'dependency_distributions':{name:row for name,row in distributions.items() if name!='isort'},
    'dependency_verification':verification}))
"""


def prepare(python: Path, expected: str, root: Path, wheel: Path,
            dependency_pins: list[dict], dependency_wheels: list[Path]) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable}))"
    checked_command([str(python), "-I", "-B", "-c", probe], root, root / "base-identity")
    base = json.loads((root / "base-identity/stdout.raw").read_bytes())
    if base["python"] != [int(part) for part in expected.split(".")]:
        raise ValueError(f"expected Python {expected}")
    checked_command([str(python), "-I", "-B", "-m", "venv", "--copies", str(root / "venv")],
                    root, root / "create-venv")
    executable = root / "venv/bin/python"
    if len(dependency_pins) != len(dependency_wheels):
        raise ValueError("dependency artifact denominator differs")
    for pin, path in zip(dependency_pins, dependency_wheels):
        checked_bytes(path.read_bytes(), pin["sha256"])
    checked_command([str(executable), "-I", "-B", "-m", "pip", "install", "--no-index",
                     "--no-deps", "--no-compile", str(wheel),
                     *[str(path) for path in dependency_wheels]], root, root / "install")
    checked_command([str(executable), "-I", "-B", "-c", PACKAGE_PROBE,
                     json.dumps(dependency_pins)], root,
                    root / "package-identity")
    identity = json.loads((root / "package-identity/stdout.raw").read_bytes())
    if identity["version"] != "9.0.2" or identity["python"] != base["python"]:
        raise ValueError("installed original identity differs")
    if identity["missing_distribution_files"] or not identity["distribution_files"]:
        raise ValueError("installed original distribution payload is incomplete")
    for name, dependency in identity["dependency_distributions"].items():
        if dependency["missing_distribution_files"] or not dependency["distribution_files"]:
            raise ValueError(f"installed dependency payload is incomplete: {name}")
    if not identity["dependency_verification"]["dependencies_verified"]:
        raise ValueError("installed dependencies were not verified")
    checked_command([str(executable), "-I", "-B", "-m", "pip", "list", "--format=json"],
                    root, root / "installed-distributions")
    installed = json.loads((root / "installed-distributions/stdout.raw").read_bytes())
    if not any(row["name"].lower() == "isort" and row["version"] == "9.0.2" for row in installed):
        raise ValueError("pinned isort distribution is missing")
    identity.update(base_executable_sha256=digest(python.read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()),
                    wheel_sha256=digest(wheel.read_bytes()), installed_distributions=installed,
                    dependency_artifacts=[{"name":pin["name"], "version":pin["version"],
                                           "filename":path.name, "sha256":digest(path.read_bytes())}
                                          for pin,path in zip(dependency_pins, dependency_wheels)],
                    historical_environment=False)
    write_json(root / "identity.json", identity)
    return executable


def checked_rows(output: Path, cases: list[dict]) -> list[dict]:
    rows = json.loads((output / "stdout.raw").read_bytes())
    if not isinstance(rows, list) or len(rows) != len(cases):
        raise ValueError("original batch result denominator differs")
    if [row.get("id") for row in rows] != [case["id"] for case in cases]:
        raise ValueError("original batch case identity or order differs")
    return rows


def oracle(executable: Path, cases: list[dict], packet: Path, project: Path,
           version: str, mode: str, output: Path) -> list[dict]:
    driver = Path(__file__).with_name("q0_isort_oracle.py")
    command = [str(executable), "-I", "-B", str(driver), "--cases", str(packet),
               "--root", str(project), "--python-version", version, "--mode", mode]
    inputs = {"packet": {"path": str(packet), "bytes": packet.stat().st_size,
                         "sha256": digest(packet.read_bytes())},
              "oracle": {"path": str(driver), "sha256": digest(driver.read_bytes())},
              "case_count": len(cases)}
    checked_command(command, output.parent, output, inputs=inputs, timeout=600)
    return checked_rows(output, cases)


def terminal_summary(targets: dict[int, dict], api_verification: dict | None,
                     checker_verification: dict | None = None,
                     excluded_api_verification: dict | None = None) -> dict:
    terminal = all(targets.get(minor, {}).get("status") in {"passed", "failed", "not_run"}
                   for minor in VERSIONS)
    passed = (terminal and api_verification is not None
              and api_verification.get("reference_comparisons_passed") is True
              and checker_verification is not None
              and checker_verification.get("checker_comparisons_passed") is True
              and excluded_api_verification is not None
              and excluded_api_verification.get("excluded_api_comparisons_passed") is True
              and all(targets[minor]["status"] == "passed" for minor in VERSIONS))
    return {"schema": "mamba.q0.isort.hosted-reference.v1", "issue": 160,
            "targets": {VERSIONS[minor]: targets.get(minor, {"status": "not_run"}) for minor in VERSIONS},
            "all_targets_terminal": terminal, "execution_passed": passed,
            "issue_accepted": False, "historical_original_evidence_recovered": False,
            "whole_suite_accepted": False}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    for minor in VERSIONS:
        parser.add_argument(f"--python3{minor}", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    repository = Path(__file__).resolve().parents[2]
    targets: dict[int, dict] = {}
    results: dict[int, list[dict]] = {}
    checker_results: dict[int, list[dict]] = {}
    excluded_api_results: dict[int, list[dict]] = {}
    verification = None
    checker_verification = None
    excluded_api_verification = None
    common_error = None
    try:
        from q0_isort_inputs import construct, verify_checker_results, verify_excluded_api_results, verify_results
        if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != "linux"
                or os.environ.get("RUNNER_ARCH") != "X64"):
            raise ValueError("original execution requires the GitHub-hosted Linux X64 job")
        lock_path = Path(__file__).with_name("q0_isort_reference_lock.json")
        lock = json.loads(lock_path.read_bytes())
        if lock["python_versions"] != list(VERSIONS.values()) or lock["version"] != "9.0.2":
            raise ValueError("version lock differs")
        profile = repository / "crates/mamba-quality/assets/profile.json"
        checked_bytes(profile.read_bytes(), lock["profile_sha256"])
        write_json(output / "run-identity.json", {
            "source_sha": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "runner_os": os.environ.get("RUNNER_OS"), "runner_arch": os.environ.get("RUNNER_ARCH"),
            "lock_sha256": digest(lock_path.read_bytes()), "profile_sha256": digest(profile.read_bytes()),
            "runner_source_sha256": digest(Path(__file__).read_bytes()),
            "oracle_source_sha256": digest(Path(__file__).with_name("q0_isort_oracle.py").read_bytes()),
            "helper_source_sha256": digest(Path(__file__).with_name("q0_isort_inputs.py").read_bytes())})
        sdist = download(lock["sdist"], output / "downloads/sdist")
        dependency_pins = lock["dependencies"]
        if not isinstance(dependency_pins, list):
            raise ValueError("dependency lock must be a list")
        dependency_wheels = []
        for index, pin in enumerate(dependency_pins):
            destination = output / "downloads" / f"dependency-{index}"
            download(pin, destination)
            dependency_wheels.append(destination / pin["filename"])
        cases, proof = construct(sdist, repository)
        if len(cases) != lock["expected_candidates"] or len({row["id"] for row in cases}) != 1355:
            raise ValueError("candidate denominator differs")
        if len(proof["accepted_ids"]) != lock["accepted"] or len(proof["excluded_ids"]) != lock["excluded"]:
            raise ValueError("accepted/excluded denominator differs")
        write_json(output / "input-proof.json", proof)
        packet = output / "inputs.json"
        write_json(packet, cases)
        excluded = [case for case in cases if case["id"] in set(proof["excluded_ids"])]
        excluded_packet = output / "excluded-inputs.json"
        write_json(excluded_packet, excluded)
        executables = {}
        for minor, version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                item = lock["wheels"][f"3{minor}"]
                download(item, output / "downloads" / f"wheel3{minor}")
                wheel = output / "downloads" / f"wheel3{minor}" / item["filename"]
                python = getattr(args, f"python3{minor}")
                if not python.is_absolute():
                    raise ValueError("interpreter path must be absolute")
                executable = prepare(python, version, root, wheel, dependency_pins, dependency_wheels)
                results[minor] = oracle(executable, cases, packet, root / "api-projects", version,
                                        "api", root / "api")
                executables[minor] = executable
                targets[minor] = {"status": "api-recorded", "api_candidate_rows": len(results[minor])}
            except Exception as error:
                targets[minor] = {"status": "failed", "phase": "api", "error": str(error)}
        if len(results) != 3:
            raise ValueError("one or more original API batches did not finish")
        verification = verify_results(repository, cases, results)
        write_json(output / "api-verification.json", verification)
        if verification.get("reference_comparisons_passed") is not True:
            raise ValueError("frozen original API comparisons did not pass")
        for minor, version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                excluded_api_results[minor] = oracle(executables[minor], excluded, excluded_packet,
                                                     root / "excluded-api-projects", version,
                                                     "excluded-api", root / "excluded-api")
            except Exception as error:
                targets[minor] = {"status": "failed", "phase": "excluded-api", "error": str(error)}
        excluded_api_verification = verify_excluded_api_results(repository, excluded, excluded_api_results)
        write_json(output / "excluded-api-verification.json", excluded_api_verification)
        if excluded_api_verification.get("excluded_api_comparisons_passed") is not True:
            raise ValueError("frozen excluded-source API comparisons did not pass")
        for minor, version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                rows = oracle(executables[minor], excluded, excluded_packet,
                              root / "checker-projects", version, "checker", root / "checker")
                checker_results[minor] = rows
                targets[minor] = {"status": "checker-recorded", "checker_rows": len(rows)}
            except Exception as error:
                targets[minor] = {"status": "failed", "phase": "checker", "error": str(error)}
        write_json(output / "checker-results.json", checker_results)
        checker_verification = verify_checker_results(repository, excluded, checker_results)
        write_json(output / "checker-verification.json", checker_verification)
        if checker_verification.get("checker_comparisons_passed") is not True:
            raise ValueError("frozen independent checker comparisons did not pass")
        for minor in VERSIONS:
            if targets.get(minor, {}).get("status") == "checker-recorded":
                targets[minor] = {"status": "passed", "api_candidate_rows": len(results[minor]),
                                  "excluded_api_rows": len(excluded_api_results[minor]),
                                  "checker_rows": len(checker_results[minor]),
                                  "checker_actual_exits": {str(code): sum(row["actual_exit"] == code
                                                                       for row in checker_results[minor])
                                                           for code in (0, 1)}}
    except Exception as error:
        common_error = {"type": type(error).__name__, "message": str(error)}
    for minor in VERSIONS:
        targets.setdefault(minor, {"status": "not_run", "reason": "common prerequisite failed"})
        if targets[minor]["status"] in {"api-recorded", "checker-recorded"}:
            targets[minor] = {"status": "failed", "phase": "required-verification",
                              "error": "required comparison did not complete"}
    summary = terminal_summary(targets, verification, checker_verification, excluded_api_verification)
    completed_checkers = sum(bool(row.get("wait_completed") is True and not row.get("timed_out")
                                 and row.get("pid") and row.get("pid") == row.get("waited_pid"))
                            for rows in checker_results.values() for row in rows)
    completed_excluded_api = sum(row.get("protocol_passed") is True
                                 for rows in excluded_api_results.values() for row in rows)
    summary.update(common_error=common_error, expected_api_processes=603,
                   completed_api_processes=len(results) + completed_excluded_api,
                   completed_main_api_processes=len(results),
                   completed_excluded_api_processes=completed_excluded_api,
                   expected_excluded_api_batches=3,
                   completed_excluded_api_batches=len(excluded_api_results),
                   expected_checker_batches=3,
                   completed_checker_batches=sum((output / f"python3{m}/checker/receipt.json").is_file()
                                                  for m in VERSIONS),
                   expected_checker_processes=600, completed_checker_processes=completed_checkers,
                   expected_reference_processes=1203,
                   completed_reference_processes=len(results) + completed_excluded_api + completed_checkers,
                   artifact_exclusions=["python3*/venv/**"])
    summary["execution_passed"] = (summary["execution_passed"] and completed_checkers == 600
                                   and completed_excluded_api == 600 and len(results) == 3)
    write_json(output / "summary.json", summary)
    write_json(output / "files.json", inventory(output, exclude_venvs=True))
    print(json.dumps(summary))
    return 0 if summary["execution_passed"] and common_error is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
