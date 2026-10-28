#!/usr/bin/env python3
"""Prepare one exact Black reference interpreter on the hosted Mac only.

The retained archive and initial payload describe this fresh interpreter.
They do not recover an old environment or execute Black or Mamba.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess

import q0_pylint_python as shared_python
from q0_mypy_reference import command, require_python_identity
from q0_pylint_reference import HOST_CONTEXT, require_host_context
from q0_ruff_reference import digest, inventory, write_json


VERSIONS = ("3.12.11", "3.13.5", "3.14.0")
LOCK_SCHEMA = "mamba.q0.163.black-reference-lock.v1"

# Keep the accepted download, safe extraction and pin checks unchanged.
require_pin = shared_python.require_pin
download = shared_python.download
extract = shared_python.extract


def require_lock(lock: dict, version: str) -> dict:
    if version not in VERSIONS:
        raise ValueError("unsupported fixed Black Python patch")
    if (not isinstance(lock, dict) or lock.get("schema") != LOCK_SCHEMA
            or lock.get("version") != "26.10.0"
            or lock.get("python_versions") != list(VERSIONS)
            or not isinstance(lock.get("interpreters"), dict)
            or set(lock["interpreters"]) != set(VERSIONS)):
        raise ValueError("Black interpreter lock or target denominator differs")
    require_host_context(lock.get("platform"))
    for target in VERSIONS:
        pin = lock["interpreters"][target]
        if not isinstance(pin, dict) or type(pin.get("bytes")) is not int:
            raise ValueError("Black interpreter artifact pin has invalid fields")
        require_pin(target, pin)
    return lock["interpreters"][version]


def source_context(repository: Path) -> dict:
    source = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=repository, text=True).strip()
    if re.fullmatch(r"[0-9a-f]{40}", source) is None or source != os.environ.get("GITHUB_SHA"):
        raise ValueError("bootstrap source head differs from the GitHub run head")
    return {
        "source_sha": source,
        "run_id": os.environ.get("GITHUB_RUN_ID"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "helper_sha256": digest(Path(__file__).read_bytes()),
        "shared_python_helper_sha256": digest(Path(shared_python.__file__).read_bytes()),
        "host": {name: os.environ.get(name) for name in
                 ("RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion", "GITHUB_SHA")},
    }


def require_probe(identity: dict, version: str, executable: Path) -> None:
    require_python_identity(identity, version)
    if (identity.get("machine") != "arm64" or identity.get("sys_platform") != "darwin"
            or identity.get("executable") != str(executable)):
        raise ValueError("bootstrap interpreter path, architecture or OS differs")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--python-version", required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    summary = {
        "schema": "mamba.q0.163.black-python-bootstrap.v1",
        "passed": False, "historical_environment_recovered": False,
        "issue_accepted": False, "python_version": args.python_version,
        "reference_tool_executed": False, "native_tool_executed": False,
        "helper_sha256": digest(Path(__file__).read_bytes()),
        "run_id": os.environ.get("GITHUB_RUN_ID"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
    }
    try:
        require_host_context(HOST_CONTEXT)
        repository = args.repository.resolve()
        summary.update(source_context(repository))
        lock_path = repository / "tools/ci/q0_black_reference_lock.json"
        lock_raw = lock_path.read_bytes()
        summary["lock_sha256"] = digest(lock_raw)
        pin = require_lock(json.loads(lock_raw), args.python_version)
        summary["pin"] = pin
        archive = download(pin, output)
        prefix = args.prefix.resolve()
        members, installed = extract(archive, prefix)
        write_json(output / "archive-members.json", members)
        write_json(output / "installed-payload.json", installed)
        summary["archive_members"] = len(members)
        summary["initial_payload_entries"] = len(installed)
        minor = int(args.python_version.split(".")[1])
        executable = prefix / "python/bin" / f"python3.{minor}"
        if digest(executable.read_bytes()) != pin["executable_sha256"]:
            raise ValueError("interpreter executable differs from the fixed archive payload")
        probe = ("import json,platform,sys; print(json.dumps({'python':list(sys.version_info[:3]),"
                 "'executable':sys.executable,'isolated':sys.flags.isolated,"
                 "'machine':platform.machine(),'sys_platform':sys.platform}))")
        receipt = command([str(executable), "-I", "-B", "-c", probe], output,
                          output / "identity-probe", inputs={
                              "archive_sha256": pin["sha256"],
                              "executable_sha256": pin["executable_sha256"],
                              "initial_payload_sha256": digest((output / "installed-payload.json").read_bytes()),
                              "lock_sha256": digest(lock_raw),
                          })
        identity = json.loads((output / "identity-probe/stdout.raw").read_bytes())
        require_probe(identity, args.python_version, executable)
        summary.update(passed=True, executable=str(executable), prefix=str(prefix),
                       identity=identity, identity_receipt=receipt)
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as stream:
            stream.write(f"python-path={executable}\n")
    except Exception as error:
        summary["passed"] = False
        summary["error"] = {"type": type(error).__name__, "message": str(error)}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", inventory(output))
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
