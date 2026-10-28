#!/usr/bin/env python3
"""Retain three complete hosted Black API batches without issue acceptance."""
from __future__ import annotations

import argparse
import base64
import csv
import datetime as dt
import hashlib
import inspect
import io
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import zipfile

from q0_mypy_reference import command, require_python_identity, require_receipt, validate_dependencies
from q0_pylint_reference import HOST_CONTEXT, require_host_context
from q0_ruff_reference import checked_bytes, digest, download, inventory, write_json

VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
CASE_COUNT = 4172
PINS = {"black": "26.10.0", "click": "8.5.0", "mypy_extensions": "1.1.0", "packaging": "26.3",
        "pathspec": "1.1.1", "platformdirs": "4.12.3", "pytokens": "0.4.1"}

# This observer is independently written from the public Black API and the
# verified recipe. Its stdout contains only the original ordered record schema.
OBSERVER = r'''
import ast
import hashlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path
import re
import site
import sys
import tempfile
import warnings

warnings.filterwarnings("ignore", category=SyntaxWarning)
sidecar = Path(__SIDECAR__)
expected_prefix = Path(__VENV__).resolve()
if Path(sys.prefix).resolve() != expected_prefix or site.ENABLE_USER_SITE:
    raise RuntimeError("main interpreter is outside the isolated target venv")
if os.environ.get("PYTHONPATH") or os.environ.get("PYTHONHOME"):
    raise RuntimeError("inherited Python import path is contaminated")
spec = importlib.util.find_spec("black")
if spec is None or spec.origin is None or not Path(spec.origin).resolve().is_relative_to(expected_prefix):
    raise RuntimeError("Black import would leave the target venv")
import black
if importlib.metadata.version("black") != "26.10.0":
    raise RuntimeError("Black version differs from the original pin")
minor = sys.version_info.minor
target = getattr(black.TargetVersion, "PY3" + str(minor))
cases = json.load(sys.stdin)
results = []
log_rows = []
log_errors = []
canonical_temp = Path(tempfile.gettempdir()).resolve()

def retain_referenced_logs(error, case, index):
    message = str(error)
    for marker in ("This diff might be helpful: ", "This invalid output might be helpful: "):
        if marker not in message:
            continue
        reported = message.rsplit(marker, 1)[1]
        try:
            path = Path(reported)
            if not path.is_absolute() or not re.fullmatch(r"blk_[A-Za-z0-9_]+\.log", path.name):
                raise ValueError("referenced Black log name is invalid")
            resolved = path.resolve(strict=True)
            if resolved.parent != canonical_temp or not resolved.is_file():
                raise ValueError("referenced Black log is outside the current canonical temp parent")
            raw = resolved.read_bytes()
            name = "case_%04d-log-%d.raw" % (index, len(log_rows))
            with (sidecar / "referenced-error-logs" / name).open("xb") as stream:
                stream.write(raw)
            log_rows.append({"index":index, "id":case["id"], "error_class":type(error).__name__,
                "marker":marker, "reported_path":reported, "canonical_path":str(resolved),
                "canonical_temp_parent":str(canonical_temp), "saved_file":"referenced-error-logs/"+name,
                "bytes":len(raw), "sha256":hashlib.sha256(raw).hexdigest()})
        except Exception as failure:
            log_errors.append({"index":index, "id":case["id"], "reported_path":reported,
                               "error_class":type(failure).__name__, "message":str(failure)})

for index, case in enumerate(cases):
    source = case["source"]
    mode = black.Mode(target_versions={target}, line_length=88, string_normalization=True,
                      magic_trailing_comma=True, preview=False, unstable=False,
                      is_pyi=case.get("extension") == "pyi")
    hint = None
    try:
        ast.parse(source, feature_version=(3, minor))
    except (SyntaxError, ValueError) as error:
        hint = str(error)
    try:
        decoded, encoding, ending = black.decode_bytes(source.encode(), mode)
        try:
            output = black.format_file_contents(decoded, fast=False, mode=mode).replace("\n", ending)
        except black.NothingChanged:
            output = source
        decoded_again, encoding_again, ending_again = black.decode_bytes(output.encode(), mode)
        try:
            second = black.format_file_contents(decoded_again, fast=False, mode=mode).replace("\n", ending_again)
        except black.NothingChanged:
            second = output
        results.append({"output":output, "sha256":hashlib.sha256(output.encode()).hexdigest(),
                        "reference_converges":second == output, "target_syntax_hint":hint,
                        "reference_check_exit":int(source != output), "newline":ending})
    except Exception as error:
        results.append({"reference_error":type(error).__name__ + ": " + str(error),
                        "target_syntax_hint":hint, "reference_check_exit":123})
        retain_referenced_logs(error, case, index)

modules = {}
stdlib_root = Path(sys.base_prefix) / "lib" / ("python%d.%d" % sys.version_info[:2])
for name, module in sorted(sys.modules.items()):
    filename = getattr(module, "__file__", None)
    if filename is None:
        continue
    path = Path(filename).resolve()
    if not path.is_relative_to(expected_prefix):
        if path.is_relative_to(stdlib_root) and not path.is_relative_to(stdlib_root / "site-packages"):
            continue
        raise RuntimeError("main imported code outside its venv and pinned standard library: " + name)
    raw = path.read_bytes()
    modules[name] = {"path":str(path), "bytes":len(raw), "sha256":hashlib.sha256(raw).hexdigest()}
metadata = {"python":list(sys.version_info[:3]), "executable":sys.executable,
            "prefix":sys.prefix, "base_prefix":sys.base_prefix, "cwd":os.getcwd(),
            "flags":{name:getattr(sys.flags,name) for name in ("isolated","ignore_environment","no_user_site","dont_write_bytecode","utf8_mode")},
            "sys_path":sys.path, "user_site_enabled":site.ENABLE_USER_SITE,
            "canonical_temp_parent":str(canonical_temp), "modules":modules,
            "referenced_logs":log_rows, "log_capture_errors":log_errors,
            "case_count":len(cases), "result_count":len(results), "black_version":black.__version__}
with (sidecar / "observer-environment.json").open("x", encoding="utf-8") as stream:
    json.dump(metadata, stream, ensure_ascii=True, indent=2)
    stream.write("\n")
json.dump(results, sys.stdout, ensure_ascii=True)
'''


