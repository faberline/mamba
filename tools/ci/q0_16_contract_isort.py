#!/usr/bin/env python3
"""Plan and observe only the sealed finite D isort contracts on hosted Macs.

An API call, an actual child process, and a source/entry/target context remain
separate units. Fixture bodies are never executed or imported.
"""
from __future__ import annotations

import argparse
from collections import Counter
import copy
import json
import os
from pathlib import Path
import platform
import re
import sys

from q0_16_packet import read_json, safe_path
import q0_16_reference as capture
import q0_isort_reference as reference
from q0_mypy_reference import require_receipt
from q0_ruff_reference import checked_bytes, digest, download, run_command, write_json, write_sources

LOCK = "tools/ci/q0_16_contract_isort_lock.json"
LOCK_SHA = "76b99f51552d0ad5309fd3d347b489cd504d6ebadd03b0134b771e0c19564f7c"
VERSIONS = {"3.12": "3.12.11", "3.13": "3.13.5", "3.14": "3.14.0"}
MARKER = "__SEALED_INPUT_BYTES_HEX__"
BASE = "crates/mamba-quality/src/infrastructure/isort/tests/"
API_OWNERS = {BASE + name + ".json" for name in
              ("ast_errors", "literal_errors", "literal_interactions", "literal_syntax", "literal_values")}
CLI_OWNERS = {BASE + name + ".json" for name in ("insertion_markers", "public_cr_stream", "split_boundaries")}
ADDRESS_KINDS = {BASE + "ast_errors.json": ("Name", "Lambda", "Starred", "UnaryOp"),
                 BASE + "literal_values.json": ("Name", "Lambda", "Starred", "UnaryOp", "Constant", "BinOp")}
STAGES = ("check-source", "format", "check-result")


