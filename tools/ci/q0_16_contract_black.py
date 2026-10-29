#!/usr/bin/env python3
"""Plan sealed Black contracts, or capture their original APIs on hosted Mac only.

Source programs are input data. This runner never executes their bodies. A batch
PID, an API invocation, and a target context are separate units.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys

LOCK = "tools/ci/q0_16_contract_black_lock.json"
VERSIONS = {"3.12": "3.12.11", "3.13": "3.13.5", "3.14": "3.14.0"}
MARKER = "__SEALED_INPUT_BYTES_HEX__"
LOG_MARKERS = ("This diff might be helpful: ", "This invalid output might be helpful: ")
API_RECORD_KEYS = ("request_id", "target", "operation", "input_sha256", "input_bytes", "mode", "api_calls", "fast", "lines",
                   "kind", "output", "output_hex", "output_sha256", "exception_type", "message", "encoding", "newline",
                   "computed_reference_exit", "internal_equivalence_and_stability_completed")


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode()


def strict_json(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def require(value: bool, reason: str) -> None:
    if not value:
        raise ValueError(reason)


def save(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=True, indent=2)
        stream.write("\n")


def source_path(repo: Path, name: str) -> Path:
    pure = PurePosixPath(name)
    require(bool(name) and not pure.is_absolute() and ".." not in pure.parts
            and "\\" not in name and str(pure) == name, "unsafe repository source path")
    path = (repo / name).resolve(strict=True)
    require(path.is_relative_to(repo.resolve()), "source escaped repository")
    return path


class Sources:
    def __init__(self, repo: Path):
        self.repo = repo
        self.raw: dict[str, bytes] = {}
        self.objects: dict[str, object] = {}

    def read(self, name: str, expected: str | None = None) -> bytes:
        if name not in self.raw:
            self.raw[name] = source_path(self.repo, name).read_bytes()
        raw = self.raw[name]
        require(expected is None or digest(raw) == expected, "source SHA256 differs: " + name)
        return raw

    def value(self, ref: dict) -> object:
        name = ref["source_id"]
        raw = self.read(name, ref["sha256"])
        if name not in self.objects:
            self.objects[name] = json.loads(raw)
        value = self.objects[name]
        pointer = ref["json_pointer"]
        require(pointer == "" or pointer.startswith("/"), "invalid source JSON pointer")
        for item in pointer.split("/")[1:] if pointer else []:
            require(re.search(r"~(?:[^01]|$)", item) is None, "invalid JSON pointer escape")
            item = item.replace("~1", "/").replace("~0", "~")
            if isinstance(value, list):
                require(re.fullmatch(r"0|[1-9][0-9]*", item) is not None, "invalid array pointer index")
                value = value[int(item)]
            else:
                require(isinstance(value, dict), "source pointer crossed scalar")
                value = value[item]
        require(digest(canonical(value)) == ref["value_sha256"], "source value SHA256 differs")
        return value


def expand(value: object, source_hex: str) -> object:
    if isinstance(value, dict):
        return {key: expand(item, source_hex) for key, item in value.items()}
    if isinstance(value, list):
        return [expand(item, source_hex) for item in value]
    return source_hex if value == MARKER else value


def sealed_mode(target: str) -> dict:
    return {"target_versions": [target], "line_length": 88, "string_normalization": True,
            "magic_trailing_comma": True, "preview": False, "unstable": False,
            "is_pyi": False, "enabled_features": []}


def load_plan(repo: Path) -> tuple[dict, list[dict], Sources]:
    sources = Sources(repo)
    sources.read("tools/ci/q0_16_contract_black.py")
    lock = json.loads(sources.read(LOCK))
    require(lock["schema"] == "mamba.q0.16.black-contract-lock.v1", "Black lock schema differs")
    require(lock["request_count"] == 747 and len(lock["requests"]) == 747, "Black context denominator differs")
    require(lock["python_versions"] == list(VERSIONS.values()), "Black runtime version axis differs")
    for name, sha in {**lock["helper_sources"], **lock["comparison_owners"]}.items():
        sources.read(name, sha)
    upstream = lock["upstream_lock"]
    sources.read(upstream["source_id"], upstream["sha256"])
    requests = []
    for entry in lock["requests"]:
        binding = entry["input_binding"]
        value = sources.value(binding["source"])
        require(type(value) is str, "Black input source must be a string")
        raw = value.encode("utf-8") if binding["representation"] == "utf8" else bytes.fromhex(value)
        require(binding["representation"] in {"utf8", "hex"} and len(raw) == binding["bytes"]
                and digest(raw) == binding["sha256"], "Black input byte binding differs")
        context = expand(entry["context_template"], raw.hex())
        require(digest(canonical(context)) == entry["request_id"], "sealed request ID differs")
        target, operation = context["target"], context["operation"]
        require(context["tool"] == "black" and target in VERSIONS
                and operation in {"black_safe_format_API", "black_decode_bytes_safe_format_API"}, "unknown Black target operation")
        require(entry["mode"] == sealed_mode(target), "Black API mode differs")
        profile = context["profile_source"]
        sources.read(profile["path"], profile["sha256"])
        require(context["selection"] == {"kind": "in_memory_API"}, "Black contract is not an in-memory API")
        for alias in entry["source_aliases"]:
            sources.value(alias["case_source"])
        expected = []
        for item in entry["expected_bindings"]:
            require(item["target"] == target, "expected target differs")
            frozen = sources.value(item["source"])
            require(isinstance(frozen, dict), "Black expected record is not an object")
            extra = sources.value(item["invalid_output_log_hash_source"]) if "invalid_output_log_hash_source" in item else None
            expected.append({"binding": item, "value": frozen, "invalid_output_log_sha256": extra})
        require(bool(expected), "Black context lacks expected output binding")
        requests.append({"request_id": entry["request_id"], "target": target, "operation": operation,
                         "source_hex": raw.hex(), "mode": entry["mode"], "input_binding": binding,
                         "source_aliases": entry["source_aliases"], "expected": expected})
    require(len({r["request_id"] for r in requests}) == 747, "duplicate Black context ID")
    require(dict(Counter(r["target"] for r in requests)) == lock["target_counts"]
            == {target: 249 for target in VERSIONS}, "Black target context counts differ")
    require(dict(Counter(r["operation"] for r in requests)) == lock["operation_counts"]
            == {"black_decode_bytes_safe_format_API": 30, "black_safe_format_API": 717}, "Black operation counts differ")
    return lock, requests, sources


def plan_rows(requests: list[dict]) -> list[dict]:
    return [{"request_id": r["request_id"], "target": r["target"], "operation": r["operation"],
             "input_binding": r["input_binding"], "source_aliases": r["source_aliases"],
             "expected_bindings": [e["binding"] for e in r["expected"]],
             "status": "not_run", "actual_api_calls": None, "comparison": None,
             "actual_reference_execution": "not_run", "native_gate_status": "not_run",
             "issue_accepted": False} for r in requests]


def require_hosted(environment: dict[str, str] | None = None, platform: str | None = None) -> None:
    env = os.environ if environment is None else environment
    require(env.get("GITHUB_ACTIONS") == "true" and (platform or sys.platform) == "darwin"
            and env.get("RUNNER_OS") == "macOS" and env.get("RUNNER_ARCH") == "ARM64",
            "Black API execution requires the GitHub-hosted macos-15 ARM64 job")
    require(not env.get("PYTHONPATH") and not env.get("PYTHONHOME"), "ambient Python import path is outside the sealed environment")


OBSERVER = r'''
import hashlib, importlib.metadata, importlib.util, json, os, pathlib, re, site, sys, tempfile
sidecar = pathlib.Path(__SIDECAR__)
expected_prefix = pathlib.Path(__VENV__).resolve()
if pathlib.Path(sys.prefix).resolve() != expected_prefix or site.ENABLE_USER_SITE:
    raise RuntimeError("observer interpreter is outside the prepared venv")
if os.environ.get("PYTHONPATH") or os.environ.get("PYTHONHOME"):
    raise RuntimeError("observer inherited a foreign Python import path")
spec = importlib.util.find_spec("black")
if not spec or not spec.origin or not pathlib.Path(spec.origin).resolve().is_relative_to(expected_prefix):
    raise RuntimeError("Black import would leave the prepared venv")
import black
if importlib.metadata.version("black") != "26.10.0":
    raise RuntimeError("Black version differs from the sealed pin")
cases = json.load(sys.stdin)
results, logs, log_errors = [], [], []
temp_parent = pathlib.Path(tempfile.gettempdir()).resolve()
def retain(error, request_id):
    for marker in ("This diff might be helpful: ", "This invalid output might be helpful: "):
        if marker not in str(error):
            continue
        reported = str(error).rsplit(marker, 1)[1]
        try:
            original = pathlib.Path(reported)
            resolved = original.resolve(strict=True)
            if not original.is_absolute() or resolved.parent != temp_parent or not re.fullmatch(r"blk_[A-Za-z0-9_]+\.log", resolved.name) or not resolved.is_file():
                raise ValueError("referenced Black log parent/name differs")
            raw = resolved.read_bytes()
            saved = "referenced-error-logs/" + request_id + "-" + str(len(logs)) + ".raw"
            with (sidecar / saved).open("xb") as stream:
                stream.write(raw)
            logs.append({"request_id":request_id, "marker":marker, "reported_path":reported,
                         "canonical_path":str(resolved), "saved_file":saved,
                         "bytes":len(raw), "sha256":hashlib.sha256(raw).hexdigest()})
        except Exception as failure:
            log_errors.append({"request_id":request_id, "error_class":type(failure).__name__, "message":str(failure)})
for case in cases:
    raw = bytes.fromhex(case["source_hex"])
    target = getattr(black.TargetVersion, "PY" + case["target"].replace(".", ""))
    mode = black.Mode(target_versions={target}, line_length=88, string_normalization=True,
                      magic_trailing_comma=True, preview=False, unstable=False, is_pyi=False, enabled_features=set())
    descriptor = {"target_versions":sorted(t.name[2] + "." + t.name[3:] for t in mode.target_versions),
                  "line_length":mode.line_length,"string_normalization":mode.string_normalization,
                  "magic_trailing_comma":mode.magic_trailing_comma,"preview":mode.preview,"unstable":mode.unstable,
                  "is_pyi":mode.is_pyi,"enabled_features":sorted(feature.name for feature in mode.enabled_features)}
    row = {"request_id":case["request_id"], "target":case["target"], "operation":case["operation"],
           "input_sha256":hashlib.sha256(raw).hexdigest(), "input_bytes":len(raw),
           "mode":descriptor,
           "api_calls":{"decode_bytes":0,"format_file_contents":0}, "fast":False, "lines":[],
           "kind":None,"output":None,"output_hex":None,"output_sha256":None,
           "exception_type":None,"message":None,"encoding":None,"newline":None,
           "computed_reference_exit":None,"internal_equivalence_and_stability_completed":False}
    source = None
    try:
        if case["operation"] == "black_decode_bytes_safe_format_API":
            row["api_calls"]["decode_bytes"] += 1
            source, encoding, newline = black.decode_bytes(raw, mode)
            row.update(encoding=encoding,newline=newline)
        else:
            source = raw.decode("utf-8")
        row["api_calls"]["format_file_contents"] += 1
        try:
            output = black.format_file_contents(source, fast=False, mode=mode, lines=())
            row["kind"] = "formatted"
            row["internal_equivalence_and_stability_completed"] = True
        except black.NothingChanged as error:
            output = source
            row.update(kind="unchanged",exception_type=type(error).__module__+"."+type(error).__name__,message=str(error))
        output_bytes = (output.replace("\n", row["newline"]).encode(row["encoding"])
                        if case["operation"] == "black_decode_bytes_safe_format_API" else output.encode("utf-8"))
        row.update(output=output,output_hex=output_bytes.hex(),output_sha256=hashlib.sha256(output_bytes).hexdigest(),
                   computed_reference_exit=int(output_bytes != raw))
    except Exception as error:
        row.update(kind="error",exception_type=type(error).__module__+"."+type(error).__name__,message=str(error),computed_reference_exit=123)
        retain(error,case["request_id"])
    results.append(row)
modules = {}
stdlib = pathlib.Path(sys.base_prefix) / "lib" / ("python%d.%d" % sys.version_info[:2])
for name,module in sorted(sys.modules.items()):
    filename = getattr(module,"__file__",None)
    if filename is None:
        continue
    path = pathlib.Path(filename).resolve()
    if not path.is_relative_to(expected_prefix):
        if path.is_relative_to(stdlib) and not path.is_relative_to(stdlib/"site-packages"):
            continue
        raise RuntimeError("observer imported code outside its venv and pinned standard library: "+name)
    data = path.read_bytes()
    modules[name] = {"path":str(path),"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()}
metadata = {"python":list(sys.version_info[:3]),"executable":sys.executable,"prefix":sys.prefix,
            "base_prefix":sys.base_prefix,"cwd":os.getcwd(),"user_site_enabled":site.ENABLE_USER_SITE,
            "flags":{"isolated":sys.flags.isolated,"ignore_environment":sys.flags.ignore_environment},
            "black_version":black.__version__,"request_count":len(cases),"result_count":len(results),
            "canonical_temp_parent":str(temp_parent),"modules":modules,"referenced_logs":logs,"log_capture_errors":log_errors}
with (sidecar/"observer-environment.json").open("x") as stream:
    json.dump(metadata,stream,ensure_ascii=True,indent=2)
    stream.write("\n")
json.dump(results,sys.stdout,ensure_ascii=True,separators=(",",":"))
'''


def observer_source(root: Path) -> str:
    return OBSERVER.replace("__SIDECAR__", repr(str(root))).replace("__VENV__", repr(str(root / "venv")))


def require_saved_receipt(returned: dict, stored: dict) -> None:
    require(type(returned) is dict and type(stored) is dict and strict_json(returned) == strict_json(stored),
            "saved receipt differs from returned receipt")
    require(type(returned.get("pid")) is int and returned["pid"] > 0
            and type(returned.get("waited_pid")) is int and returned["waited_pid"] > 0
            and returned.get("waited_pid") == returned["pid"] and returned.get("wait_completed") is True
            and returned.get("timed_out") is False and returned.get("launch_error") is None
            and type(returned.get("actual_exit")) is int and returned["actual_exit"] == 0
            and type(returned.get("expected_exits")) is list and len(returned["expected_exits"]) == 1
            and type(returned["expected_exits"][0]) is int and returned["expected_exits"][0] == 0
            and returned.get("passed") is True, "batch receipt lacks completed successful wait")


def reconcile_capture_files(capture: Path, returned: dict, stdin: bytes) -> bytes:
    """Read saved receipts and every raw process input/output again before pass."""
    saved_return = json.loads((capture / "returned-receipt.json").read_bytes())
    stored = json.loads((capture / "receipt.json").read_bytes())
    require(strict_json(saved_return) == strict_json(returned), "returned receipt changed after capture")
    require_saved_receipt(returned, stored)
    for filename, key in (("stdin.raw", "stdin_sha256"), ("stdout.raw", "stdout_sha256"),
                          ("stderr.raw", "stderr_sha256"), ("inputs.json", "inputs_sha256"),
                          ("environment-before.json", "environment_before_sha256"),
                          ("environment-after.json", "environment_after_sha256")):
        require(digest((capture / filename).read_bytes()) == stored[key], "saved capture bytes changed: " + filename)
    require((capture / "stdin.raw").read_bytes() == stdin, "saved batch input transport differs")
    require((capture / "environment-before.json").read_bytes() == (capture / "environment-after.json").read_bytes(),
            "saved inherited environment changed")
    return (capture / "stdout.raw").read_bytes()


def message_equal(actual: str | None, expected: str | None, exception: str | None,
                  policy: str, logs: list[dict], invalid_hash: str | None) -> bool:
    if actual is None or expected is None:
        return actual == expected
    marker = None
    if exception == "black.parsing.ASTSafetyError" and policy != "literal_nothing_changed":
        marker = LOG_MARKERS[0]
    elif exception == "black.parsing.InvalidInput" and expected.startswith("INTERNAL ERROR: Black 26.10.0 on Python (CPython) ") and policy == "format_wrapper":
        marker = LOG_MARKERS[1]
    if marker is None:
        return actual == expected
    if marker not in actual or marker not in expected:
        return False
    aprefix, apath = actual.rsplit(marker, 1)
    eprefix, epath = expected.rsplit(marker, 1)
    grammar = r"blk_[A-Za-z0-9_]+\.log" if marker == LOG_MARKERS[0] else r"blk_.*\.log"
    if aprefix != eprefix or not all(re.fullmatch(grammar, PurePosixPath(p).name) for p in (apath, epath)):
        return False
    matching = [log for log in logs if log["marker"] == marker and log["reported_path"] == apath]
    return len(matching) == 1 and (marker != LOG_MARKERS[1] or matching[0]["sha256"] == invalid_hash)


def compare_record(actual: dict, request: dict, logs: list[dict]) -> list[dict]:
    require(isinstance(actual, dict) and list(actual) == list(API_RECORD_KEYS), "Black API record schema/order differs")
    require(actual["request_id"] == request["request_id"] and actual["target"] == request["target"]
            and actual["operation"] == request["operation"], "Black API result context differs")
    raw = bytes.fromhex(request["source_hex"])
    require(actual["input_sha256"] == digest(raw) and type(actual["input_bytes"]) is int and actual["input_bytes"] == len(raw)
            and strict_json(actual["mode"]) == strict_json(request["mode"]) and actual["fast"] is False and actual["lines"] == []
            and type(actual["computed_reference_exit"]) is int, "Black API result input/mode/fast/lines differs")
    calls = actual["api_calls"]
    require(strict_json(calls) == strict_json({"decode_bytes": int(request["operation"] == "black_decode_bytes_safe_format_API"),
                      "format_file_contents": 1}), "Black API invocation denominator differs")
    if actual["kind"] != "error":
        output = bytes.fromhex(actual["output_hex"])
        require(digest(output) == actual["output_sha256"] and actual["computed_reference_exit"] == int(output != raw)
                and ((actual["kind"] == "formatted" and actual["internal_equivalence_and_stability_completed"] is True)
                     or (actual["kind"] == "unchanged" and output == raw
                         and actual["internal_equivalence_and_stability_completed"] is False)), "Black successful API integrity differs")
    else:
        require(actual["output"] is None and actual["output_hex"] is None
                and actual["computed_reference_exit"] == 123 and actual["internal_equivalence_and_stability_completed"] is False,
                "Black API error integrity differs")
    comparisons = []
    for binding in request["expected"]:
        expected, policy = binding["value"], binding["binding"]["comparison_policy"]
        fields = {}
        if policy == "raw_bytes":
            fields = {"output_bytes": actual["output_hex"] == expected["stable_hex"],
                      "output_sha256": actual["output_sha256"] == expected["stable_sha256"],
                      "computed_reference_exit": actual["computed_reference_exit"] == expected["original_initial_check_exit"],
                      "successful_safe_API": actual["kind"] != "error"}
        else:
            kind, exception, message = actual["kind"], actual["exception_type"], actual["message"]
            if kind == "unchanged" and policy != "literal_nothing_changed":
                kind, exception, message = "formatted", None, None
            expected_hex = expected.get("output_hex")
            if "output" in expected:
                expected_hex = expected["output"].encode().hex() if expected["output"] is not None else None
            fields = {"kind": kind == expected["kind"], "output_bytes": actual["output_hex"] == expected_hex,
                      "computed_reference_exit": actual["computed_reference_exit"] == expected["reference_exit"],
                      "exception_type": exception == expected["exception_type"],
                      "message": message_equal(message, expected["message"], expected["exception_type"], policy,
                                               logs, binding["invalid_output_log_sha256"])}
            if "error_log_sha256" in expected and kind == "error":
                fields["error_log_bytes"] = len(logs) == 1 and logs[0]["sha256"] == expected["error_log_sha256"] and logs[0]["bytes_hex"] == expected["error_log_hex"]
        comparisons.append({"expected_binding": binding["binding"], "fields": fields,
                            "equal": bool(fields) and all(fields.values()),
                            "scope": "source-owned API output/error comparator; old CLI metadata is not a new CLI execution"})
    return comparisons


def capture_payloads(root: Path, identity: dict, phase: str, python: Path) -> dict:
    venv = (root / "venv").resolve(strict=True)
    destination = root / "retained-payloads"
    destination.mkdir(exist_ok=True)
    rows = []
    for name, distribution in identity["distributions"].items():
        base = Path(distribution["metadata_path"]).parent
        for relative, expected in distribution["distribution_files"].items():
            path = (base / relative).resolve(strict=True)
            require(path.is_relative_to(venv) and path.is_file(), "installed payload escaped prepared venv")
            raw = path.read_bytes()
            fact = {"bytes": len(raw), "sha256": digest(raw)}
            require(fact == expected, "installed payload changed from package identity")
            saved = destination / (fact["sha256"] + ".raw")
            if saved.exists():
                require(saved.read_bytes() == raw, "retained installed payload differs")
            else:
                saved.write_bytes(raw)
            rows.append({"distribution": name, "installed_relative_path": relative, **fact,
                         "retained": saved.relative_to(root).as_posix()})
    base = python.resolve(strict=True).read_bytes()
    require(digest(base) == identity["base_executable_sha256"] == identity["venv_executable_sha256"], "base/venv binary changed")
    binary = destination / (digest(base) + ".raw")
    if not binary.exists():
        binary.write_bytes(base)
    require(binary.read_bytes() == base and bool(rows), "base payload copy or package denominator differs")
    result = {"phase": phase, "package_files": rows, "base_executable": {"sha256": digest(base), "bytes": len(base), "retained": binary.relative_to(root).as_posix()}}
    save(root / ("payloads-" + phase + ".json"), result)
    return result


def validate_batch_ids(records: list[dict], selected: list[dict]) -> None:
    require(isinstance(records, list) and len(records) == len(selected) == 249
            and [r["request_id"] for r in records] == [r["request_id"] for r in selected], "Black batch is partial or out of order")


def validate_observer(root: Path, executable: Path, version: str, selected: list[dict], records: list[dict]) -> tuple[dict, dict[str, list[dict]]]:
    observed = json.loads((root / "observer-environment.json").read_bytes())
    identity = json.loads((root / "identity.json").read_bytes())
    require(observed["python"] == list(map(int, version.split("."))) and observed["executable"] == str(executable)
            and observed["prefix"] == str(root / "venv") and observed["base_prefix"] == identity["base_prefix"]
            and observed["cwd"] == os.getcwd() and observed["user_site_enabled"] is False
            and observed["flags"] == {"isolated": 0, "ignore_environment": 0}
            and observed["black_version"] == "26.10.0" and observed["request_count"] == len(selected)
            and observed["result_count"] == len(records) and not observed["log_capture_errors"], "Black observer identity/log capture differs")
    validate_batch_ids(records, selected)
    site = Path(identity["distributions"]["black"]["metadata_path"]).parent
    files = {name: fact for item in identity["distributions"].values() for name, fact in item["distribution_files"].items()}
    require("black" in observed["modules"], "observer Black module identity is missing")
    for module in observed["modules"].values():
        path = Path(module["path"])
        require(path.is_relative_to(root / "venv") and files.get(path.relative_to(site).as_posix()) == {"bytes": module["bytes"], "sha256": module["sha256"]}
                and digest(path.read_bytes()) == module["sha256"], "observer module differs from pinned installed payload")
    require(observed["canonical_temp_parent"] == str(Path(__import__("tempfile").gettempdir()).resolve()), "observer temp parent differs")
    by_id = {r["request_id"]: r for r in records}
    logs: dict[str, list[dict]] = {}
    for log in observed["referenced_logs"]:
        record = by_id[log["request_id"]]
        require(record["kind"] == "error" and log["marker"] + log["reported_path"] in record["message"], "Black error log is not referenced by its actual error")
        relative = PurePosixPath(log["saved_file"])
        require(len(relative.parts) == 2 and relative.parts[0] == "referenced-error-logs" and ".." not in relative.parts, "unsafe saved error log path")
        raw = (root / relative).read_bytes()
        original = Path(log["reported_path"]).resolve(strict=True)
        require(original.parent == Path(observed["canonical_temp_parent"]) and original.read_bytes() == raw
                and len(raw) == log["bytes"] and digest(raw) == log["sha256"], "retained error log raw bytes differ")
        logs.setdefault(log["request_id"], []).append({**log, "bytes_hex": raw.hex()})
    expected_logs = {(r["request_id"], marker) for r in records for marker in LOG_MARKERS if marker in (r["message"] or "")}
    actual_logs = [(log["request_id"], log["marker"]) for log in observed["referenced_logs"]]
    require(len(set(actual_logs)) == len(actual_logs) and set(actual_logs) == expected_logs, "Black referenced log coverage differs")
    return observed, logs


def run(repo: Path, output: Path, python: Path, version: str) -> int:
    output.mkdir(parents=True, exist_ok=False)
    # Declare every frozen context before source checks, host admission, or prep.
    # A failed prerequisite therefore leaves all planned rows visible as unrun.
    declared = json.loads(source_path(repo, LOCK).read_bytes())
    require(len(declared["requests"]) == 747, "sealed Black plan denominator differs")
    requests = [{"request_id": entry["request_id"], "target": entry["context_template"]["target"],
                 "operation": entry["context_template"]["operation"], "input_binding": entry["input_binding"],
                 "source_aliases": entry["source_aliases"], "expected": [{"binding": b} for b in entry["expected_bindings"]]}
                for entry in declared["requests"]]
    rows = plan_rows(requests)
    save(output / "plan.json", {"schema": "mamba.q0.16.black-contract-plan.v1", "request_count": len(rows), "requests": rows})
    save(output / "initial-results.json", rows)
    errors, receipt, selected, identity = [], None, [], None
    target = ".".join(version.split(".")[:2])
    root = output / ("python" + target.replace(".", ""))
    try:
        lock, requests, sources = load_plan(repo)
        require(rows == plan_rows(requests), "resolved Black plan differs from declared initial rows")
        require(version == VERSIONS.get(target), "Black full Python version differs")
        require(python.is_absolute(), "Black interpreter path must be absolute")
        require_hosted()
        import q0_black_reference as black_reference
        from q0_pylint_reference import HOST_CONTEXT, require_host_context
        require_host_context(HOST_CONTEXT)
        upstream = json.loads(sources.read(lock["upstream_lock"]["source_id"]))
        selected = [r for r in requests if r["target"] == target]
        save(output / "run-identity.json", {"schema": "mamba.q0.16.black-contract-run.v1", "source_sha": os.environ.get("GITHUB_SHA"),
             "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
             "python_version": version, "all_planned_contexts": 747, "selected_target_contexts": len(selected),
             "lock_sha256": digest((repo / LOCK).read_bytes()),
             "source_files": {name: digest(raw) for name, raw in sources.raw.items()},
             "native_execution": "not_run", "issue_accepted": False})
        wheels = []
        for index, pin in enumerate(upstream["dependencies"][version]):
            destination = output / "downloads" / ("wheel-%02d" % index)
            black_reference.download(pin, destination)
            wheels.append(destination / pin["filename"])
        executable = black_reference.prepare(python, version, root, upstream["dependencies"][version], wheels, upstream["interpreters"][version])
        identity = json.loads((root / "identity.json").read_bytes())
        before = capture_payloads(root, identity, "before", python)
        (root / "referenced-error-logs").mkdir()
        text = observer_source(root)
        (root / "observer-source.py").write_text(text)
        transport = [{key: r[key] for key in ("request_id", "target", "operation", "source_hex", "mode")} for r in selected]
        stdin = canonical(transport)
        returned = black_reference.run_inherited([str(executable), "-c", text], stdin, root / "batch",
                     {"request_count": len(selected), "input_sha256": digest(stdin), "observer_sha256": digest(text.encode()),
                      "identity_sha256": digest((root / "identity.json").read_bytes())}, timeout=300)
        # Retain the complete API return before reading or parsing saved evidence.
        receipt = returned
        save(root / "batch/returned-receipt.json", returned)
        stored = json.loads((root / "batch/receipt.json").read_bytes())
        require_saved_receipt(returned, stored)
        stdout, stderr = (root / "batch/stdout.raw").read_bytes(), (root / "batch/stderr.raw").read_bytes()
        require(digest(stdout) == receipt["stdout_sha256"] and digest(stderr) == receipt["stderr_sha256"], "batch raw hashes differ from saved receipt")
        require((root / "batch/environment-before.json").read_bytes() == (root / "batch/environment-after.json").read_bytes(), "inherited controller environment changed")
        records = json.loads(stdout)
        require(isinstance(records, list) and stdout == canonical(records), "Black API transport differs")
        observed, logs = validate_observer(root, executable, version, selected, records)
        by_request = {r["request_id"]: r for r in rows}
        for index, (request, actual) in enumerate(zip(selected, records)):
            row = by_request[request["request_id"]]
            comparisons = compare_record(actual, request, logs.get(request["request_id"], []))
            row.update(status="observed", actual_reference_execution="observed", actual_api_calls=actual["api_calls"],
                       actual_pointer={"member": (root / "batch/stdout.raw").relative_to(output).as_posix(), "json_pointer": "/" + str(index)},
                       receipt_pointer=(root / "batch/receipt.json").relative_to(output).as_posix(),
                       returned_receipt_pointer=(root / "batch/returned-receipt.json").relative_to(output).as_posix(),
                       actual_batch_pid=receipt["pid"], per_context_process_exit_observed=False,
                       comparison=comparisons, contract_matches=all(c["equal"] for c in comparisons),
                       referenced_logs=[log["saved_file"] for log in logs.get(request["request_id"], [])])
        after = capture_payloads(root, identity, "after", python)
        require({k:v for k,v in before.items() if k != "phase"} == {k:v for k,v in after.items() if k != "phase"}, "prepared code/payload/RECORD/base changed during capture")
        require(all((repo / name).read_bytes() == raw for name, raw in sources.raw.items()), "sealed repository sources changed during capture")
    except Exception as error:
        errors.append({"stage": "capture", "error_class": type(error).__name__, "message": str(error)})
        if type((receipt or {}).get("pid")) is int and receipt["pid"] > 0:
            for row in rows:
                if row["target"] == target and row["status"] == "not_run":
                    row.update(status="unknown", actual_reference_execution="unknown",
                               actual_batch_pid=receipt["pid"], actual_api_calls=None,
                               reason="known batch process; complete API record not independently reconciled")
    finally:
        if identity is not None and not (root / "payloads-after.json").exists():
            try:
                capture_payloads(root, identity, "after", python)
            except Exception as error:
                errors.append({"stage": "post_capture_payloads", "error_class": type(error).__name__, "message": str(error)})
    save(output / "final-results.json", rows)
    if not errors and receipt is not None:
        try:
            require(strict_json(json.loads((output / "final-results.json").read_bytes())) == strict_json(rows)
                    and strict_json(json.loads((output / "initial-results.json").read_bytes())) == strict_json(plan_rows(requests)),
                    "saved initial/final result data differs")
            stored_records = json.loads(reconcile_capture_files(root / "batch", receipt, stdin))
            _, stored_logs = validate_observer(root, executable, version, selected, stored_records)
            final_by_id = {row["request_id"]: row for row in json.loads((output / "final-results.json").read_bytes())}
            for index, (request, actual) in enumerate(zip(selected, stored_records)):
                row = final_by_id[request["request_id"]]
                require(row["actual_pointer"] == {"member": (root / "batch/stdout.raw").relative_to(output).as_posix(), "json_pointer": "/" + str(index)}
                        and row["actual_api_calls"] == actual["api_calls"] and row["actual_batch_pid"] == receipt["pid"]
                        and row["comparison"] == compare_record(actual, request, stored_logs.get(request["request_id"], [])),
                        "saved final data pointer/comparison differs from raw API record")
        except Exception as error:
            errors.append({"stage": "final_reconciliation", "error_class": type(error).__name__, "message": str(error)})
    observed_rows = [r for r in rows if r["status"] == "observed"]
    matched = bool(not errors and len(observed_rows) == 249 and all(r.get("contract_matches") for r in observed_rows))
    summary = {"schema": "mamba.q0.16.black-contract-summary.v1", "all_planned_target_contexts": 747,
               "selected_target_contexts": 249 if target in VERSIONS else 0, "observed_target_contexts": len(observed_rows),
               "actual_batch_processes": int(type((receipt or {}).get("pid")) is int and receipt["pid"] > 0),
               "waited_batch_processes": int(type((receipt or {}).get("pid")) is int and receipt["pid"] > 0
                    and receipt.get("wait_completed") is True and receipt.get("waited_pid") == receipt["pid"]),
               "capture_process_fact": {key: (receipt or {}).get(key) for key in
                    ("pid", "waited_pid", "wait_completed", "actual_exit", "timed_out", "launch_error")},
               "actual_api_calls": {key: sum((r.get("actual_api_calls") or {}).get(key, 0) for r in rows) for key in ("decode_bytes", "format_file_contents")},
               "contract_capture_passed": matched, "errors": errors, "issue_accepted": False,
               "native_execution": "not_run", "historical_original_evidence_recovered": False,
               "api_statuses_are_process_exits": False, "excluded_venv_tree": root.relative_to(output).as_posix() + "/venv"}
    save(output / "summary.json", summary)
    files = {path.relative_to(output).as_posix(): {"bytes": path.stat().st_size, "sha256": digest(path.read_bytes())}
             for path in sorted(output.rglob("*")) if path.is_file() and not path.is_relative_to(root / "venv")}
    save(output / "artifact-manifest.json", files)
    print(json.dumps({"contract_capture_passed": matched, "planned_target_contexts": 747, "observed_target_contexts": len(observed_rows), "issue_accepted": False}))
    return 0 if matched else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    plan = sub.add_parser("plan")
    plan.add_argument("--repo", type=Path, required=True)
    plan.add_argument("--output", type=Path)
    capture = sub.add_parser("run")
    capture.add_argument("--repo", type=Path, required=True)
    capture.add_argument("--artifact-dir", type=Path, required=True)
    capture.add_argument("--python-version", required=True)
    capture.add_argument("--python", type=Path, required=True)
    args = parser.parse_args()
    if args.mode == "run":
        return run(args.repo.resolve(), args.artifact_dir.resolve(), args.python, args.python_version)
    lock, requests, _ = load_plan(args.repo.resolve())
    result = {"schema": "mamba.q0.16.black-contract-plan.v1", "request_count": len(requests),
              "target_counts": lock["target_counts"], "operation_counts": lock["operation_counts"],
              "requests": plan_rows(requests), "actual_reference_execution": "not_run", "issue_accepted": False}
    if args.output:
        save(args.output, result)
    print(json.dumps({k:v for k,v in result.items() if k != "requests"}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