def observer_source(sidecar: Path, venv: Path) -> str:
    return OBSERVER.replace("__SIDECAR__", repr(str(sidecar))).replace("__VENV__", repr(str(venv)))


INSTALLER = """import runpy,sys
wheel=sys.argv[1]
sys.path.insert(0,wheel)
sys.argv=['pip',*sys.argv[2:]]
runpy.run_module('pip',run_name='__main__')
"""

# The target has no installed pip. Its pinned packaging dependency provides
# the same public Requirement/marker/version APIs used by the shared validator.
PACKAGE_PROBE = inspect.getsource(validate_dependencies).replace("from pip._vendor.packaging", "from packaging") + r'''
import base64, hashlib, importlib, importlib.machinery, importlib.metadata, json, pathlib, sys
from packaging.utils import canonicalize_name
pins = json.loads(sys.argv[1])
distributions = {}
for pin in pins:
    name = canonicalize_name(pin['name'])
    try:
        distribution = importlib.metadata.distribution(name)
    except importlib.metadata.PackageNotFoundError:
        distributions[name] = {'name':pin['name'],'missing_distribution':True}
        continue
    files, missing, records, native = {}, [], {}, {}
    for relative in distribution.files or []:
        path = pathlib.Path(distribution.locate_file(relative))
        if not path.resolve().is_relative_to(pathlib.Path(sys.prefix).resolve()) or not path.is_file():
            missing.append(str(relative))
            continue
        raw = path.read_bytes()
        fact = {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
        files[str(relative)] = fact
        if str(relative).endswith('.dist-info/RECORD'):
            records[str(relative)] = {**fact,'bytes_b64':base64.b64encode(raw).decode('ascii')}
        if any(str(relative).endswith(suffix) for suffix in importlib.machinery.EXTENSION_SUFFIXES):
            native[str(relative)] = fact
    metadata_path = pathlib.Path(distribution._path).resolve()
    distributions[name] = {'name':distribution.metadata['Name'], 'version':distribution.version,
        'metadata_path':str(metadata_path), 'requires_dist':distribution.requires or [],
        'requires_python':distribution.metadata.get('Requires-Python'),
        'distribution_files':files,'missing_distribution_files':missing,
        'installed_records':records,'native_files':native}
try:
    verification = validate_dependencies(pins,distributions)
except Exception as error:
    verification = {'dependencies_verified':False,'error_class':type(error).__name__,'message':str(error)}
modules = {}
for name in [pin['name'] for pin in pins]:
    module = importlib.import_module(name)
    path = pathlib.Path(module.__file__).resolve()
    raw = path.read_bytes()
    modules[name] = {'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
site_packages = pathlib.Path(importlib.metadata.distribution('black')._path).parent
installed = {canonicalize_name(d.metadata['Name']):d.version for d in importlib.metadata.distributions(path=[str(site_packages)])}
print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated,
    'prefix':sys.prefix,'base_prefix':sys.base_prefix,'distributions':distributions,'modules':modules,
    'installed_site_distributions':installed,'dependency_verification':verification}))
'''

