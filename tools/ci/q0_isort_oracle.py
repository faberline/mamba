#!/usr/bin/env python3
"""Collect fresh isort API records. Fixture source is data, never executed."""
from __future__ import annotations

import argparse
import ast
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import warnings


def child_path(root: Path, name: str) -> Path:
    relative = Path(name)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError("case layout path must stay within its project")
    return root / relative


def materialize(case: dict, root: Path) -> None:
    root.mkdir(parents=True, exist_ok=False)
    for name in case.get("directories", []):
        child_path(root, name).mkdir(parents=True, exist_ok=True)
    for name, contents in case.get("files", {}).items():
        path = child_path(root, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("x", encoding="utf-8", newline="") as stream:
            stream.write(contents)


def api_case(case: dict, root: Path, minor: int, sort_code, config_factory) -> dict:
    """Record errors, skips and nonconvergence instead of relabelling them."""
    materialize(case, root)
    source = case["source"]
    source_hash = hashlib.sha256(source.encode()).hexdigest()
    if case.get("source_sha256", source_hash) != source_hash:
        raise ValueError("case source SHA mismatch")
    config = config_factory(profile="black", line_length=88, py_version=f"3{minor}",
                            src_paths=(root, root / "src"))
    syntax_hint = None
    syntax_error = None
    try:
        ast.parse(source, feature_version=(3, minor))
    except (SyntaxError, ValueError) as error:
        syntax_hint = str(error)
        syntax_error = {"class": type(error).__name__, "message": str(error)}
    result = {"id": case["id"], "source_sha256": source_hash,
              "case_root": str(root), "target_syntax_hint": syntax_hint,
              "target_syntax_error": syntax_error}
    calls = 0
    output = None
    try:
        calls += 1
        output = sort_code(source, config=config, extension=case.get("extension", "py"))
        calls += 1
        repeated = sort_code(output, config=config, extension=case.get("extension", "py"))
        result.update(output=output, repeat_output=repeated,
                      reference_converges=output == repeated, repeat_call_executed=True)
    except Exception as error:
        if type(error).__name__ == "FileSkipComment":
            result.update(output=source, reference_converges=True, reference_skipped=True,
                          repeat_call_executed=calls > 1)
        else:
            result["reference_error"] = type(error).__name__ + ": " + str(error)
            result.update(error_class=type(error).__name__, message=str(error),
                          repeat_call_executed=calls > 1)
            if output is not None:
                result["first_output_before_repeat_error"] = output
    return result


def checker_receipt(command: list[str], root: Path, source: Path,
                    project_file: Path, timeout: float = 60,
                    stdin: bytes | None = None) -> dict:
    """Keep a real reference process, including failed launches/timeouts."""
    source_before = source.read_bytes()
    def raw_file(name: str, contents: bytes) -> None:
        with (root / name).open("xb") as stream:
            stream.write(contents)
    raw_file("source-before.raw", source_before)
    raw_file("stdin.raw", stdin or b"")
    started = time.monotonic()
    result = {"argv": command, "cwd": str(root), "pid": None, "waited_pid": None,
              "wait_completed": False, "actual_exit": None, "timed_out": False,
              "launch_error": None, "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
              "source_sha256": hashlib.sha256(source_before).hexdigest(),
              "pyproject_sha256": hashlib.sha256(project_file.read_bytes()).hexdigest()}
    stdout = stderr = b""
    try:
        process = subprocess.Popen(command, cwd=root,
                                   stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONNOUSERSITE="1"),
                                   start_new_session=True)
        result["pid"] = process.pid
        try:
            stdout, stderr = process.communicate(input=stdin, timeout=timeout)
        except subprocess.TimeoutExpired:
            result["timed_out"] = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        result.update(waited_pid=process.pid, wait_completed=True, actual_exit=process.returncode)
    except OSError as error:
        result["launch_error"] = {"type": type(error).__name__, "message": str(error)}
    raw_file("stdout.raw", stdout)
    raw_file("stderr.raw", stderr)
    try:
        source_after = source.read_bytes()
        raw_file("source-after.raw", source_after)
        result.update(source_unchanged=source_before == source_after,
                      source_after_sha256=hashlib.sha256(source_after).hexdigest())
    except OSError as error:
        result.update(source_unchanged=False, source_after_sha256=None,
                      source_after_error={"type": type(error).__name__, "message": str(error)})
    result.update(
                  duration_seconds=time.monotonic() - started,
                  stdout_sha256=hashlib.sha256(stdout).hexdigest(),
                  stderr_sha256=hashlib.sha256(stderr).hexdigest(),
                  stdin_sha256=hashlib.sha256(stdin or b"").hexdigest())
    with (root / "receipt.json").open("x") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    return result