def encoded(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"), allow_nan=False).encode()


def require(condition: bool, message: str) -> None:
    if not condition: raise ValueError(message)


class Sources:
    def __init__(self, repo: Path): self.repo, self.raw, self.objects = repo, {}, {}

    def read(self, path: str, sha: str | None = None) -> bytes:
        safe_path(path)
        observed = capture.fact(self.repo / path)
        raw = (self.repo / path).read_bytes()
        require(digest(raw) == observed["sha256"] and (sha is None or observed["sha256"] == sha), "source SHA differs: " + path)
        require(path not in self.raw or self.raw[path] == raw, "source changed during plan construction")
        self.raw[path] = raw
        return raw

    def value(self, ref: dict) -> object:
        path = ref["source_id"]; raw = self.read(path, ref["sha256"])
        if path not in self.objects: self.objects[path] = read_json(raw)
        value = self.objects[path]; pointer = ref["json_pointer"]
        require(pointer == "" or pointer.startswith("/"), "invalid source pointer")
        for part in pointer.split("/")[1:] if pointer else []:
            require(re.search(r"~(?:[^01]|$)", part) is None, "invalid source pointer escape")
            part = part.replace("~1", "/").replace("~0", "~")
            if type(value) is list:
                require(re.fullmatch(r"0|[1-9][0-9]*", part) is not None, "invalid source array index")
                value = value[int(part)]
            else:
                require(type(value) is dict, "source pointer crossed a scalar")
                value = value[part]
        require(digest(encoded(value)) == ref["value_sha256"], "selected source value SHA differs")
        return value


def settings(target: str) -> str:
    return "[settings]\nprofile = black\nline_length = 88\npy_version = " + target.replace(".", "") + "\nsrc_paths = .,./src\n"


def api_recipe(target: str) -> dict:
    return {"profile": "black", "line_length": 88, "py_version": target.replace(".", ""),
            "src_paths": ["<project>", "<project>/src"], "extension": "py"}


def cli_flags(target: str) -> list[str]:
    return ["--profile", "black", "--line-length", "88", "--python-version", target.replace(".", ""),
            "--src", ".", "--src", "./src", "--settings-file", ".isort.cfg"]


def load_plan(repo: Path) -> tuple[dict, list[dict], Sources]:
    sources = Sources(repo)
    lock = read_json(checked_bytes(sources.read(LOCK), LOCK_SHA))
    require(lock["schema"] == "mamba.q0.16.isort-contract-lock.v1" and len(lock["requests"]) == 504,
            "isort sealed context denominator/schema differs")
    require(lock["python_versions"] == list(VERSIONS.values()), "isort runtime targets differ")
    for path, sha in {**lock["helper_sources"], **lock["comparison_owners"]}.items(): sources.read(path, sha)
    for key in ("upstream_lock", "mac_wheel_lock", "base_python_lock"):
        sources.read(lock[key]["source_id"], lock[key]["sha256"])
    requests = []
    for entry in lock["requests"]:
        bound = entry["input_binding"]; scalar = sources.value(bound["source"])
        require(type(scalar) is str and bound["representation"] in {"utf8", "hex"}, "input byte representation differs")
        raw = scalar.encode() if bound["representation"] == "utf8" else bytes.fromhex(scalar)
        require(len(raw) == bound["bytes"] and digest(raw) == bound["sha256"], "exact input bytes differ")
        filename = safe_path(bound["filename"])
        require("/" not in filename and filename.endswith(".py"), "declared isort filename differs")
        context = copy.deepcopy(entry["context_template"])
        require(context["layout"]["files"][filename]["bytes_hex"] == MARKER, "input marker is missing")
        context["layout"]["files"][filename]["bytes_hex"] = raw.hex()
        require(digest(encoded(context)) == entry["request_id"], "source/entry/operation/target context ID differs")
        target, operation = context["target"], context["operation"]
        require(context["tool"] == "isort" and target in VERSIONS and operation in {"isort_code_API", "isort_file_CLI"}, "unknown isort context")
        require(context["selection"] == {"kind": "single_source", "paths": [filename], "absolute": False}, "entry selection differs")
        layout = context["layout"]
        require(layout == {"files": {filename: {"bytes_hex": raw.hex(), "bytes": len(raw), "sha256": digest(raw)}},
            "symlinks": {}, "declared_directories": [], "modes": {}, "model_packages": {}, "entrypoints": None,
            "absolute_inputs": False, "source_role_name_for_FS_operations": None, "restricted_path": None, "filename_blocker": None}, "unsupported or changed isort layout")
        sources.read(context["profile_source"]["path"], context["profile_source"]["sha256"])
        require(entry["api_recipe"] == api_recipe(target) if operation == "isort_code_API" else
                entry["cli_flags"] == cli_flags(target) and entry["settings_overlay"] == settings(target), "sealed API/CLI configuration differs")
        alias_values = {}
        for alias in entry["source_aliases"]:
            ref = alias["case_source"]; value = sources.value(ref); owner = ref["source_id"]
            require(owner in (API_OWNERS if operation == "isort_code_API" else CLI_OWNERS), "source owner does not own this API/CLI operation")
            key = "source" if "source" in value else "source_hex" if "source_hex" in value else "source_bytes_hex"
            require((value[key].encode() if key == "source" else bytes.fromhex(value[key])) == raw, "source alias input differs")
            alias_values[(owner, ref["json_pointer"])] = value
        expected = []
        for item in entry["expected_bindings"]:
            require(item["target"] == target, "expected target differs")
            ref = item["source"]; owner = ref["source_id"]; value = sources.value(ref)
            ordinal = list(VERSIONS).index(target)
            require(any(ref["json_pointer"] == pointer + "/expected/" + str(ordinal)
                or owner == BASE + "public_cr_stream.json" and ref["json_pointer"] == pointer and case["target"] == target
                for (path, pointer), case in alias_values.items() if path == owner), "expected pointer is not the selected source/target value")
            policy = {"kind": "owner_ast_address" if owner in ADDRESS_KINDS and target != "3.14" else "exact",
                      "owner": owner.removesuffix(".json") + ".rs", "kinds": list(ADDRESS_KINDS.get(owner, ())) if owner in ADDRESS_KINDS and target != "3.14" else []}
            require(item["message_policy"] == policy, "owner-specific message policy differs")
            expected.append({"binding": item, "value": value})
        require(bool(expected), "isort context lacks its expected source values")
        requests.append({"request_id": entry["request_id"], "target": target, "operation": operation,
            "filename": filename, "source_hex": raw.hex(), "context": context, "input_binding": bound,
            "source_aliases": entry["source_aliases"], "expected": expected})
    require(len({row["request_id"] for row in requests}) == 504 and
        dict(Counter(row["target"] for row in requests)) == lock["target_counts"] == {target: 168 for target in VERSIONS} and
        dict(Counter(row["operation"] for row in requests)) == lock["operation_counts"] == {"isort_code_API": 429, "isort_file_CLI": 75}, "isort context identity/counts differ")
    return lock, requests, sources


def require_hosted() -> None:
    capture.pylint_reference.require_host_context(capture.pylint_reference.HOST_CONTEXT)
    require(platform.machine() == "arm64" and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted", "isort requires the hosted Mac ARM64 job")
    require(not os.environ.get("PYTHONPATH") and not os.environ.get("PYTHONHOME"), "ambient Python import path differs")


OBSERVER = r'''
import contextlib, hashlib, importlib.metadata, importlib.util, io, json, pathlib, site, sys
request = json.loads(pathlib.Path(sys.argv[1]).read_bytes())
project, venv = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]).resolve()
if pathlib.Path(sys.prefix).resolve() != venv or not sys.flags.isolated or not sys.flags.dont_write_bytecode or site.ENABLE_USER_SITE:
    raise RuntimeError("observer escaped the isolated prepared interpreter")
spec = importlib.util.find_spec("isort")
if not spec or not spec.origin or not pathlib.Path(spec.origin).resolve().is_relative_to(venv):
    raise RuntimeError("isort import escaped prepared venv")
import isort
from isort.settings import Config
if importlib.metadata.version("isort") != "9.0.2": raise RuntimeError("isort version differs")
raw = bytes.fromhex(request["source_hex"])
result = {"request_id":request["request_id"],"target":request["target"],"operation":"isort_code_API",
    "input_sha256":hashlib.sha256(raw).hexdigest(),"api_calls":0,"output":None,"output_hex":None,
    "error_class":None,"error_module":None,"message":None,"version":isort.__version__}
out, err = io.StringIO(), io.StringIO()
with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
    try:
        config = Config(profile="black",line_length=88,py_version=request["target"].replace(".",""),src_paths=(project,project/"src"))
        result["api_calls"] += 1
        output = isort.code(raw.decode("utf-8"),config=config,extension="py")
        result.update(output=output,output_hex=output.encode("utf-8").hex())
    except Exception as error:
        result.update(error_class=type(error).__name__,error_module=type(error).__module__,message=str(error))
result.update(api_stdout_hex=out.getvalue().encode().hex(),api_stderr_hex=err.getvalue().encode().hex(),
    python=list(sys.version_info[:3]),executable=sys.executable,prefix=sys.prefix,
    isolated=sys.flags.isolated,dont_write_bytecode=sys.flags.dont_write_bytecode,user_site=site.ENABLE_USER_SITE)
modules = {}
stdlib = pathlib.Path(sys.base_prefix)/"lib"/("python%d.%d" % sys.version_info[:2])
for name,module in sorted(sys.modules.items()):
    filename=getattr(module,"__file__",None)
    if not filename or name == "__main__": continue
    path=pathlib.Path(filename).resolve()
    if path.is_relative_to(venv):
        data=path.read_bytes(); modules[name]={"path":str(path),"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()}
    elif not path.is_relative_to(stdlib) or path.is_relative_to(stdlib/"site-packages"):
        raise RuntimeError("observer imported foreign module: "+name)
result["modules"]=modules
print(json.dumps(result,ensure_ascii=True,separators=(",",":")))
'''


def address_message(message: str, kinds: list[str]) -> str | None:
    if type(message) is not str or " object at 0x" not in message: return None
    before, after = message.split(" object at 0x", 1)
    if not any(before.endswith("<ast." + kind) for kind in kinds) or not re.fullmatch(r"[0-9a-fA-F]+>\.", after): return None
    return before + " object at 0xADDRESS>."


def compare_api(actual: dict, request: dict) -> list[dict]:
    raw = bytes.fromhex(request["source_hex"])
    require(actual["request_id"] == request["request_id"] and actual["target"] == request["target"] and
        actual["operation"] == "isort_code_API" and actual["input_sha256"] == digest(raw) and
        type(actual["api_calls"]) is int and actual["api_calls"] == 1, "API result context/call count differs")
    require(actual["api_stdout_hex"] == actual["api_stderr_hex"] == "", "API emitted unexpected stdout/stderr")
    output = actual["output"]; error = actual["error_class"]
    require((type(output) is str and actual["output_hex"] == output.encode().hex() and error is None and actual["message"] is None)
            or output is None and actual["output_hex"] is None and type(error) is str and type(actual["message"]) is str, "API result output/error shape differs")
    projection = int(output.encode() != raw) if output is not None else int(error != "ValueError")
    comparisons = []
    for item in request["expected"]:
        expected, policy = item["value"], item["binding"]["message_policy"]
        message_equal = actual["message"] == expected.get("message")
        if not message_equal and policy["kind"] == "owner_ast_address":
            a, b = address_message(actual["message"], policy["kinds"]), address_message(expected.get("message"), policy["kinds"])
            message_equal = a is not None and a == b
        fields = {"output_bytes": actual["output_hex"] == (expected["output"].encode().hex() if expected.get("output") is not None else None),
            "error_class": error == expected.get("error_class"), "message": message_equal,
            "version": actual["version"] == expected.get("version", "9.0.2") == "9.0.2",
            "source_owned_exit_projection": projection == expected["reference_exit"]}
        comparisons.append({"expected_binding": item["binding"], "fields": fields, "equal": all(fields.values()),
                            "exit_projection_is_process_exit": False, "computed_exit_projection": projection})
    return comparisons


def cli_expected(request: dict) -> dict:
    values = []
    for item in request["expected"]:
        expected = item["value"]; owner = item["binding"]["source"]["source_id"]
        if owner.endswith("/insertion_markers.json"):
            require(expected["first_output_hex"] == expected["stable_output_hex"], "one-pass and stable outputs differ")
            value = {"output_hex": expected["first_output_hex"], "exits": [expected["source_check_exit"],expected["first_format_exit"],expected["stable_check_exit"]]}
        elif owner.endswith("/public_cr_stream.json"):
            require(all(len(expected[key]) == 2 and len(set(expected[key])) == 1 for key in
                ("public_initial_check_exits","public_stable_format_exits","public_stable_check_exits")), "public CR repeated source facts disagree")
            value = {"output_hex":expected["public_stable_bytes_hex"],"exits":[expected["public_initial_check_exits"][0],expected["public_stable_format_exits"][0],expected["public_stable_check_exits"][0]]}
        else:
            value = {"output_hex":expected["output_hex"],"exits":[expected["check_exit"],expected["format_exit"],expected["after_check_exit"]]}
        values.append(value)
    require(bool(values) and all(value == values[0] for value in values), "CLI aliases disagree on output/actual stage exits")
    return values[0]


def process(argv: list[str], cwd: Path, destination: Path, inputs: dict, exits: tuple[int, ...], records: list[dict]) -> dict:
    row = {"status":"failed","capture_process_fact":None,"receipt":None,"returned_receipt":None,"errors":[],"artifact_dir":str(destination)}
    row["directory_from_context"]=destination.name
    invocation={"argv":argv,"cwd":str(cwd),"inputs":inputs,"expected_exits":list(exits),"artifact_dir":str(destination)}
    invocation_bytes=capture.strict_json_bytes(invocation)
    row["captured_invocation_json"]=invocation_bytes.decode();row["captured_invocation_sha256"]=digest(invocation_bytes)
    records.append(row)
    try:
        returned = run_command(argv,cwd,destination,expected_exits=exits,timeout=120,inputs=inputs)
        row["capture_process_fact"] = {key:returned.get(key) for key in ("pid","waited_pid","wait_completed","actual_exit","timed_out","launch_error")}
        write_json(destination / "returned-receipt.json",returned)
        row["returned_receipt"] = read_json((destination / "returned-receipt.json").read_bytes())
        normalized=dict(returned);normalized["expected_exits"]=list(exits)
        capture.require_saved_receipt(returned,normalized,exits)
        captured_bytes=capture.strict_json_bytes(normalized)
        row["captured_return_json"]=captured_bytes.decode();row["captured_return_sha256"]=digest(captured_bytes)
        row["receipt"] = read_json((destination / "receipt.json").read_bytes())
        receipt = capture.require_saved_receipt(returned,row["receipt"],exits)
        require(type(receipt.get("waited_pid")) is int,"waited PID is not an integer")
        require_receipt(receipt,exits)
        require(receipt["argv"] == argv and receipt["cwd"] == str(cwd),"actual process argv/cwd differs")
        for name in ("stdout","stderr","stdin","inputs"):
            path = destination / (name + (".json" if name == "inputs" else ".raw"))
            require(digest(path.read_bytes()) == receipt[name+"_sha256"],"actual raw/input hash differs")
        require(capture.strict_json_bytes(read_json((destination/"inputs.json").read_bytes())) == capture.strict_json_bytes(inputs),"actual process inputs differ")
        row["status"]="passed"
    except Exception as error:
        row["errors"].append({"type":type(error).__name__,"message":str(error)})
    row["raw_pointers"]={name:destination.name+"/"+name for name in
        ("receipt.json","returned-receipt.json","inputs.json","stdin.raw","stdout.raw","stderr.raw") if (destination/name).is_file()}
    return row


def reconcile_process(record: dict) -> None:
    """Re-read final files against immutable strings captured from the API return."""
    anchors={}
    for name in ("invocation","return"):
        raw=record["captured_"+name+"_json"].encode()
        require(digest(raw)==record["captured_"+name+"_sha256"],"captured process anchor changed")
        anchors[name]=read_json(raw)
    invocation,original=anchors["invocation"],anchors["return"]
    require(type(invocation["expected_exits"]) is list and all(type(code) is int for code in invocation["expected_exits"]),"captured exit contract differs")
    exits=tuple(invocation["expected_exits"]);returned=dict(original);returned["expected_exits"]=exits
    for name in ("returned_receipt","receipt"):
        require(capture.strict_json_bytes(record[name])==capture.strict_json_bytes(original),"in-memory process receipt differs from captured actual return")
    destination=Path(invocation["artifact_dir"])
    require(record["artifact_dir"]==str(destination) and record["directory_from_context"]==destination.name,"process artifact context changed")
    for name in ("returned-receipt.json","receipt.json"):
        capture.fact(destination/name)
        capture.require_saved_receipt(returned,read_json((destination/name).read_bytes()),exits)
    require(type(original.get("waited_pid")) is int,"actual waited PID is not an integer")
    require_receipt(original,exits)
    facts={key:original.get(key) for key in ("pid","waited_pid","wait_completed","actual_exit","timed_out","launch_error")}
    require(capture.strict_json_bytes(record["capture_process_fact"])==capture.strict_json_bytes(facts),"known captured child facts changed")
    require(capture.strict_json_bytes(original["argv"])==capture.strict_json_bytes(invocation["argv"])
        and original["cwd"]==invocation["cwd"],"original process argv/cwd changed")
    for name in ("stdout","stderr","stdin","inputs"):
        path=destination/(name+(".json" if name=="inputs" else ".raw"));capture.fact(path)
        require(digest(path.read_bytes())==original[name+"_sha256"],"final raw/input bytes differ from captured return")
    require((destination/"stdin.raw").read_bytes()==b"","final process stdin differs from DEVNULL")
    inputs=invocation["inputs"]
    require(capture.strict_json_bytes(read_json((destination/"inputs.json").read_bytes()))==capture.strict_json_bytes(inputs),"original process input context changed")
    if "observer_sha256" in inputs:
        script,packet=Path(invocation["argv"][3]),Path(invocation["argv"][4])
        capture.fact(script);capture.fact(packet)
        require(digest(script.read_bytes())==inputs["observer_sha256"] and
            capture.strict_json_bytes(read_json(packet.read_bytes()))==capture.strict_json_bytes(inputs["request"]),"final observer source/request input changed")
        for name,value in inputs["layout"]["files"].items():
            path=Path(invocation["cwd"])/safe_path(name);capture.fact(path)
            require(path.read_bytes()==bytes.fromhex(value["bytes_hex"]),"final API source bytes changed")
    if "source_hex" in inputs and "settings_hex" in inputs:
        snapshot=destination.parent/(destination.name+"-input.raw");settings_file=Path(invocation["cwd"])/".isort.cfg"
        capture.fact(snapshot);capture.fact(settings_file)
        require(snapshot.read_bytes()==bytes.fromhex(inputs["source_hex"]) and settings_file.read_bytes()==bytes.fromhex(inputs["settings_hex"]),"final CLI source/settings input proof changed")


def module_identity(actual: dict, identity: dict, root: Path, executable: Path, version: str) -> None:
    venv=root/"venv"; site=Path(identity["module"]).parent.parent
    require(capture.strict_json_bytes(actual["python"])==capture.strict_json_bytes(list(map(int,version.split(".")))) and actual["executable"]==str(executable)
        and Path(actual["prefix"]).resolve()==venv.resolve() and type(actual["isolated"]) is int and actual["isolated"]==1
        and type(actual["dont_write_bytecode"]) is int and actual["dont_write_bytecode"]==1 and actual["user_site"] is False,"observer interpreter identity differs")
    expected={name:fact for distribution in [identity,*identity["dependency_distributions"].values()] for name,fact in distribution["distribution_files"].items()}
    require("isort" in actual["modules"],"observer did not attest isort")
    for row in actual["modules"].values():
        path=Path(row["path"]); require(path.is_relative_to(venv) and expected.get(path.relative_to(site).as_posix())==capture.fact(path)=={"bytes":row["bytes"],"sha256":row["sha256"]},"observer module escaped pinned installed payload")


def execute(request: dict, executable: Path, identity: dict, environment: Path, root: Path) -> dict:
    root.mkdir(parents=True,exist_ok=False); project=root/"project"
    raw=bytes.fromhex(request["source_hex"]); filename=request["filename"]
    files={filename:raw}
    if request["operation"]=="isort_file_CLI":files[".isort.cfg"]=settings(request["target"]).encode()
    write_sources(project,files); (root/"input.raw").write_bytes(raw)
    row={"request_id":request["request_id"],"target":request["target"],"operation":request["operation"],
        "context_dir":"contexts/"+request["request_id"],
        "status":"failed","processes":[],"actual_api_calls":None,"comparisons":None,"errors":[],"source_aliases":request["source_aliases"],
        "expected_bindings":[item["binding"] for item in request["expected"]],"issue_accepted":False,"native_gate_status":"not_run"}
    try:
        if request["operation"]=="isort_code_API":
            payload={key:request[key] for key in ("request_id","target","source_hex")}
            write_json(root/"request.json",payload)
            script=root/"observer.py"; script.write_text(OBSERVER)
            result=process([str(executable),"-I","-B",str(script),str(root/"request.json"),str(project),str(environment/"venv")],project,root/"api",
                {"request":payload,"observer_sha256":digest(OBSERVER.encode()),"layout":request["context"]["layout"]},(0,),row["processes"])
            require(result["status"]=="passed","API child did not provide a verified terminal receipt")
            require((root/"api/stderr.raw").read_bytes()==b"","API observer produced unexpected stderr")
            actual=read_json((root/"api/stdout.raw").read_bytes()); row["actual"]=actual
            row["actual_api_calls"]=actual.get("api_calls") if type(actual.get("api_calls")) is int and actual["api_calls"]>=0 else None
            module_identity(actual,identity,environment,executable,VERSIONS[request["target"]])
            row["comparisons"]=compare_api(actual,request)
        else:
            expected=cli_expected(request); stages=[]
            for index,name in enumerate(STAGES):
                before=(project/filename).read_bytes(); command=[str(executable),"-I","-B","-m","isort",*cli_flags(request["target"])]
                if name!="format":command.append("--check-only")
                command.append(filename)
                result=process(command,project,root/name,{"request_id":request["request_id"],"source_hex":before.hex(),"settings_hex":files[".isort.cfg"].hex()},(0,1),row["processes"])
                after=(project/filename).read_bytes(); (root/(name+"-input.raw")).write_bytes(before);(root/(name+"-output.raw")).write_bytes(after)
                stdout,stderr=(root/name/"stdout.raw").read_bytes(),(root/name/"stderr.raw").read_bytes()
                exit_value=(result.get("receipt") or {}).get("actual_exit")
                wanted=bytes.fromhex(expected["output_hex"]) if name!="check-source" else raw
                # Only this exact source-owned single-file path envelope is bound.
                wanted_out=(f"Fixing {project/filename}\n".encode() if name=="format" and before!=after else b"")
                wanted_err=(f"ERROR: {project/filename} Imports are incorrectly sorted and/or formatted.\n".encode()
                            if name!="format" and expected["exits"][index]==1 else b"")
                checks={"process_receipt":result["status"]=="passed","actual_exit":type(exit_value) is int and exit_value==expected["exits"][index],
                    "complete_output_bytes":after==wanted,"check_did_not_change_bytes":name=="format" or before==after,
                    "exact_settings_preserved":(project/".isort.cfg").read_bytes()==files[".isort.cfg"],
                    "stdout_single_file_envelope":stdout==wanted_out,"stderr_single_file_envelope":stderr==wanted_err}
                stages.append({"stage":name,"fields":checks,"equal":all(checks.values()),"raw_streams_normalized":False})
            row["comparisons"]=stages;row["actual_api_calls"]=0
        require(row["comparisons"] and all(item["equal"] for item in row["comparisons"]),"source-bound output/error/exit comparison failed")
        row["status"]="observed"
    except Exception as error:
        row["errors"].append({"type":type(error).__name__,"message":str(error)})
    try:
        current=capture.artifact_inventory(project,[])
        allowed={name:{"kind":"regular","bytes":len(content),"sha256":digest(content)} for name,content in files.items()}
        if request["operation"]=="isort_file_CLI":
            content=(project/filename).read_bytes();allowed[filename]={"kind":"regular","bytes":len(content),"sha256":digest(content)}
        row["source_config_boundary_preserved"]=current==allowed
        require(row["source_config_boundary_preserved"],"source/config boundary changed")
    except Exception as error:
        row["source_config_boundary_preserved"]=False;row["status"]="failed"
        row["errors"].append({"stage":"source_preservation","type":type(error).__name__,"message":str(error)})
    write_json(root/"observation.json",row)
    return row


def payloads(environment: Path, identity: dict, phase: str, python: Path) -> dict:
    site=Path(identity["module"]).parent.parent;venv=(environment/"venv").resolve();retained=environment/"retained-payloads";retained.mkdir(exist_ok=True)
    rows={};errors=[]
    for distribution in [identity,*identity["dependency_distributions"].values()]:
        for relative,expected in distribution["distribution_files"].items():
            try:
                path=(site/relative).resolve(strict=True);require(path.is_relative_to(venv),"installed payload escaped environment")
                raw=path.read_bytes();actual={"bytes":len(raw),"sha256":digest(raw)}
                destination=retained/(actual["sha256"]+".raw")
                if not destination.exists():destination.write_bytes(raw)
                require(destination.read_bytes()==raw,"retained installed payload differs")
                rows[relative]={"expected":expected,"actual":actual,"retained":destination.relative_to(environment).as_posix()}
                require(actual==expected,"installed payload changed")
            except Exception as error:errors.append({"path":relative,"type":type(error).__name__,"message":str(error)})
    if not rows or not any(name.endswith(".dist-info/RECORD") for name in rows):
        errors.append({"type":"ValueError","message":"package/RECORD denominator is empty"})
    result={"package_files":rows,"base":capture.fact(python.resolve(strict=True)),"venv_python":capture.fact(environment/"venv/bin/python"),
            "tree":capture.artifact_inventory(environment/"venv",[]),"errors":errors}
    for label,path in (("base",python.resolve(strict=True)),("venv_python",environment/"venv/bin/python")):
        raw=path.read_bytes();destination=retained/(digest(raw)+".raw")
        if not destination.exists():destination.write_bytes(raw)
        require(destination.read_bytes()==raw,"retained interpreter differs")
    result["matches_identity"]=not errors and result["base"]["sha256"]==identity["base_executable_sha256"] and result["venv_python"]["sha256"]==identity["venv_executable_sha256"]
    write_json(environment/("payloads-"+phase+".json"),result)
    return result


def planned_rows(requests: list[dict]) -> list[dict]:
    return [{"request_id":r["request_id"],"target":r["target"],"operation":r["operation"],"status":"not_run",
        "source_aliases":r["source_aliases"],"expected_bindings":[item["binding"] for item in r["expected"]],
        "processes":[],"actual_api_calls":None,"comparisons":None,"errors":[],"issue_accepted":False,"native_gate_status":"not_run"} for r in requests]


def publish(output: Path, rows: list[dict], passed_context: bool) -> None:
    require(len(rows)==168 and len({row["request_id"] for row in rows})==168,"final intended-lane denominator differs")
    for row in rows:
        require(re.fullmatch(r"[a-f0-9]{64}",row["request_id"]) is not None,"unsafe final context identity")
        folder=output/"contexts"/row["request_id"];folder.mkdir(parents=True,exist_ok=True)
        initial=folder/"observation.json";row["observation_pointer"]=initial.relative_to(output).as_posix() if initial.exists() else None
        for process_record in row["processes"]:
            try:
                reconcile_process(process_record)
                process_record["final_proof_reconciled"]=True
            except Exception as error:
                process_record["final_proof_reconciled"]=False;process_record["status"]="failed";row["status"]="failed"
                finding={"stage":"final_process_proof","type":type(error).__name__,"message":str(error)}
                process_record["errors"].append(finding);row["errors"].append(dict(finding))
        row["capture_context_passed"]=passed_context
        if row["status"]=="observed" and not passed_context:row["status"]="failed"
        final=folder/"final-result.json";row["result_pointer"]=final.relative_to(output).as_posix()
        capture.strict_json_bytes(row);write_json(final,row)
    write_json(output/"final-results.json",rows)
    with (output/"raw-index.jsonl").open("x") as stream:
        for row in rows:stream.write(json.dumps(row,ensure_ascii=True,allow_nan=False)+"\n")
    require(capture.strict_json_bytes(read_json((output/"final-results.json").read_bytes()))==capture.strict_json_bytes(rows)
        and capture.strict_json_bytes([read_json(line) for line in (output/"raw-index.jsonl").read_bytes().splitlines()])==capture.strict_json_bytes(rows)
        and all(capture.strict_json_bytes(read_json((output/row["result_pointer"]).read_bytes()))==capture.strict_json_bytes(row) for row in rows),"authoritative final results and indexes disagree")


def process_counts(rows: list[dict]) -> dict:
    processes=[p for row in rows for p in row["processes"]];facts=[p.get("capture_process_fact") or {} for p in processes]
    known=lambda fact:type(fact.get("pid")) is int and fact["pid"]>0
    return {"actual_target_processes":sum(known(fact) for fact in facts),
        "waited_target_processes":sum(known(fact) and type(fact.get("waited_pid")) is int and fact.get("waited_pid")==fact["pid"] and fact.get("wait_completed") is True and type(fact.get("actual_exit")) is int for fact in facts),
        "verified_process_receipts":sum(p["status"]=="passed" for p in processes),
        "actual_isort_code_calls":sum(row["actual_api_calls"] for row in rows if type(row.get("actual_api_calls")) is int and row["actual_api_calls"]>=0)}


def run(repo: Path, output: Path, python: Path, version: str) -> int:
    output.mkdir(parents=True,exist_ok=False);lock,requests,sources=load_plan(repo)
    target=".".join(version.split(".")[:2]);require(version==VERSIONS.get(target),"exact target Python version differs")
    selected=[r for r in requests if r["target"]==target];rows=planned_rows(selected)
    require(Counter(r["operation"] for r in selected)=={"isort_code_API":143,"isort_file_CLI":25},"intended-lane operation counts differ")
    write_json(output/"plan.json",{"all_target_contexts":504,"selected_target_contexts":168,"contexts":rows})
    write_json(output/"initial-results.json",rows)
    errors=[];identity=None;before=None;environment=output/"environment";context=False
    try:
        require_hosted();require(python.is_absolute(),"base interpreter path must be absolute")
        upstream=read_json(sources.read(lock["upstream_lock"]["source_id"]))
        wheels=read_json(sources.read(lock["mac_wheel_lock"]["source_id"]))["dependencies"]["isort"]
        base=read_json(sources.read(lock["base_python_lock"]["source_id"]))
        checked_bytes(python.resolve(strict=True).read_bytes(),base["interpreters"][version]["executable_sha256"])
        download(upstream["sdist"],output/"downloads/sdist")
        dependencies=[]
        for index,pin in enumerate(upstream["dependencies"]):
            destination=output/"downloads"/("dependency-"+str(index));download(pin,destination);dependencies.append(destination/pin["filename"])
        pin=wheels["wheels"][target.split(".")[1]];download(pin,output/"downloads/wheel")
        executable=reference.prepare(python,version,environment,output/"downloads/wheel"/pin["filename"],upstream["dependencies"],dependencies)
        identity=read_json((environment/"identity.json").read_bytes());before=payloads(environment,identity,"before",python)
        require(before["matches_identity"],"prepared payload/base/RECORD identity is incomplete")
        write_json(output/"run-identity.json",{"source_sha":os.environ.get("GITHUB_SHA"),"run_id":os.environ.get("GITHUB_RUN_ID"),
            "run_attempt":os.environ.get("GITHUB_RUN_ATTEMPT"),"python_version":version,"lock_sha256":digest((repo/LOCK).read_bytes()),
            "selected_contexts":168,"api_contexts":143,"cli_contexts":25,"planned_target_processes":218,
            "new_host_context":"macos-15 ARM64","wheel_source":lock["mac_wheel_lock"],"source_distribution":lock["upstream_lock"],
            "dependency_artifacts":identity["dependency_artifacts"],"historical_environment":False,"issue_accepted":False})
        for index,request in enumerate(selected):
            try:rows[index]=execute(request,executable,identity,environment,output/"contexts"/request["request_id"])
            except Exception as error:
                rows[index]["status"]="failed";rows[index]["errors"].append({"stage":"context_controller","type":type(error).__name__,"message":str(error)})
        after=payloads(environment,identity,"after",python)
        require(after["matches_identity"] and capture.strict_json_bytes(before)==capture.strict_json_bytes(after),"prepared payload/base/RECORD/tree changed")
        require(all((repo/path).read_bytes()==raw for path,raw in sources.raw.items()),"sealed repository sources changed")
        context=True
    except Exception as error:errors.append({"type":type(error).__name__,"message":str(error)})
    if identity is not None and not (environment/"payloads-after.json").exists():
        try:payloads(environment,identity,"after",python)
        except Exception as error:errors.append({"stage":"post_capture_payloads","type":type(error).__name__,"message":str(error)})
    publish(output,rows,context)
    counts=process_counts(rows)
    matched=bool(context and not errors and len(rows)==168 and all(row["status"]=="observed" for row in rows)
                 and all(counts[key]==218 for key in ("actual_target_processes","waited_target_processes","verified_process_receipts")) and counts["actual_isort_code_calls"]==143)
    summary={"schema":"mamba.q0.16.isort-contract-summary.v1","contract_capture_passed":matched,"all_planned_contexts":504,
        "selected_target_contexts":168,"observed_contexts":sum(row["status"]=="observed" for row in rows),**counts,"errors":errors,
        "api_exit_projections_are_process_exits":False,"native_gate_status":"not_run","issue_accepted":False,
        "artifact_exclusions":["environment/venv"],"historical_original_evidence_recovered":False}
    write_json(output/"summary.json",summary)
    write_json(output/"artifact-manifest.json",capture.artifact_inventory(output,[environment/"venv"]))
    print(json.dumps({key:summary[key] for key in ("contract_capture_passed","selected_target_contexts","actual_target_processes","actual_isort_code_calls","issue_accepted")}))
    return 0 if matched else 1


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest="mode",required=True)
    plan=sub.add_parser("plan");plan.add_argument("--repo",type=Path,required=True);plan.add_argument("--output",type=Path)
    command=sub.add_parser("run");command.add_argument("--repo",type=Path,required=True);command.add_argument("--artifact-dir",type=Path,required=True)
    command.add_argument("--python-version",required=True,choices=VERSIONS.values());command.add_argument("--python",type=Path,required=True)
    args=parser.parse_args()
    if args.mode=="run":return run(args.repo.resolve(strict=True),args.artifact_dir.resolve(),args.python,args.python_version)
    lock,requests,_=load_plan(args.repo.resolve(strict=True));result={"schema":"mamba.q0.16.isort-contract-plan.v1","requests":planned_rows(requests),
        "all_planned_contexts":504,"target_counts":lock["target_counts"],"operation_counts":lock["operation_counts"],"actual_reference_execution":"not_run","issue_accepted":False}
    if args.output:write_json(args.output,result)
    print(json.dumps({key:value for key,value in result.items() if key!="requests"}));return 0


if __name__=="__main__":raise SystemExit(main())