SAFE_ENV = {"PATH", "LANG", "LC_ALL", "LC_CTYPE", "LC_COLLATE", "LC_MESSAGES", "TZ", "TMPDIR", "TMP", "TEMP",
            "PYTHONPATH", "PYTHONHOME", "PYTHONUTF8", "PYTHONIOENCODING", "PYTHONDONTWRITEBYTECODE",
            "PYTHONNOUSERSITE", "PYTHONSAFEPATH", "PYTHONHASHSEED", "PYTHONPYCACHEPREFIX", "PYTHONUSERBASE",
            "PYTHONWARNINGS", "PYTHONBREAKPOINT", "PYTHONMALLOC", "PYTHONDEVMODE", "RUNNER_OS", "RUNNER_ARCH",
            "RUNNER_TEMP", "GITHUB_WORKSPACE", "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"}


def environment_snapshot(environment: dict[str, str] | None = None) -> dict:
    environment = dict(os.environ) if environment is None else environment
    public, private = {}, {}
    for name, value in sorted(environment.items()):
        if name in SAFE_ENV:
            public[name] = value
        else:
            raw = value.encode("utf-8", "surrogateescape")
            private[name] = {"bytes":len(raw), "sha256":digest(raw)}
    return {"inheritance":"unmodified; no Popen env argument", "names":sorted(environment),
            "allowlisted_values":public, "private_value_identities":private}


def run_inherited(argv: list[str], stdin: bytes, output: Path, inputs: dict, timeout: float = 300) -> dict:
    """Unlike package commands, the original main child inherits cwd and env."""
    output.mkdir(parents=True, exist_ok=False)
    write_json(output / "inputs.json", inputs)
    (output / "stdin.raw").write_bytes(stdin)
    write_json(output / "environment-before.json", environment_snapshot())
    record = {"argv":argv, "cwd":os.getcwd(), "expected_exits":[0], "timeout_seconds":timeout,
              "environment_mode":"inherited", "cwd_mode":"inherited", "pid":None, "waited_pid":None,
              "wait_completed":False, "actual_exit":None, "timed_out":False, "launch_error":None,
              "started_utc":dt.datetime.now(dt.timezone.utc).isoformat()}
    started, stdout, stderr = time.monotonic(), b"", b""
    try:
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        record["pid"] = process.pid
        try:
            stdout, stderr = process.communicate(input=stdin, timeout=timeout)
        except subprocess.TimeoutExpired:
            record["timed_out"] = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        record.update(waited_pid=process.pid, wait_completed=True, actual_exit=process.returncode)
    except OSError as error:
        record["launch_error"] = {"error_class":type(error).__name__, "message":str(error)}
    (output / "stdout.raw").write_bytes(stdout)
    (output / "stderr.raw").write_bytes(stderr)
    write_json(output / "environment-after.json", environment_snapshot())
    record.update(duration_seconds=time.monotonic()-started, stdin_sha256=digest(stdin), stdout_sha256=digest(stdout),
                  stderr_sha256=digest(stderr), inputs_sha256=digest((output / "inputs.json").read_bytes()),
                  environment_before_sha256=digest((output / "environment-before.json").read_bytes()),
                  environment_after_sha256=digest((output / "environment-after.json").read_bytes()))
    record["passed"] = (record["wait_completed"] and not record["timed_out"] and record["launch_error"] is None
                         and record["actual_exit"] == 0)
    write_json(output / "receipt.json", record)
    return record


def require_pins(pins: list[dict]) -> None:
    if len(pins) != 7 or {pin["name"]:pin["version"] for pin in pins} != PINS:
        raise ValueError("Black runtime pins differ")