def checker_case(case: dict, root: Path, minor: int, isort) -> dict:
    """Use the retained independent CLI recipe, never an API exit projection."""
    root.mkdir(parents=True, exist_ok=False)
    source = root / "main.py"
    source.write_bytes(case["source"].encode())
    project = root / "pyproject.toml"
    project.write_text('[project]\nname="fixture"\nversion="0"\n'
                       f'requires-python=">=3.{minor}"\n')
    command = [sys.executable, "-I", "-B", "-m", "isort", "--profile", "black",
               "--line-length", "88", "--python-version", f"3{minor}",
               "--check-only", str(source)]
    result = checker_receipt(command, root, source, project)
    result.update(id=case["id"], version=isort.__version__, case_root=str(root))
    return result


EXCLUDED_API_CODE = """
import ast, json, pathlib, sys
import isort
from isort.settings import Config
request = json.load(sys.stdin)
config = Config(profile='black', line_length=88, py_version=request['version'],
                settings_path=request['root'], src_paths=[request['root']])
try:
    output = isort.code(request['source'], config=config,
                       file_path=pathlib.Path(request['file']))
    result = {'output':output}
except Exception as error:
    result = {'error_class':type(error).__name__, 'message':str(error)}
try:
    ast.parse(request['source'], feature_version=(3, sys.version_info.minor))
    result['target_syntax_error'] = None
except Exception as error:
    result['target_syntax_error'] = {'class':type(error).__name__, 'message':str(error)}
result['version'] = isort.__version__
print(json.dumps(result))
"""


def excluded_api_case(case: dict, root: Path, minor: int, isort) -> dict:
    """Run the distinct retained excluded API recipe in an actual child."""
    root.mkdir(parents=True, exist_ok=False)
    source = root / "main.py"
    source.write_bytes(case["source"].encode())
    project = root / "pyproject.toml"
    project.write_text('[project]\nname="fixture"\nversion="0"\n'
                       f'requires-python=">=3.{minor}"\n')
    request = {"source": case["source"], "version": f"3{minor}",
               "root": str(root), "file": str(source)}
    command = [sys.executable, "-I", "-B", "-c", EXCLUDED_API_CODE]
    receipt = checker_receipt(command, root, source, project,
                              stdin=json.dumps(request).encode())
    row = {**receipt, "id": case["id"], "case_root": str(root)}
    row["stderr_empty"] = (root / "stderr.raw").read_bytes() == b""
    row["protocol_passed"] = (row["wait_completed"] and not row["timed_out"]
                              and row["launch_error"] is None and row["actual_exit"] == 0
                              and row["pid"] > 0 and row["pid"] == row["waited_pid"]
                              and row["source_unchanged"] and row["stderr_empty"])
    try:
        result = json.loads((root / "stdout.raw").read_bytes())
        if not isinstance(result, dict) or result.get("version") != "9.0.2":
            raise ValueError("excluded API child identity/result differs")
        row.update(result)
    except (ValueError, UnicodeDecodeError) as error:
        row.update(protocol_passed=False, result_error=str(error))
    return row


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--python-version", required=True)
    parser.add_argument("--mode", choices=("api", "excluded-api", "checker"), default="api")
    args = parser.parse_args()
    if (os.environ.get("GITHUB_ACTIONS") != "true" or sys.platform != "linux"
            or os.environ.get("RUNNER_ARCH") != "X64"):
        raise RuntimeError("original execution requires the GitHub-hosted Linux X64 job")
    if list(sys.version_info[:3]) != [int(part) for part in args.python_version.split(".")]:
        raise ValueError("the original interpreter version differs from the pin")
    import isort
    from isort.settings import Config
    if isort.__version__ != "9.0.2":
        raise ValueError("the original isort version differs from the pin")
    cases = json.loads(args.cases.read_bytes())
    if not isinstance(cases, list) or not cases:
        raise ValueError("original input batch must be a nonempty array")
    args.root.mkdir(parents=True, exist_ok=False)
    warnings.simplefilter("ignore", SyntaxWarning)
    records = []
    for index, case in enumerate(cases):
        project = args.root / str(index)
        if args.mode == "api":
            records.append(api_case(case, project, sys.version_info.minor, isort.code, Config))
        elif args.mode == "excluded-api":
            try:
                records.append(excluded_api_case(case, project, sys.version_info.minor, isort))
            except Exception as error:
                records.append({"id": case["id"], "case_root": str(project),
                                "source_sha256": hashlib.sha256(case["source"].encode()).hexdigest(),
                                "actual_exit": None, "wait_completed": False,
                                "source_unchanged": False, "protocol_passed": False,
                                "setup_error": type(error).__name__ + ": " + str(error)})
        else:
            try:
                records.append(checker_case(case, project, sys.version_info.minor, isort))
            except Exception as error:
                records.append({"id": case["id"], "case_root": str(project),
                                "source_sha256": hashlib.sha256(case["source"].encode()).hexdigest(),
                                "actual_exit": None, "wait_completed": False,
                                "source_unchanged": False, "setup_error": type(error).__name__ + ": " + str(error)})
    json.dump(records, sys.stdout, ensure_ascii=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
