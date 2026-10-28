#!/usr/bin/env python3
"""Run the fixed Ruff reference inputs on a hosted Linux runner.

This creates new execution evidence. It does not recover historical evidence
or accept issue #159. No Mamba program or fixture program is executed.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import tomllib
from urllib.parse import urlsplit
from urllib.request import urlopen


VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
ALLOWED_HOSTS = {"files.pythonhosted.org", "static.crates.io"}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_json(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def checked_bytes(data: bytes, expected: str) -> bytes:
    if digest(data) != expected:
        raise ValueError("artifact SHA-256 mismatch")
    return data


def inventory(root: Path, *, exclude_venvs: bool = False) -> dict[str, dict]:
    return {
        str(path.relative_to(root)): {"bytes": path.stat().st_size,
                                     "sha256": digest(path.read_bytes())}
        for path in sorted(root.rglob("*")) if path.is_file()
        and not (exclude_venvs and len(path.relative_to(root).parts) > 1
                 and path.relative_to(root).parts[0] in {"python312", "python313", "python314"}
                 and path.relative_to(root).parts[1] == "venv")
    }


def run_command(argv: list[str], cwd: Path, output: Path, *,
                expected_exits: tuple[int, ...] = (0,), timeout: float = 300,
                inputs: dict | None = None) -> dict:
    """Retain a terminal receipt even if launch or execution fails."""
    output.mkdir(parents=True, exist_ok=False)
    write_json(output / "inputs.json", inputs or {})
    (output / "stdin.raw").write_bytes(b"")
    record = {"argv": argv, "cwd": str(cwd), "expected_exits": expected_exits,
              "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
              "pid": None, "waited_pid": None, "wait_completed": False,
              "actual_exit": None, "timed_out": False, "launch_error": None}
    started = time.monotonic()
    stdout = stderr = b""
    try:
        environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1",
                           PYTHONNOUSERSITE="1", RUFF_NO_CACHE="1",
                           PIP_CONFIG_FILE=os.devnull,
                           PIP_DISABLE_PIP_VERSION_CHECK="1")
        process = subprocess.Popen(argv, cwd=cwd, env=environment,
                                   stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        record["pid"] = process.pid
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            record["timed_out"] = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        record.update(waited_pid=process.pid, wait_completed=True,
                      actual_exit=process.returncode)
    except OSError as error:
        record["launch_error"] = {"type": type(error).__name__, "message": str(error)}
    (output / "stdout.raw").write_bytes(stdout)
    (output / "stderr.raw").write_bytes(stderr)
    record.update(duration_seconds=time.monotonic() - started,
                  stdout_sha256=digest(stdout), stderr_sha256=digest(stderr),
                  stdin_sha256=digest(b""), inputs_sha256=digest((output / "inputs.json").read_bytes()))
    record["passed"] = (record["wait_completed"] and not record["timed_out"]
                        and record["actual_exit"] in expected_exits)
    write_json(output / "receipt.json", record)
    return record


def checked_command(argv: list[str], cwd: Path, output: Path, **options) -> dict:
    result = run_command(argv, cwd, output, **options)
    if not result["passed"]:
        raise RuntimeError(f"command failed; receipt: {output / 'receipt.json'}")
    return result


def download(item: dict, output: Path) -> bytes:
    output.mkdir(parents=True, exist_ok=False)
    filename = item["filename"]
    if Path(filename).name != filename:
        raise ValueError("artifact filename must be one path component")
    record = {"url": item["url"], "expected_sha256": item["sha256"],
              "filename": filename, "new_download": True, "passed": False}
    try:
        for address in [item["url"]]:
            parts = urlsplit(address)
            if parts.scheme != "https" or parts.hostname not in ALLOWED_HOSTS:
                raise ValueError("artifact URL is outside the frozen public sources")
        with urlopen(item["url"], timeout=60) as response:
            final = urlsplit(response.geturl())
            if final.scheme != "https" or final.hostname not in ALLOWED_HOSTS:
                raise ValueError("artifact redirect is outside the frozen public sources")
            raw = response.read(50_000_001)
            if len(raw) > 50_000_000:
                raise ValueError("artifact exceeds the 50 MB bound")
            record.update(final_url=response.geturl(), http_status=response.status,
                          bytes=len(raw), actual_sha256=digest(raw))
        (output / filename).write_bytes(raw)
        checked_bytes(raw, item["sha256"])
        record["passed"] = True
        return raw
    except Exception as error:
        record["error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        write_json(output / "download.json", record)


def write_sources(root: Path, sources: dict[str, bytes]) -> None:
    root.mkdir(parents=True, exist_ok=False)
    for name, raw in sources.items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("unsafe input path")
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)


def read_messages(output: Path) -> list[dict]:
    messages = json.loads((output / "stdout.raw").read_bytes())
    if not isinstance(messages, list):
        raise ValueError("Ruff output is not a diagnostic array")
    return messages


def ruff(binary: Path, project: Path, names: list[str], minor: int,
         output: Path, *, fixing: bool = False) -> tuple[list[dict], dict]:
    from q0_ruff_inputs import SELECT
    command = [str(binary), "check", "--isolated", "--no-cache", "--no-preview",
               "--select", SELECT, "--target-version", f"py3{minor}",
               "--output-format", "json"]
    if fixing:
        command += ["--fix", "--no-unsafe-fixes"]
    command += [str(project / name) for name in names]
    current = {name: {"bytes": (project / name).stat().st_size,
                      "sha256": digest((project / name).read_bytes())} for name in names}
    receipt = checked_command(command, project, output, expected_exits=(0, 1), inputs=current)
    return read_messages(output), receipt


def prepare(python: Path, expected: str, root: Path, wheel: Path) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable}))"
    checked_command([str(python), "-I", "-c", probe], root, root / "base-identity")
    identity = json.loads((root / "base-identity/stdout.raw").read_bytes())
    if identity["python"] != list(map(int, expected.split("."))):
        raise ValueError(f"expected Python {expected}")
    checked_command([str(python), "-I", "-m", "venv", "--copies", str(root / "venv")],
                    root, root / "create-venv")
    executable = root / "venv/bin/python"
    checked_command([str(executable), "-I", "-m", "pip", "install", "--no-index",
                     "--no-deps", "--no-compile", str(wheel)], root, root / "install")
    binary = root / "venv/bin/ruff"
    checked_command([str(binary), "--version"], root, root / "ruff-version")
    if (root / "ruff-version/stdout.raw").read_bytes().strip() != b"ruff 0.16.10":
        raise ValueError("unexpected Ruff version")
    checked_command([str(executable), "-I", "-m", "pip", "list", "--format=json"],
                    root, root / "installed-distributions")
    installed = json.loads((root / "installed-distributions/stdout.raw").read_bytes())
    if not any(row["name"].lower() == "ruff" and row["version"] == "0.16.10" for row in installed):
        raise ValueError("pinned Ruff distribution is missing")
    identity.update(base_executable_sha256=digest(python.read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()),
                    ruff_binary_sha256=digest(binary.read_bytes()),
                    wheel_sha256=digest(wheel.read_bytes()), installed_distributions=installed,
                    historical_environment=False)
    write_json(root / "identity.json", identity)
    return binary


def coverage(messages: list[dict], project: Path, minor: int,
             rules: list[str], witnesses: dict, output: Path) -> None:
    by_file: dict[str, list[dict]] = {}
    for message in messages:
        name = str(Path(message["filename"]).relative_to(project))
        by_file.setdefault(name, []).append(message)
    rows, errors = [], []
    for code in rules:
        witness = witnesses.get(code)
        if code == "F621":
            witness = {"negative": "generated/F621.py", "positive": "controls/clean.py"}
        if witness is None:
            rows.append({"rule": code, "status": "unknown", "reason": "missing witness"})
            errors.append(code)
            continue
        negative, positive = witness["negative"], witness["positive"]
        hits = [m for m in by_file.get(negative, []) if m["code"] == code]
        positive_hits = [m for m in by_file.get(positive, []) if m["code"] == code]
        disposition = ("original-dormant" if code == "F621" else
                       "target-inactive" if minor < {"B911": 13, "B912": 14}.get(code, 12)
                       else "observed")
        passed = bool(hits) == (disposition == "observed") and not positive_hits
        rows.append({"rule": code, **witness, "disposition": disposition,
                     "diagnostics": len(hits), "positive_diagnostics": len(positive_hits),
                     "interaction_project": len({m["code"] for m in messages if m["code"]}) > 1,
                     "fix_applicabilities": sorted({m["fix"]["applicability"] for m in hits if m["fix"]}),
                     "status": "pass" if passed else "fail"})
        if not passed:
            errors.append(code)
    write_json(output, {"rules": rows, "denominator": len(rules),
                        "unknown": sum(r["status"] == "unknown" for r in rows),
                        "not_run": 0, "unsupported": 0, "errors": errors})
    if errors:
        raise ValueError(f"rule coverage failed: {errors}")


def run_target(binary: Path, sources: dict[str, bytes], minor: int,
               root: Path, rules: list[str], witnesses: dict) -> dict:
    project = root / "project"
    write_sources(project, sources)
    (project / "pyproject.toml").write_text(f"[project]\nrequires-python='>=3.{minor}'\n")
    names = sorted(sources)
    before, before_exit = ruff(binary, project, names, minor, root / "before")
    coverage(before, project, minor, rules, witnesses, root / "coverage.json")
    fixed_messages, fixed_exit = ruff(binary, project, names, minor, root / "fix-1", fixing=True)
    fixed = {name: (project / name).read_bytes() for name in names}
    write_sources(root / "fixed-inputs", fixed)
    repeated, repeat_exit = ruff(binary, project, names, minor, root / "fix-2", fixing=True)
    if fixed != {name: (project / name).read_bytes() for name in names}:
        raise ValueError("repeat safe fix changed input bytes")
    after, after_exit = ruff(binary, project, names, minor, root / "after")
    if repeated != fixed_messages or after != fixed_messages:
        raise ValueError("ordered repeated diagnostics differ")
    if repeat_exit["actual_exit"] != fixed_exit["actual_exit"] or after_exit["actual_exit"] != fixed_exit["actual_exit"]:
        raise ValueError("repeat or check exit differs from safe fix exit")
    clean, clean_exit = ruff(binary, project, ["controls/clean.py", "controls/clean.pyi"],
                             minor, root / "clean")
    if clean or clean_exit["actual_exit"] != 0:
        raise ValueError("clean controls failed")
    return {"status": "passed", "input_count": len(sources), "rule_count": len(rules),
            "before_diagnostics": len(before), "after_diagnostics": len(after),
            "changed_files": sum(fixed[n] != sources[n] for n in names),
            "exits": {"before": before_exit["actual_exit"], "fix-1": fixed_exit["actual_exit"],
                      "fix-2": repeat_exit["actual_exit"], "after": after_exit["actual_exit"],
                      "clean": clean_exit["actual_exit"]}, "safe_fix_converged": True}


def terminal_summary(targets: dict[int, dict]) -> dict:
    terminal = all(targets.get(m, {}).get("status") in {"passed", "failed", "not_run"} for m in VERSIONS)
    passed = terminal and all(targets[m]["status"] == "passed" for m in VERSIONS)
    return {"schema": "mamba.q0.ruff.hosted-reference.v1", "issue": 159,
            "targets": {VERSIONS[m]: targets.get(m, {"status": "not_run"}) for m in VERSIONS},
            "all_targets_terminal": terminal, "execution_passed": passed,
            "issue_accepted": False, "historical_original_evidence_recovered": False}


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
    common_error = None
    try:
        from q0_ruff_inputs import baseline, expanded, verify_frozen_inputs
        if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != "linux"
                or os.environ.get("RUNNER_ARCH") != "X64"):
            raise ValueError("reference execution requires the GitHub-hosted Linux X64 job")
        lock_file = Path(__file__).with_name("q0_ruff_reference_lock.json")
        lock = json.loads(lock_file.read_bytes())
        profile_file = repository / "crates/mamba-quality/assets/profile.json"
        checked_bytes(profile_file.read_bytes(), lock["profile_sha256"])
        if lock["python_versions"] != list(VERSIONS.values()):
            raise ValueError("Python target lock differs")
        cargo_lock = tomllib.loads((repository / "Cargo.lock").read_text())
        fixture_package = [row for row in cargo_lock["package"]
                           if row["name"] == "ruff_linter" and row["version"] == "0.16.10"]
        if len(fixture_package) != 1 or fixture_package[0]["checksum"] != lock["fixtures"]["sha256"]:
            raise ValueError("fixture archive differs from the current Cargo lock")
        rules = [row["code"] for row in json.loads(profile_file.read_bytes())["ruff"]["rules"]]
        if len(rules) != 174 or len(set(rules)) != 174:
            raise ValueError("fixed profile rule denominator differs")
        write_json(output / "run-identity.json", {"source_sha": os.environ.get("GITHUB_SHA"),
                   "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
                   "runner_os": os.environ.get("RUNNER_OS"), "runner_arch": os.environ.get("RUNNER_ARCH"),
                   "lock_sha256": digest(lock_file.read_bytes()), "profile_sha256": digest(profile_file.read_bytes()),
                   "runner_source_sha256": digest(Path(__file__).read_bytes()),
                   "helper_source_sha256": digest(Path(__file__).with_name("q0_ruff_inputs.py").read_bytes())})
        artifacts = {name: download(lock[name], output / "downloads" / name)
                     for name in ("sdist", "fixtures", "wheel")}
        sources = baseline(artifacts["fixtures"])
        if len(sources) != 596:
            raise ValueError("baseline input denominator differs")
        binaries, raw_reports, project_roots = {}, {}, {}
        wheel = output / "downloads/wheel" / lock["wheel"]["filename"]
        for minor, version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                python = getattr(args, f"python3{minor}")
                if not python.is_absolute():
                    raise ValueError("interpreter path must be absolute")
                binary = prepare(python, version, root, wheel)
                project = root / "baseline-project"
                write_sources(project, sources)
                project_roots[minor] = project
                raw_reports[minor], _ = ruff(binary, project, sorted(sources), minor, root / "baseline")
                binaries[minor] = binary
            except Exception as error:
                targets[minor] = {"status": "failed", "phase": "baseline", "error": str(error)}
        if len(binaries) != len(VERSIONS):
            raise ValueError("at least one target has no complete baseline")
        sources, witnesses = expanded(sources, raw_reports, project_roots, rules)
        if len(sources) != 784:
            raise ValueError("full input denominator differs")
        proof = verify_frozen_inputs(repository, sources)
        write_json(output / "frozen-input-proof.json", proof)
        write_sources(output / "inputs", sources)
        write_json(output / "input-manifest.json", inventory(output / "inputs"))
        for minor in VERSIONS:
            try:
                targets[minor] = run_target(binaries[minor], sources, minor, output / f"python3{minor}",
                                            rules, witnesses[minor])
            except Exception as error:
                targets[minor] = {"status": "failed", "phase": "full-corpus", "error": str(error)}
    except Exception as error:
        common_error = {"type": type(error).__name__, "message": str(error)}
    for minor in VERSIONS:
        targets.setdefault(minor, {"status": "not_run", "reason": "common prerequisite failed"})
    summary = terminal_summary(targets)
    summary["common_error"] = common_error
    stages = ("before", "fix-1", "fix-2", "after", "clean")
    completed = 0
    for minor in VERSIONS:
        for stage in stages:
            receipt = output / f"python3{minor}" / stage / "receipt.json"
            if receipt.is_file() and json.loads(receipt.read_bytes())["wait_completed"]:
                completed += 1
    summary.update(expected_reference_commands=15, completed_reference_commands=completed,
                   artifact_exclusions=["python3*/venv/**"])
    summary["execution_passed"] = summary["execution_passed"] and completed == 15
    write_json(output / "summary.json", summary)
    write_json(output / "files.json", inventory(output, exclude_venvs=True))
    print(json.dumps(summary))
    return 0 if summary["execution_passed"] and common_error is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