def bundled_pip(python: Path, version: str) -> Path:
    base = python.resolve(strict=True).parent.parent
    wheels = list((base / "lib" / ("python" + ".".join(version.split(".")[:2])) / "ensurepip/_bundled").glob("pip-*.whl"))
    if len(wheels) != 1:
        raise ValueError("exact PBS bundled installer wheel is missing or ambiguous")
    return wheels[0]


def require_venv_link(executable: Path, base: Path) -> dict:
    if not executable.is_symlink() or executable.resolve(strict=True) != base.resolve(strict=True):
        raise ValueError("Black venv must link to the exact pinned base")
    return {"venv_creation_mode":"symlinks-without-pip", "venv_executable_is_symlink":True,
            "venv_executable_resolved":str(executable.resolve(strict=True)),
            "base_executable_resolved":str(base.resolve(strict=True))}


def require_package_identity(identity: dict, version: str, pins: list[dict], venv: Path) -> None:
    require_pins(pins)
    require_python_identity(identity, version)
    expected = {name.replace("_", "-"):value for name,value in PINS.items()}
    if identity["installed_site_distributions"] != expected:
        raise ValueError("Black target site-packages has missing, extra or wrong distributions")
    markers = identity["dependency_verification"].get("marker_environment", {})
    if (markers.get("python_full_version") != version or markers.get("python_version") != ".".join(version.split(".")[:2])
            or markers.get("sys_platform") != "darwin" or markers.get("platform_system") != "Darwin"
            or markers.get("platform_machine") != "arm64"):
        raise ValueError("Black dependency marker environment differs")
    if validate_dependencies(pins, identity["distributions"], markers) != identity["dependency_verification"]:
        raise ValueError("Black dependency verification differs")
    for name,row in identity["distributions"].items():
        if not Path(row["metadata_path"]).is_relative_to(venv) or len(row["installed_records"]) != 1:
            raise ValueError(f"Black distribution metadata/RECORD is incomplete: {name}")
        for record,fact in row["installed_records"].items():
            raw = base64.b64decode(fact["bytes_b64"], validate=True)
            if len(raw) != fact["bytes"] or digest(raw) != fact["sha256"] or row["distribution_files"][record] != {"bytes":len(raw),"sha256":digest(raw)}:
                raise ValueError("installed RECORD raw bytes differ")
            declarations = list(csv.reader(io.StringIO(raw.decode("utf-8"))))
            if (any(len(parts) != 3 for parts in declarations)
                    or len({parts[0] for parts in declarations}) != len(declarations)
                    or {parts[0] for parts in declarations} != set(row["distribution_files"])):
                raise ValueError("installed RECORD file denominator differs")
            for filename, encoded, size in declarations:
                if filename == record:
                    if encoded != "" or size != "":
                        raise ValueError("installed RECORD self-row must omit hash and size")
                    continue
                payload = row["distribution_files"][filename]
                expected_hash = "sha256=" + base64.urlsafe_b64encode(bytes.fromhex(payload["sha256"])).rstrip(b"=").decode()
                if encoded != expected_hash or size != str(payload["bytes"]):
                    raise ValueError("installed RECORD declared payload hash or size differs")
    if set(identity["modules"]) != set(PINS):
        raise ValueError("Black module identity denominator differs")
    for name,module in identity["modules"].items():
        path = Path(module["path"])
        if not path.is_relative_to(venv) or digest(path.read_bytes()) != module["sha256"]:
            raise ValueError(f"Black module import escaped or changed: {name}")


def prepare(python: Path, version: str, root: Path, pins: list[dict], wheels: list[Path], interpreter_pin: dict) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    require_pins(pins)
    checked_bytes(python.resolve(strict=True).read_bytes(), interpreter_pin["executable_sha256"])
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated}))"
    command([str(python),"-I","-B","-c",probe], root, root / "base-identity")
    base = json.loads((root / "base-identity/stdout.raw").read_bytes())
    require_python_identity(base, version)
    command([str(python),"-I","-B","-m","venv","--symlinks","--without-pip",str(root / "venv")], root, root / "create-venv")
    executable = root / "venv/bin/python"
    link = require_venv_link(executable, python)
    installer = bundled_pip(python, version)
    installer_raw = installer.read_bytes()
    write_json(root / "installer-identity.json", {"path":str(installer), "bytes":len(installer_raw),
               "sha256":digest(installer_raw), "source":"exact archive-bound interpreter ensurepip bundle"})
    artifacts = []
    if len(wheels) != len(pins):
        raise ValueError("Black wheel denominator differs")
    for pin,path in zip(pins,wheels):
        raw = checked_bytes(path.read_bytes(), pin["sha256"])
        artifacts.append({"name":pin["name"], "version":pin["version"], "filename":path.name, "bytes":len(raw), "sha256":digest(raw)})
    prefix = [str(executable),"-I","-B","-c",INSTALLER,str(installer)]
    command([*prefix,"install","--no-index","--no-deps","--no-compile",*map(str,wheels)], root,
            root / "install", inputs={"installer_sha256":digest(installer_raw), "wheels":artifacts})
    command([str(executable),"-I","-B","-c",PACKAGE_PROBE,json.dumps(pins)], root, root / "package-identity")
    identity = json.loads((root / "package-identity/stdout.raw").read_bytes())
    require_package_identity(identity, version, pins, root / "venv")
    command([*prefix,"list","--format=json"], root, root / "installed-distributions")
    command([*prefix,"freeze","--all"], root, root / "pip-freeze")
    identity.update(base_identity=base, base_executable_sha256=digest(python.resolve().read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()), wheel_artifacts=artifacts,
                    installer_sha256=digest(installer_raw), historical_environment=False, **link)
    write_json(root / "identity.json", identity)
    return executable


def decode_records(raw: bytes, cases: list[dict]) -> list[dict]:
    records = json.loads(raw)
    if not isinstance(records, list) or len(records) != CASE_COUNT or len(cases) != CASE_COUNT:
        raise ValueError("Black batch must contain every ordered input row")
    for record in records:
        if not isinstance(record, dict):
            raise ValueError("Black main record is not an object")
        keys = (["reference_error", "target_syntax_hint", "reference_check_exit"] if "reference_error" in record else
                ["output", "sha256", "reference_converges", "target_syntax_hint", "reference_check_exit", "newline"])
        if list(record) != keys:
            raise ValueError("Black main record schema or field order differs")
    if raw != json.dumps(records, ensure_ascii=True).encode():
        raise ValueError("Black main stdout is not the exact original JSON transport")
    return records


def validate_observer(root: Path, executable: Path, version: str, cases: list[dict], records: list[dict]) -> dict:
    observed = json.loads((root / "observer-environment.json").read_bytes())
    package = json.loads((root / "identity.json").read_bytes())
    if (observed["python"] != list(map(int,version.split("."))) or observed["executable"] != str(executable)
            or observed["prefix"] != str(root / "venv") or observed["base_prefix"] != package["base_prefix"]
            or observed["cwd"] != os.getcwd() or observed["black_version"] != "26.10.0"
            or observed["case_count"] != CASE_COUNT or observed["result_count"] != CASE_COUNT
            or observed["user_site_enabled"] is not False
            or observed["flags"]["isolated"] != 0 or observed["flags"]["ignore_environment"] != 0):
        raise ValueError("Black main interpreter/import/cwd identity differs")
    if observed["log_capture_errors"]:
        raise ValueError("referenced Black log evidence is missing or invalid")
    markers = {"This diff might be helpful: ", "This invalid output might be helpful: "}
    expected_logs = {(index, marker) for index, record in enumerate(records)
                     for marker in markers if marker in record.get("reference_error", "")}
    logs = observed["referenced_logs"]
    actual_logs = [(log["index"], log["marker"]) for log in logs]
    saved_paths = [Path(log["saved_file"]) for log in logs]
    if (len(set(actual_logs)) != len(actual_logs) or set(actual_logs) != expected_logs
            or len(set(saved_paths)) != len(saved_paths)):
        raise ValueError("referenced Black log coverage or retained path is missing, repeated or unexpected")
    site_root = Path(package["distributions"]["black"]["metadata_path"]).parent
    files = {name:fact for distribution in package["distributions"].values()
             for name,fact in distribution["distribution_files"].items()}
    if "black" not in observed["modules"]:
        raise ValueError("main Black module identity is missing")
    for name,module in observed["modules"].items():
        path = Path(module["path"])
        relative = str(path.relative_to(site_root))
        fact = {"bytes":module["bytes"], "sha256":module["sha256"]}
        if not path.is_relative_to(root / "venv") or files.get(relative) != fact or digest(path.read_bytes()) != module["sha256"]:
            raise ValueError("main package module differs from its installed payload: " + name)
    canonical_temp = Path(tempfile.gettempdir()).resolve()
    if observed["canonical_temp_parent"] != str(canonical_temp):
        raise ValueError("main canonical temp parent differs from inherited environment")
    for log in logs:
        index = log["index"]
        if (type(index) is not int or not 0 <= index < CASE_COUNT or log["id"] != cases[index]["id"]
                or log["marker"] not in markers
                or "reference_error" not in records[index]
                or log["marker"] + log["reported_path"] not in records[index]["reference_error"]
                or log["error_class"] != records[index]["reference_error"].split(": ",1)[0]):
            raise ValueError("referenced Black log case/error identity differs")
        original = Path(log["reported_path"]).resolve(strict=True)
        if (original.parent != canonical_temp or str(original) != log["canonical_path"]
                or log["canonical_temp_parent"] != str(canonical_temp)
                or not re.fullmatch(r"blk_[A-Za-z0-9_]+\.log", original.name)):
            raise ValueError("referenced Black log parent/name differs")
        relative = Path(log["saved_file"])
        if len(relative.parts) != 2 or relative.parts[0] != "referenced-error-logs" or ".." in relative.parts:
            raise ValueError("unsafe retained Black log path")
        saved = (root / relative).read_bytes()
        if len(saved) != log["bytes"] or digest(saved) != log["sha256"] or original.read_bytes() != saved:
            raise ValueError("retained Black log raw bytes differ")
    return observed


def make_observations(cases: list[dict], expected: list[dict], records: list[dict] | None,
                      minor: int, receipt: dict | None, observer: dict | None,
                      unavailable: str = "not_run") -> list[dict]:
    from q0_black_compare import compare_records, validate_record
    if len(cases) != CASE_COUNT or len(expected) != CASE_COUNT:
        raise ValueError("Black observation index requires every input and expected row")
    if records is not None and len(records) != CASE_COUNT:
        raise ValueError("Black observation index cannot accept partial arrays")
    rows = []
    for index, (case,frozen) in enumerate(zip(cases,expected)):
        row = {"input_index":index, "id":case["id"], "name":case.get("name"), "origins":case["origins"],
               "source_sha256":case["source_sha256"], "python_version":VERSIONS[minor], "python_minor":minor,
               "expected_pointer":{"file":"expected-rows.json", "array_index":index, "target_index":minor-12},
               "actual_pointer":{"file":f"python3{minor}/main/stdout.raw", "array_index":index} if records is not None else None,
               "batch_receipt_pointer":f"python3{minor}/main/receipt.json" if receipt is not None else None,
               "observed_batch_actual_exit":receipt.get("actual_exit") if receipt else None,
               "per_case_process_exit_observed":False,
               "semantic_roles":{role:"unknown" for role in ("positive","negative","interaction","dormant","unsupported")}}
        if records is None:
            row.update(observation_status=unavailable, strict_comparison={"equal":None,"reason":unavailable,"approved_path_variant":False},
                       computed_reference_check_exit=None, secondary_integrity_errors=[unavailable], secondary_metadata_equal=None)
        else:
            actual = records[index]
            comparison = compare_records(actual,frozen["expected"][minor-12])
            secondary = validate_record(actual,case)
            keys = ("target_syntax_hint","reference_check_exit","reference_converges","newline","sha256")
            actual_metadata = {key:actual[key] for key in keys if key in actual}
            frozen_metadata = {key:frozen["expected"][minor-12][key] for key in keys if key in frozen["expected"][minor-12]}
            row.update(observation_status="observed", strict_comparison=comparison,
                       computed_reference_check_exit=actual["reference_check_exit"],
                       reference_outcome="reference_error" if "reference_error" in actual else "output",
                       secondary_integrity_errors=secondary, secondary_metadata_equal=actual_metadata==frozen_metadata,
                       secondary_actual_metadata=actual_metadata, secondary_frozen_metadata=frozen_metadata,
                       referenced_log_pointers=[log["saved_file"] for log in (observer or {}).get("referenced_logs",[]) if log["index"]==index])
        rows.append(row)
    return rows


def write_index(path: Path, rows: list[dict]) -> None:
    with path.open("x", encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=True) + "\n")


def capture_target(executable: Path, root: Path, version: str, cases: list[dict], expected: list[dict],
                   input_bytes: bytes, shared_inputs: dict) -> tuple[dict,list[dict]]:
    minor = int(version.split(".")[1])
    errors, receipt, records, observed = [], None, None, None
    (root / "referenced-error-logs").mkdir(exist_ok=False)
    text = observer_source(root,root / "venv")
    (root / "observer-source.py").write_text(text)
    try:
        receipt = run_inherited([str(executable),"-c",text],input_bytes,root / "main",
                                {**shared_inputs,"case_count":CASE_COUNT,"observer_sha256":digest(text.encode()),
                                 "environment_identity_sha256":digest((root / "identity.json").read_bytes())},timeout=300)
        try:
            require_receipt(receipt)
        except Exception as error:
            errors.append({"stage":"main_process","error_class":type(error).__name__,"message":str(error)})
        records = decode_records((root / "main/stdout.raw").read_bytes(),cases)
        observed = validate_observer(root,executable,version,cases,records)
        if ((root / "main/environment-before.json").read_bytes()
                != (root / "main/environment-after.json").read_bytes()):
            raise ValueError("controller environment changed during main capture")
    except Exception as error:
        errors.append({"stage":"main_capture","error_class":type(error).__name__,"message":str(error)})
    rows = make_observations(cases,expected,records,minor,receipt,observed,"unknown" if receipt else "not_run")
    write_index(root / "observation-index.jsonl",rows)
    comparisons = [row["strict_comparison"] for row in rows]
    write_json(root / "comparison.json", {"case_count":CASE_COUNT,"records_available":records is not None,
                "strict_matches":sum(row["equal"] is True for row in comparisons),
                "strict_failures":sum(row["equal"] is False for row in comparisons),"observations":rows})
    complete = not errors and records is not None and len(records)==CASE_COUNT
    strict = complete and all(row["equal"] is True for row in comparisons)
    integrity = complete and all(not row["secondary_integrity_errors"] for row in rows)
    summary = {"python_version":version, "errors":errors, "evidence_complete":complete,
               "observed_rows":len(records) if records is not None else 0, "indexed_rows":len(rows),
               "strict_parity_passed":strict, "strict_matches":sum(row["equal"] is True for row in comparisons),
               "strict_failures":sum(row["equal"] is False for row in comparisons),
               "secondary_integrity_passed":integrity,
               "secondary_metadata_matches":sum(row["secondary_metadata_equal"] is True for row in rows),
               "computed_status_counts":{str(code):sum(row["computed_reference_check_exit"]==code for row in rows) for code in (0,1,123)},
               "batch_actual_exit":receipt.get("actual_exit") if receipt else None,
               "referenced_logs":len((observed or {}).get("referenced_logs",[])),
               "job_passed":bool(strict and integrity), "issue_accepted":False}
    write_json(root / "target-summary.json",summary)
    return summary,rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    for minor in VERSIONS:
        parser.add_argument(f"--python3{minor}",type=Path,required=True)
    args = parser.parse_args()
    if not args.repository.is_absolute() or not args.output.is_absolute():
        parser.error("repository and output must be absolute paths")
    repository, output = args.repository.resolve(), args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    targets, all_rows, common_error = {}, [], None
    try:
        require_host_context(HOST_CONTEXT)
        from q0_black_inputs import build_inputs, read_expected
        lock_path = repository / "tools/ci/q0_black_reference_lock.json"
        lock = json.loads(lock_path.read_bytes())
        if (lock["version"]!="26.10.0" or lock["python_versions"]!=list(VERSIONS.values())
                or lock["case_count"]!=CASE_COUNT or lock["batch_count"]!=3 or lock["reference_timeout_seconds"]!=300
                or lock["platform"]!=HOST_CONTEXT or set(lock["dependencies"])!=set(VERSIONS.values())):
            raise ValueError("Black reference lock differs")
        for pins in lock["dependencies"].values():
            require_pins(pins)
        profile = repository / "crates/mamba-quality/assets/profile.json"
        checked_bytes(profile.read_bytes(),lock["profile_sha256"])
        for relative,sha256 in lock["comparator_sources"].items():
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("unsafe comparator source path")
            checked_bytes((repository/path).read_bytes(),sha256)
        write_json(output / "run-identity.json", {"source_sha":os.environ.get("GITHUB_SHA"),"run_id":os.environ.get("GITHUB_RUN_ID"),
            "run_attempt":os.environ.get("GITHUB_RUN_ATTEMPT"),"host_context":HOST_CONTEXT,"cwd":os.getcwd(),
            "repository":str(repository),"lock_sha256":digest(lock_path.read_bytes()),"profile_sha256":digest(profile.read_bytes()),
            "batch_count":3,"case_count":CASE_COUNT,"reference_timeout_seconds":300,
            "source_files":{name:digest((repository/"tools/ci"/name).read_bytes()) for name in
                ["q0_black_reference.py","q0_black_inputs.py","q0_black_compare.py","q0_mypy_reference.py","q0_ruff_reference.py"]}})
        archive = download(lock["sdist"],output / "downloads/sdist")
        if len(archive)!=lock["sdist"]["bytes"]:
            raise ValueError("Black source archive size differs")
        cases,proof = build_inputs(repository,archive)
        expected = read_expected(repository)
        if len(cases)!=CASE_COUNT or len(expected)!=CASE_COUNT or [{k:v for k,v in row.items() if k!="expected"} for row in expected]!=cases:
            raise ValueError("Black input/frozen order differs")
        input_bytes = json.dumps(cases).encode()
        if len(input_bytes)!=lock["input_bytes"] or digest(input_bytes)!=lock["input_sha256"]:
            raise ValueError("Black stdin transport bytes differ")
        (output / "inputs.raw").write_bytes(input_bytes)
        write_json(output / "input-proof.json",proof)
        write_json(output / "expected-rows.json",expected)
        shared = {"input_sha256":digest(input_bytes),"input_bytes":len(input_bytes),
                  "input_proof_sha256":digest((output/"input-proof.json").read_bytes()),
                  "expected_rows_sha256":digest((output/"expected-rows.json").read_bytes())}
        cached = {}
        for minor,version in VERSIONS.items():
            root = output / f"python3{minor}"
            try:
                python = getattr(args,f"python3{minor}")
                if not python.is_absolute():
                    raise ValueError("Black interpreter path must be absolute")
                pins,wheels = lock["dependencies"][version],[]
                for pin in pins:
                    key = pin["filename"],pin["sha256"]
                    if key not in cached:
                        destination = output / "downloads" / f"wheel-{len(cached):02}"
                        download(pin,destination)
                        cached[key] = destination/pin["filename"]
                    wheels.append(cached[key])
                executable = prepare(python,version,root,pins,wheels,lock["interpreters"][version])
                targets[version],rows = capture_target(executable,root,version,cases,expected,input_bytes,shared)
            except Exception as error:
                targets[version] = {"python_version":version,"evidence_complete":False,"job_passed":False,
                    "strict_parity_passed":False,"observed_rows":0,"indexed_rows":CASE_COUNT,
                    "errors":[{"stage":"target_prerequisite","error_class":type(error).__name__,"message":str(error)}]}
                rows = make_observations(cases,expected,None,minor,None,None)
                root.mkdir(parents=True,exist_ok=True)
                if not (root/"observation-index.jsonl").exists():
                    write_index(root/"observation-index.jsonl",rows)
                write_json(output/f"python3{minor}-failure.json",targets[version])
            all_rows.extend(rows)
        write_index(output / "observation-index.jsonl",all_rows)
    except Exception as error:
        common_error = {"error_class":type(error).__name__,"message":str(error)}
    for version in VERSIONS.values():
        targets.setdefault(version,{"python_version":version,"evidence_complete":False,"job_passed":False,
                                    "strict_parity_passed":False,"observed_rows":0,"indexed_rows":0,"reason":"common prerequisite failed"})
    passed = common_error is None and all(target["job_passed"] for target in targets.values()) and len(all_rows)==CASE_COUNT*3
    summary = {"schema":"mamba.q0.163.black-hosted-reference.v1","issue":163,"targets":targets,"common_error":common_error,
               "indexed_observations":len(all_rows),"observed_rows":sum(t["observed_rows"] for t in targets.values()),
               "evidence_complete":common_error is None and all(t["evidence_complete"] for t in targets.values()),
               "strict_parity_passed":common_error is None and all(t["strict_parity_passed"] for t in targets.values()),
               "job_passed":passed,"issue_accepted":False,"whole_suite_accepted":False,
               "historical_original_evidence_recovered":False,"native_parity":"not_run",
               "supplementary_174_safety":"not_run","ancillary_error_contracts":"not_run",
               "computed_row_statuses_are_process_exits":False}
    write_json(output/"summary.json",summary)
    write_json(output/"artifact-manifest.json",inventory(output,exclude_venvs=True))
    print(json.dumps({"job_passed":passed,"indexed_observations":len(all_rows),"issue_accepted":False}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
