#!/usr/bin/env python3
"""Capture finite source-owned mypy and Pylint contracts on hosted Macs.

Fixture programs are static checker inputs. This runner never executes them.
Reference observations do not accept any native or parent issue gate.
"""
from __future__ import annotations

import argparse
import ast
import copy
import errno
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shlex
import stat
import sys
import zipfile

import q0_16_reference as capture
from q0_16_packet import CONFIG_BYTES, read_json, safe_path
from q0_mypy_reference import require_receipt
from q0_ruff_reference import digest, run_command, write_json

REGISTRY = "docs/operations/python-quality/q0-16-required-behavior-registry.json"
REGISTRY_SHA = "a9523a75ff4777e7884a4f2ef5833ca0e41f16b2434b7bee63e83081a1846c40"
LOCK = "tools/ci/q0_16_contract_lint_lock.json"
BASELINE = "1413abd7e91bd6289a9f5761734821395b66f4ba"
COUNTS = {"mypy": 2982, "pylint": 1380}
VERSIONS = capture.VERSIONS
TARGETS = ["3.12", "3.13", "3.14"]
TIMEOUT = 600
PYPROJECT = b'[project]\nname = "fixture"\nversion = "0.0.0"\nrequires-python = ">=3.12"\n'
NORMAL_FIELDS = {"rule", "message", "hint", "path", "severity", "location"}
RAW_MYPY_FIELDS = {"file", "line", "column", "end_line", "end_column", "message", "hint", "code", "severity"}
MYPY_MAIN_SHA = "13c5661811f3d11d51ce2c7a4cc9a8451a3c72f1f7629adf61198d0b0bef8ace"
MYPY_UTIL_SHA = "961d74878dd3736f7025292a3fb50973dea5a67cb1e4820895f4d5f4778f36b3"
PYINFO_SHA = "cd19c696e92a9de347b95facb8331e7709c405c5e380dc5577098d42d2e84bf4"
MODULEFINDER_SHA = "09ff2266e5ac2b827c69066bbd2fef5471a30dc8d6dd48d1aa92925a4624549e"
MYPY_BLOCKER_SOURCES = {
    "mypy/fastparse.py": {"sha256": "9bea678209bab98c6f6b7aeed0174905be3c9a456b1ca0730d613c29a8e0f5d7", "line_spans": [[260, 270]]},
    "mypy/build.py": {"sha256": "61e1295a840159ea32f78dcc2fe253c49be7d1915f47551ad74be91be76df641", "line_spans": [[932, 937]]},
}
MYPY_NONBLOCKER_SOURCES = {
    "mypy/nativeparse.py": {"sha256": "6621e8f6e863dbc6e6bee8139528a0d76ec6df8c91a6abbf6f9dd2444360bc3a", "line_spans": [[192, 208]]},
}
PYLINT_ROOT_ROWS = {"cross_file_import_precedence.json": {2, 3, 4, 5, 6, 8, 9, 11, 12, 13, 14, 16, 18, 19},
    "dependency_io.json": set(range(6, 15))}
PYLINT_DIRECTORY_ROWS = {"cross_file_import_precedence.json": {17, 18, 19, 30},
    "cross_file_relative_boundaries.json": {6}, "cross_file_selected_imports.json": {1}, "cross_file_stubs.json": {7, 10, 11}}
PYLINT_RECIPE_SOURCES = {
    "crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stores.rs": "d10f43b946f544b46880eb3051e56dc8d64ae7e1b6477f387579a6bc30498875",
    "crates/mamba-quality/src/infrastructure/metadata.rs": "559f8fff1f5007a9303b86bf08472e280a5aa32289dc1994f3459cf11fa370d0",
    "crates/mamba-quality/src/infrastructure/filesystem.rs": "2374693d91c50293239ec53d076529d841c6167af1397d1bf44ce6c789ce19c9",
    "crates/mamba-quality/src/infrastructure/pylint/runner.rs": "5d4dd09b66c48ed289c3675f14a530ae25de9fad2ebefdef26922aa9af02f10a"}
PYLINT_INIT_HOOK_SOURCE = {"member": "pylint/config/utils.py", "sha256": "563b8a79ff2b4225a81e46ab5f722bf24075956e683f76b4d9e9815d98fad350", "line_spans": [[146, 154]]}
PYLINT_NAMESPACE_SOURCE = {"member": "pylint/lint/expand_modules.py", "sha256": "924ba931f2bea27008d2e2eeb19fdafed69290acab13767996946e52eca9eb05", "line_spans": [[99, 108]]}
REFERENCE_LIBRARY_SELECTIONS = {
    "pydantic": {"prefix": "pydantic/", "excluded": ["pydantic/mypy.py", "pydantic/v1/mypy.py", "pydantic/v1/_hypothesis_plugin.py"], "files": 104},
    "pydantic_core": {"prefix": "pydantic_core/", "excluded": [], "files": 4},
    "annotated-types": {"prefix": "annotated_types/", "excluded": ["annotated_types/test_cases.py"], "files": 2},
    "typing-inspection": {"prefix": "typing_inspection/", "excluded": [], "files": 5},
    "typing_extensions": {"exact": "typing_extensions.py", "excluded": [], "files": 1}}
METADATA_QUERY_GUARD = r'''import contextlib, hashlib, io, json, os, pathlib, site, sys, traceback

def validate_query_args(actual, helper):
    if actual != [helper, "getsearchdirs"]:
        raise ValueError("metadata transport only accepts the pinned helper/getsearchdirs query")

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

policy = json.loads(pathlib.Path(sys.argv[1]).read_bytes())
args = sys.argv[2:]
root = pathlib.Path(policy["proof_root"])
prefix = root / str(os.getpid())
record = {"pid_self_reported": os.getpid(), "parent_pid_self_reported": os.getppid(),
    "parent_wait_claimed": False, "completion_self_reported": False,
    "transport_argv": sys.argv, "real_python": sys.executable,
    "python": list(sys.version_info[:3]), "prefix": sys.prefix, "base_prefix": sys.base_prefix,
    "isolated": sys.flags.isolated, "safe_path": sys.flags.safe_path,
    "dont_write_bytecode": sys.flags.dont_write_bytecode, "user_site": site.ENABLE_USER_SITE}
with pathlib.Path(str(prefix) + ".started.json").open("x") as stream:
    json.dump(record, stream, ensure_ascii=True, sort_keys=True)
out, err = io.BytesIO(), io.BytesIO()
stdout = io.TextIOWrapper(out, encoding="utf-8", write_through=True)
stderr = io.TextIOWrapper(err, encoding="utf-8", write_through=True)
exit_code = 0
try:
    validate_query_args(args, policy["helper"])
    if (sys.executable != policy["real_python"] or sys.prefix != policy["venv"]
            or list(sys.version_info[:3]) != policy["python"] or sys.flags.isolated != 1
            or not sys.flags.safe_path or not sys.flags.dont_write_bytecode or site.ENABLE_USER_SITE):
        raise ValueError("metadata query escaped its real isolated interpreter")
    helper = pathlib.Path(policy["helper"])
    raw = helper.read_bytes()
    if sha(raw) != policy["helper_sha256"]:
        raise ValueError("pinned metadata helper bytes changed")
    stdlib = pathlib.Path(sys.base_prefix) / "lib" / ("python%d.%d" % sys.version_info[:2])
    allowed_paths = {str(stdlib), str(stdlib / "lib-dynload"), str(stdlib.parent / ("python%d%d.zip" % sys.version_info[:2])), policy["site"]}
    if any(path not in allowed_paths for path in sys.path) or policy["site"] not in sys.path:
        raise ValueError("metadata query search path contains foreign or checker code")
    fixture_roots = [pathlib.Path(path).resolve() for path in policy["fixture_roots"]]
    def audit(event, values):
        if event == "exec":
            filename = values[0].co_filename
            if not filename.startswith("<") and any(pathlib.Path(filename).resolve().is_relative_to(path) for path in fixture_roots):
                raise RuntimeError("fixture body execution is outside the metadata query")
        elif event == "import" and len(values) > 1 and isinstance(values[1], str):
            if any(pathlib.Path(values[1]).resolve().is_relative_to(path) for path in fixture_roots):
                raise RuntimeError("fixture import is outside the metadata query")
    sys.addaudithook(audit)
    record["actual_sys_path"] = list(sys.path)
    record["helper_sha256"] = sha(raw)
    record["helper_bytes"] = len(raw)
    record["fixture_execution_guard"] = True
    sys.argv = list(args)
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        exec(compile(raw, str(helper), "exec"), {"__name__": "__main__", "__file__": str(helper), "__package__": None, "__spec__": None})
    if helper.read_bytes() != raw:
        raise ValueError("metadata helper changed during query")
    modules = {}
    for name, module in sorted(sys.modules.copy().items()):
        filename = getattr(module, "__file__", None)
        if not filename:
            continue
        path = pathlib.Path(filename).resolve()
        if not (path.is_relative_to(stdlib) or path == pathlib.Path(__file__).resolve()) or path.is_relative_to(stdlib / "site-packages"):
            raise ValueError("metadata query imported a foreign module: " + name)
        if any(path.is_relative_to(fixture) for fixture in fixture_roots):
            raise ValueError("metadata query imported a fixture module: " + name)
        data = path.read_bytes()
        modules[name] = {"path": str(path), "bytes": len(data), "sha256": sha(data)}
    record["module_origins"] = modules
except BaseException as error:
    exit_code = error.code if isinstance(error, SystemExit) and type(error.code) is int else 1
    if exit_code == 0:
        exit_code = 1
    traceback.print_exc(file=stderr)
    record["error_type"] = type(error).__name__
    record["error_message"] = str(error)
stdout.flush(); stderr.flush()
raw_out, raw_err = out.getvalue(), err.getvalue()
for suffix, raw_stream in (("stdout.raw", raw_out), ("stderr.raw", raw_err)):
    with pathlib.Path(str(prefix) + "." + suffix).open("xb") as stream:
        stream.write(raw_stream)
record.update(completion_self_reported=True, self_reported_exit=exit_code,
    stdout_bytes=len(raw_out), stdout_sha256=sha(raw_out), stderr_bytes=len(raw_err), stderr_sha256=sha(raw_err))
with pathlib.Path(str(prefix) + ".completed.json").open("x") as stream:
    json.dump(record, stream, ensure_ascii=True, sort_keys=True)
sys.stdout.buffer.write(raw_out); sys.stdout.buffer.flush()
sys.stderr.buffer.write(raw_err); sys.stderr.buffer.flush()
raise SystemExit(exit_code)
'''


def encode(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"), allow_nan=False).encode()


def value_sha(value: object) -> str:
    return digest(encode(value))


def tokens(pointer: str) -> list[str]:
    if pointer == "":
        return []
    if not pointer.startswith("/") or re.search(r"~(?![01])", pointer):
        raise ValueError("invalid source JSON pointer")
    return [part.replace("~1", "/").replace("~0", "~") for part in pointer[1:].split("/")]


def select(value: object, pointer: str) -> object:
    for part in tokens(pointer):
        if isinstance(value, list):
            if not re.fullmatch(r"0|[1-9][0-9]*", part):
                raise ValueError("invalid source array index")
            value = value[int(part)]
        else:
            value = value[part]
    return value


class Sources:
    """Read repository JSON and Rust JSON constants without importing fixtures."""

    def __init__(self, repo: Path):
        self.repo, self.files, self.objects = repo, {}, {}

    def raw(self, path: str) -> bytes:
        safe_path(path)
        if path not in self.files:
            self.files[path] = capture.fact(self.repo / path)
        raw = (self.repo / path).read_bytes()
        if digest(raw) != self.files[path]["sha256"]:
            raise ValueError("source changed during plan construction")
        return raw

    def object(self, path: str, constant: str | None = None) -> object:
        key = path, constant
        if key not in self.objects:
            raw = self.raw(path)
            if constant:
                matches = list(re.finditer(rb"pub const " + constant.encode() +
                    rb':\s*&str\s*=\s*r(?P<h>#+)"(?P<b>.*?)"(?P=h);', raw, re.S))
                if len(matches) != 1:
                    raise ValueError("one source-owned Rust JSON constant is required")
                raw = matches[0]["b"]
            self.objects[key] = read_json(raw)
        return self.objects[key]

    def resolve(self, ref: dict) -> object:
        raw = self.raw(ref["source_id"])
        if digest(raw) != ref["sha256"]:
            raise ValueError("source binding SHA differs")
        value = select(self.object(ref["source_id"], ref.get("constant")), ref["json_pointer"])
        if "value_sha256" in ref and value_sha(value) != ref["value_sha256"]:
            raise ValueError("selected source value SHA differs")
        return value

    def child(self, ref: dict, *parts) -> dict:
        pointer = ref["json_pointer"] + "".join("/" + str(p).replace("~", "~0").replace("/", "~1") for p in parts)
        result = {"source_id": ref["source_id"], "sha256": digest(self.raw(ref["source_id"])), "json_pointer": pointer}
        if ref.get("constant"):
            result["constant"] = ref["constant"]
        result["value_sha256"] = value_sha(self.resolve(result))
        return result


def payload(raw: bytes) -> dict:
    return {"bytes_hex": raw.hex(), "bytes": len(raw), "sha256": digest(raw)}


def scalar_filename(value: dict, index: int, owner: str) -> str:
    if isinstance(value.get("filename"), str):
        return value["filename"]
    if "case_{index}.py" in owner or "case_{case_index}.py" in owner:
        return f"case_{index}.py"
    candidates = re.findall(r'\.join\("([^"\n]+\.pyi?)"\)', owner)
    for name in ("candidate.py", "main.py"):
        if name in candidates:
            return name
    raise ValueError("scalar filename has no source-owned materialization")


def layout_for(sources: Sources, manifest: dict, record: dict, value: dict, owner: str) -> dict:
    files, links, modes = {}, {}, {}
    declared = value.get("files", {})
    for name, item in ((x.get("path", x.get("name")), x) for x in declared) if isinstance(declared, list) else declared.items():
        raw = item.encode() if isinstance(item, str) else item["text"].encode() if isinstance(item.get("text"), str) else bytes(item["bytes"])
        files[name] = payload(raw)
    for name, item in value.get("entries", {}).items():
        if item["kind"] == "symlink":
            links[name] = item["link_target"]
        else:
            files[name] = payload(bytes.fromhex(item["bytes_hex"]))
            modes[name] = item.get("mode")
    links.update(value.get("symlinks", {}))
    files.update({name: payload(text.encode()) for name, text in value.get("local", {}).items()})
    for key in ("source", "source_utf8", "source_hex", "source_bytes_hex"):
        if isinstance(value.get(key), str):
            name = scalar_filename(value, record.get("legacy_record_entry_index", 0), owner)
            files[name] = payload(bytes.fromhex(value[key]) if key.endswith("hex") else value[key].encode())
            break
    if not files:
        for item in record.get("input_bindings", []):
            ref = item["pointer"]
            raw = sources.raw(ref["source_id"]) if ref["source_id"].endswith(".py") else sources.resolve(ref)
            if isinstance(raw, str):
                raw = raw.encode()
            if isinstance(raw, bytes):
                files[scalar_filename(value, record.get("legacy_record_entry_index", 0), owner)] = payload(raw)
                break
    return {"files": files, "symlinks": links, "declared_directories": value.get("directories", []),
        "modes": modes, "model_packages": {n: payload(s.encode()) for n, s in value.get("packages", {}).items()},
        "entrypoints": value.get("inputs"), "absolute_inputs": value.get("absolute_inputs", False),
        "source_role_name_for_FS_operations": value.get("name") if manifest["tool"] == "pylint" and
            any(x in manifest["source"]["source_id"] for x in ("entry_io", "entry_path_errno", "dependency_io")) else None,
        "restricted_path": value.get("restricted"), "filename_blocker": None}


def selection_for(tool: str, value: dict, layout: dict, owner: str) -> dict:
    if layout["entrypoints"] is not None:
        return {"kind": "explicit_declared_inputs", "paths": layout["entrypoints"], "absolute": layout["absolute_inputs"]}
    if "Filesystem.load(&selected" in owner:
        return {"kind": "explicit_files_selection", "paths": sorted(value.get("files", {})), "absolute": False}
    if 'Filesystem.load(&[root_path.join("main.py")]' in owner:
        return {"kind": "explicit_main", "paths": ["main.py"], "absolute": False}
    if tool == "mypy" and isinstance(value.get("files"), dict):
        return {"kind": "directory_root", "paths": ["."], "absolute": False}
    return {"kind": "native_directory_python_source_selection" if tool == "pylint" else "single_source",
        "paths": sorted(n for n in layout["files"] if tool != "pylint" or n.endswith(".py")), "absolute": False}


def expected_binding(sources: Sources, section: dict) -> dict:
    ref = next(section[k] for k in ("expected_pointer", "record_pointer", "oracle_pointer") if k in section)
    value = sources.resolve(ref)
    if isinstance(value, dict):
        for key in ("expected", "diagnostics", "reference_json_diagnostics", "original_messages"):
            if key in value:
                return sources.child(ref, key)
    return ref


def public_context(context: dict) -> dict:
    result = copy.deepcopy(context)
    for group in ("files", "model_packages"):
        for row in result["layout"][group].values():
            row.pop("bytes_hex")
    return result


def filesystem_operations(layout: dict, selection: dict) -> list[dict]:
    operations = []
    name, first = layout["source_role_name_for_FS_operations"], next(iter(selection["paths"]), None)
    if layout["restricted_path"]:
        operations.append({"operation": "chmod", "path": layout["restricted_path"], "mode": 0})
    if name in {"unreadable_file", "unreadable_first", "unreadable_last", "quoted_path", "backslash_path", "unicode_path"}:
        operations.append({"operation": "chmod", "path": "unreadable.py" if name == "unreadable_last" else first, "mode": 0})
    elif name == "unsearchable_parent":
        operations.append({"operation": "chmod_parent", "path": first, "mode": 0})
    elif name == "broken_symlink":
        operations.append({"operation": "symlink", "path": first, "target": "absent.py"})
    elif name == "empty_directory":
        operations.append({"operation": "mkdir", "path": first})
    return operations


def duplicate_command_selection(sources: Sources, ref: dict, value: dict, target: str) -> list[dict]:
    """Use every frozen JSON command's absolute ordered input spelling."""
    root = value["original_case_root"]
    if type(root) is not str or not PurePosixPath(root).is_absolute() or root.endswith("/"):
        raise ValueError("duplicate-module frozen command root differs")
    wanted = [root + "/" + name for name in value["inputs"]]
    proofs, selected = [], []
    for index, section in enumerate(value["targets"]):
        if section.get("target") != target:
            continue
        runs = section["reporters"]["json"]["runs"]
        if not runs:
            raise ValueError("duplicate-module target lacks frozen JSON commands")
        for repeat, run in enumerate(runs):
            argv = run["original_command_argv"]
            if type(argv) is not list or any(type(arg) is not str for arg in argv) or argv.count("--") != 1:
                raise ValueError("duplicate-module JSON command wrapper differs")
            split = argv.index("--"); outer, inner = argv[:split], argv[split + 1:]
            def option(args, name):
                if args.count(name) != 1 or args.index(name) + 1 >= len(args):
                    raise ValueError("frozen JSON command option differs: " + name)
                return args[args.index(name) + 1]
            if (option(outer, "--project") != root or option(outer, "--target") != target
                    or option(outer, "--tool") != "mypy" or run["original_command_cwd"] != root
                    or option(inner, "--python-version") != target or inner.count("--output=json") != 1):
                raise ValueError("duplicate-module JSON command target/project/reporter differs")
            tail = inner[inner.index("--output=json") + 1:]
            if tail != wanted:
                raise ValueError("duplicate-module JSON selected input order/spelling differs")
            selected.append(tail)
            proofs.append(sources.child(ref, "targets", index, "reporters", "json", "runs", repeat, "original_command_argv"))
    if not selected or any(args != selected[0] for args in selected):
        raise ValueError("duplicate-module repeated JSON selections disagree")
    return proofs


def build_lock(repo: Path) -> tuple[dict, list[dict]]:
    sources = Sources(repo)
    if digest(sources.raw(REGISTRY)) != REGISTRY_SHA:
        raise ValueError("fixed finite registry identity differs")
    tier = sources.object(REGISTRY)["tiers"]["B"]
    manifests = {m["unit_id"]: m for m in tier["manifests"] if m["tool"] in COUNTS}
    profile_path = "crates/mamba-quality/assets/profile.json"
    requests, corrections, pylint_corrections, library_corrections, namespace_corrections = {}, {}, {}, {}, {}
    excluded = {"builtin_values_regressions.json", "pydantic_config.json", "pydantic_config_entrypoints.json", "cross_file_dependency_regressions.json"}

    def append(manifest: dict, record: dict, value: dict, section: dict):
        owners = manifest["owning_rust_bindings"]
        owner = "\n".join(sources.raw(x["source_id"]).decode() for x in owners)
        layout = layout_for(sources, manifest, record, value, owner)
        context = {"tool": manifest["tool"], "target": section["target"],
            "operation": "mypy_json" if manifest["tool"] == "mypy" else "pylint_json2", "layout": layout,
            "selection": selection_for(manifest["tool"], value, layout, owner),
            "profile_source": {"path": profile_path, "sha256": digest(sources.raw(profile_path))}}
        old_identity, correction = value_sha(context), None
        if (record["record_pointer"]["source_id"] == "crates/mamba-quality/src/infrastructure/mypy/tests/dependencies.json"
                and record["record_pointer"]["json_pointer"] == "/29"):
            if value.get("name") != "typed_pydantic" or value.get("reference_data") != (
                    "locked Pydantic 2.13.5 and its fixed dependency type data are present; builtin Rust model data supplies the same symbols"):
                raise ValueError("fixed typed-Pydantic reference-library declaration differs")
            library_ref = sources.child(record["record_pointer"], "reference_data")
            package_lock_path = "tools/ci/q0_pydantic_reference_lock.json"
            package_lock_ref = {"source_id": package_lock_path, "sha256": digest(sources.raw(package_lock_path)), "json_pointer": ""}
            version = VERSIONS[int(section["target"].split(".")[1])]
            packages = sources.object(package_lock_path)["dependencies"][version]
            origins = []
            for name, selection in REFERENCE_LIBRARY_SELECTIONS.items():
                index, pin = next((index, pin) for index, pin in enumerate(packages) if pin["name"] == name)
                origins.append({"name": name, "version": pin["version"], "wheel_pin": {key: pin[key] for key in ("filename", "sha256")},
                    "wheel_binding": sources.child(package_lock_ref, "dependencies", version, index),
                    "member_selection": selection})
            context["reference_library_assets"] = {"source_binding": library_ref, "origins": origins,
                "static_type_files_only": True, "library_programs_executed": False, "native_binaries_copied": False}
            library_corrections[value_sha(context)] = {"old_request_id": old_identity, "new_request_id": value_sha(context),
                "target": section["target"], "source_binding": library_ref,
                "reason": "source_declares_locked_Pydantic_and_dependency_type_data_present"}
        if manifest["source"]["source_id"] == "crates/mamba-quality/src/infrastructure/mypy/tests/duplicate_modules.json":
            proofs = duplicate_command_selection(sources, record["record_pointer"], value, section["target"])
            old_selection = copy.deepcopy(context["selection"])
            context["selection"]["absolute"] = True
            correction = {"old_request_id": old_identity, "new_request_id": value_sha(context),
                "reason": "frozen_JSON_reporter_commands_use_absolute_selected_inputs",
                "old_selection": old_selection, "new_selection": copy.deepcopy(context["selection"]),
                "source_command_bindings": proofs}
        if manifest["tool"] == "pylint":
            prior_context = copy.deepcopy(context)
            source_name = Path(record["record_pointer"]["source_id"]).name
            ordinal = record.get("legacy_record_entry_index")
            reasons, proofs = [], []
            if ordinal in PYLINT_ROOT_ROWS.get(source_name, set()):
                if value.get("search_roots") != [".", "src"]:
                    raise ValueError("fixed Pylint ordered search-root source differs")
                context["search_roots"] = [".", "src"]
                reasons.append("ordered_project_import_search_roots")
                proofs.append(sources.child(record["record_pointer"], "search_roots"))
            if ordinal in PYLINT_DIRECTORY_ROWS.get(source_name, set()):
                if (value.get("inputs") != ["."] or not any(item["source_id"].endswith("/cross_file_stores.rs") for item in owners)):
                    raise ValueError("fixed Pylint directory selection owner differs")
                selected = sorted(name for name in context["layout"]["files"] if name.endswith(".py"))
                if not selected:
                    raise ValueError("fixed Pylint directory selection has no source targets")
                context["selection"] = {"kind": "source_owned_directory_non_stub_selection", "paths": selected, "absolute": False}
                reasons.append("source_owned_directory_non_stub_selection")
                proofs.append(sources.child(record["record_pointer"], "inputs"))
            intermediate_identity = value_sha(context)
            if (record["record_pointer"]["source_id"] == "crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_selected_imports.json"
                    and record["record_pointer"]["json_pointer"] == "/1"):
                binding = {"source": sources.child(record["record_pointer"], "inputs"),
                    "owner": {"source_id": "crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stores.rs",
                        "sha256": PYLINT_RECIPE_SOURCES["crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stores.rs"]},
                    "upstream": PYLINT_NAMESPACE_SOURCE}
                context["module_source_roots"] = ["."]
                context["module_source_roots_binding"] = binding
                namespace_corrections[value_sha(context)] = {"old_request_id": intermediate_identity, "new_request_id": value_sha(context),
                    "target": section["target"], "source_binding": binding,
                    "reason": "preserve_project_relative_namespace_module_discovery"}
            if reasons:
                pylint_corrections[value_sha(context)] = {"old_request_id": value_sha(prior_context), "new_request_id": value_sha(context),
                    "target": section["target"], "reasons": reasons, "source_bindings": proofs,
                    "old_selection": prior_context["selection"], "new_selection": copy.deepcopy(context["selection"]),
                    "old_search_roots": None, "new_search_roots": context.get("search_roots")}
                if intermediate_identity != value_sha(context):
                    pylint_corrections[value_sha(context)]["via_64db_request_id"] = intermediate_identity
        identity = value_sha(context)
        if correction:
            corrections[identity] = correction
        node = requests.setdefault(identity, {"request_id": identity, "context": context, "source_aliases": [], "expected_bindings": [], "owner_sources": owners})
        node["source_aliases"].append({"unit_id": record["unit_id"], "manifest_id": manifest["unit_id"], "case_source": record["record_pointer"]})
        binding = {"target": section["target"], "source": expected_binding(sources, section)}
        if (record["record_pointer"]["source_id"] == "crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.json"
                and record["record_pointer"]["json_pointer"] == "/16"):
            owner_id = "crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.rs"
            variant_owner = next(item for item in owners if item["source_id"] == owner_id)
            if ('case["authorized_variants"][target].as_array()' not in owner
                    or 'variants.contains(&actual)' not in owner):
                raise ValueError("fixed dependency source-error variant owner differs")
            target_index = TARGETS.index(section["target"])
            variants = value["authorized_variants"][target_index]
            if type(variants) is not list or len(variants) != 3 or any(type(rows) is not list for rows in variants):
                raise ValueError("fixed row16 whole-list variant slots differ")
            binding["authorized_variants_owner"] = variant_owner
            binding["authorized_variants"] = [sources.child(record["record_pointer"], "authorized_variants", target_index, index)
                for index in range(3)]
        node["expected_bindings"].append(binding)

    for record in tier["original_record_entries"]:
        if record["manifest_id"] not in manifests or record["unit_kind"] != "original_record_entry":
            continue
        manifest = manifests[record["manifest_id"]]
        if Path(manifest["source"]["source_id"]).name in excluded:
            continue
        value = sources.resolve(record["record_pointer"])
        if value.get("flag_state") not in {None, "extra_checks_enabled"}:
            raise ValueError("unsealed flag state cannot use the strict default profile")
        if "profile_flags" in value and value["profile_flags"] != sources.object(profile_path)["mypy"]["strict_flags"]:
            raise ValueError("declared checker flags differ from the sealed profile")
        for section in record["binding"]["sections"]:
            append(manifest, record, value, section)
    for whole in tier["whole_scalar_sources"]:
        manifest = next(m for m in manifests.values() if m["source"]["source_id"] == whole["pointer"]["source_id"])
        ref = sources.child(manifest["source"])
        record = {"unit_id": whole["unit_id"], "record_pointer": whole["pointer"], "legacy_record_entry_index": 0}
        for target in TARGETS:
            append(manifest, record, sources.resolve(ref), {"target": target, "expected_pointer": sources.child(ref, "expected_diagnostics")})
    cases = list(requests.values())
    counts = {tool: sum(c["context"]["tool"] == tool for c in cases) for tool in COUNTS}
    if counts != COUNTS or sum(bool(c["context"]["layout"]["model_packages"]) for c in cases) != 138:
        raise ValueError(f"finite lint request denominator differs: {counts}")
    if len(corrections) != 45 or len({row["old_request_id"] for row in corrections.values()}) != 45:
        raise ValueError("source-proven duplicate selection correction denominator differs")
    if (len(pylint_corrections) != 90 or len({row["old_request_id"] for row in pylint_corrections.values()}) != 90
            or sum("ordered_project_import_search_roots" in row["reasons"] for row in pylint_corrections.values()) != 69
            or sum("source_owned_directory_non_stub_selection" in row["reasons"] for row in pylint_corrections.values()) != 27):
        raise ValueError("source-proven Pylint context correction denominator differs")
    if len(library_corrections) != 3 or {row["target"] for row in library_corrections.values()} != set(TARGETS):
        raise ValueError("fixed typed-Pydantic library correction denominator differs")
    if len(namespace_corrections) != 3 or {row["target"] for row in namespace_corrections.values()} != set(TARGETS):
        raise ValueError("fixed Pylint namespace-root correction denominator differs")
    for path, expected in PYLINT_RECIPE_SOURCES.items():
        if digest(sources.raw(path)) != expected:
            raise ValueError("fixed Pylint source recipe differs")
    for path in ["tools/ci/q0_mypy_reference_lock.json", "tools/ci/q0_pylint_reference_lock.json", "tools/ci/q0_pydantic_reference_lock.json",
                 "tools/ci/q0_16_reference.py", "tools/ci/q0_16_packet.py", "tools/ci/q0_mypy_reference.py", "tools/ci/q0_pylint_reference.py",
                 "tools/ci/q0_pydantic_reference.py", "tools/ci/q0_ruff_reference.py", "tools/ci/q0_mypy_inputs.py", "tools/ci/q0_pylint_inputs.py"]:
        sources.raw(path)
    for path in ["tools/ci/q0_16_project_contracts.py", "crates/mamba-quality/src/infrastructure/pylint/tests/crash_filename.rs"]:
        sources.raw(path)
    lock = {"schema": "mamba.q0.16.contract-lint-lock.v1", "baseline_source_head": BASELINE,
        "registry_sha256": REGISTRY_SHA, "python_versions": list(VERSIONS.values()), "request_counts": counts,
        "metadata_package_models": 46, "source_files": dict(sorted(sources.files.items())),
        "metadata_query": {"kind": "real_minimal_pinned_Python_venv", "without_pip": True, "system_site_packages": False,
            "creation_flags": ["--symlinks", "--without-pip"], "executable_identity": "symlink_to_exact_pinned_base",
            "helper": {"member": "mypy/pyinfo.py", "sha256": PYINFO_SHA}, "fixture_modules_executed": False,
            "caller": {"member": "mypy/modulefinder.py", "sha256": MODULEFINDER_SHA, "line_spans": [[846, 881], [945, 949]]},
            "transport": {"kind": "guarded_query_transport_not_Python_identity", "guard_sha256": digest(METADATA_QUERY_GUARD.encode()),
                "real_python_flags": ["-I", "-B"], "only_allowed_arguments": ["exact_pinned_pyinfo_path", "getsearchdirs"],
                "helper_bytes_modified": False, "internal_child_parent_wait_claimed": False}},
        "mypy_JSON_terminal_sources": {"mypy/main.py": MYPY_MAIN_SHA, "mypy/util.py": MYPY_UTIL_SHA},
        "mypy_JSON_blocker_sources": MYPY_BLOCKER_SOURCES,
        "mypy_JSON_nonblocker_sources": MYPY_NONBLOCKER_SOURCES,
        "generated_pyproject": {"source_owner": "tools/ci/q0_16_project_contracts.py", "bytes": len(PYPROJECT), "sha256": digest(PYPROJECT), "explicit_fixture_takes_precedence": True},
        "selection_corrections_from_R4": list(corrections.values()),
        "pylint_context_corrections_from_1881": list(pylint_corrections.values()),
        "mypy_library_asset_corrections_from_64db": list(library_corrections.values()),
        "pylint_namespace_corrections_from_64db": list(namespace_corrections.values()),
        "reference_library_member_selections": REFERENCE_LIBRARY_SELECTIONS,
        "pylint_recipe_sources": PYLINT_RECIPE_SOURCES, "pylint_init_hook_source": PYLINT_INIT_HOOK_SOURCE,
        "pylint_namespace_source": PYLINT_NAMESPACE_SOURCE,
        "requests": [{**c, "context": public_context(c["context"])} for c in cases],
        "source_programs_executed": False, "native_parity": "not_run"}
    return lock, cases


def load_inputs(repo: Path) -> tuple[dict, list[dict]]:
    frozen = read_json((repo / LOCK).read_bytes())
    actual, cases = build_lock(repo)
    if encode(frozen) != encode(actual):
        raise ValueError("frozen lint request/source/owner lock differs")
    return frozen, cases


def alias_value(sources: Sources, alias: dict) -> object:
    ref = alias["case_source"]
    # These two scalar sources are whole scenarios, not character cases.
    return sources.object(ref["source_id"], ref.get("constant")) if alias["unit_id"].endswith(":whole-source") else sources.resolve(ref)


def expected_record(sources: Sources, binding: dict) -> tuple[list[dict], str]:
    value = sources.resolve(binding)
    if isinstance(value, list):
        return value, "declared_array"
    for field in ("original_json_diagnostics", "json_diagnostics", "expected_diagnostics", "expected_native_diagnostics"):
        if isinstance(value.get(field), list):
            return value[field], field
    if isinstance(value.get("original_json"), dict):
        call = value["original_json"]
        for key in ("stdout_hex", "raw_stdout_hex"):
            if key in call:
                return json_lines(bytes.fromhex(call[key])), "original_json." + key
    if isinstance(value.get("repeat_1"), dict) and isinstance(value["repeat_1"].get("diagnostics"), list):
        return value["repeat_1"]["diagnostics"], "repeat_1.diagnostics"
    raise ValueError("source contract has no complete declared diagnostic array")


def explicit_exit(sources: Sources, case: dict, alias: dict) -> tuple[int | None, dict | None]:
    value = alias_value(sources, alias)
    if not isinstance(value, dict):
        return None, None
    target = TARGETS.index(case["context"]["target"])
    ref = alias["case_source"]
    if alias["unit_id"].endswith(":whole-source"):
        ref = sources.child(ref, *())
        ref["json_pointer"] = ""
        ref["value_sha256"] = value_sha(value)
    if (case["context"]["tool"] == "mypy" and alias["unit_id"].endswith(":whole-source")
            and Path(ref["source_id"]).name in {"lambda_binding.json", "with_binding.json"}):
        calls = value.get("original_calls")
        if type(calls) is not list:
            raise ValueError("whole-source contract lacks its declared original JSON calls")
        matched = [(index, call) for index, call in enumerate(calls)
            if type(call) is dict and call.get("target") == case["context"]["target"] and call.get("reporter") == "json"]
        if not matched or any(type(call.get("actual_exit")) is not int for _, call in matched):
            raise ValueError("whole-source target lacks exact integer JSON process exits")
        code = matched[0][1]["actual_exit"]
        if any(call["actual_exit"] != code for _, call in matched):
            raise ValueError("repeated whole-source JSON process exits conflict")
        bindings = [sources.child(ref, "original_calls", index, "actual_exit") for index, _ in matched]
        selected = {**bindings[0], "repeat_bindings": bindings}
        return code, selected
    candidates = []
    if case["context"]["tool"] == "pylint":
        candidates = [("reference_exit",), ("reference_exits", target), ("repeat_1", "exit_code")]
    else:
        candidates = [("original_json_exit",), ("json_original_exit",), ("original_json_exits", target),
            ("original_json_process_exits", target), ("original_json", "actual_exit"),
            ("original_reporters", "json", "original_exit")]
        if value.get("target") == case["context"]["target"]:
            # A target-specific row can retain several JSON reporter repeats.
            # They are repeated calls, not a second target axis.
            repeated = {field: value[field] for field in ("original_json_exits", "original_json_process_exits")
                if type(value.get(field)) is list and value[field] and all(type(code) is int for code in value[field])}
            candidates = [parts for parts in candidates if parts[0] not in repeated]
            for field, codes in repeated.items():
                candidates.extend((field, index) for index in range(len(codes)))
        # Target records explicitly name the JSON reporter and its actual exit.
        if isinstance(value.get("targets"), list):
            for i, item in enumerate(value["targets"]):
                if not isinstance(item, dict):
                    continue
                if item.get("target", item.get("python")) == case["context"]["target"]:
                    for key in ("reference_json_exit", "original_json_exit", "json_exit"):
                        candidates.append(("targets", i, key))
                    for j, _ in enumerate(item.get("reporters", {}).get("json", {}).get("runs", [])):
                        candidates.append(("targets", i, "reporters", "json", "runs", j, "actual_process_exit"))
        if "original_json_diagnostics" in value:
            candidates += [("original_actual_exit",)]
    found = []
    for parts in candidates:
        selected = value
        try:
            for key in parts:
                selected = selected[key]
        except (KeyError, IndexError, TypeError):
            continue
        if type(selected) is int:
            found.append((selected, sources.child(ref, *parts)))
    if found and any(code != found[0][0] for code, _ in found):
        raise ValueError("explicit JSON process exit fields conflict")
    return found[0] if found else (None, None)


def json_lines(raw: bytes) -> list[dict]:
    rows = []
    for line in raw.splitlines():
        if line.strip():
            row = read_json(line)
            if type(row) is not dict or set(row) != RAW_MYPY_FIELDS:
                raise ValueError("mypy JSON line has unknown or missing fields")
            rows.append(row)
    return rows


def project_path(path: str, project: Path, metadata_site: Path | None = None) -> str:
    if not isinstance(path, str):
        raise ValueError("diagnostic path must be text")
    if path.startswith(str(project) + "/"):
        return path[len(str(project)) + 1:]
    if metadata_site is not None and path.startswith(str(metadata_site) + "/"):
        target = metadata_site.parent.name.removeprefix("python")
        return ".venv/lib/python" + target + "/site-packages/" + path[len(str(metadata_site)) + 1:]
    return path


def normal_mypy(row: dict, project: Path, metadata_site: Path | None) -> dict:
    return {"rule": row["code"], "message": row["message"], "hint": row["hint"],
        "path": project_path(row["file"], project, metadata_site), "severity": row["severity"],
        "location": {"line": row["line"], "column": row["column"] + 1,
                     "end_line": row["end_line"], "end_column": row["end_column"]}}


def normal_pylint(row: dict, project: Path) -> dict:
    return {"rule": row["messageId"], "message": row["message"], "hint": None,
        "path": project_path(row["path"], project), "severity": row["type"],
        "location": {"line": row["line"], "column": row["column"], "end_line": row["endLine"], "end_column": row["endColumn"]}}


def crash_filename(rows: list[dict]) -> list[dict]:
    """Port only the pinned crash_filename.rs message contract."""
    rows = copy.deepcopy(rows)
    pattern = re.compile(r"^(.*?): Fatal error while checking '(.*?)'\. Please open an issue in our bug tracker so we address this\. There is a pre-filled template that you can use in '(.*/)?pylint-crash-([0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2})\.txt'\.$")
    for row in rows:
        match = pattern.fullmatch(row["message"]) if row.get("rule") == "F0002" else None
        path = row["path"]
        if match and all(v == path or v.endswith("/" + path) for v in (match[1], match[2])):
            row["message"] = (f"{path}: Fatal error while checking '{path}'. Please open an issue in our bug tracker so we address this. "
                "There is a pre-filled template that you can use in '<cache>/pylint-crash-<timestamp>.txt'.")
    return rows


def bind_duplicate_message(message: str, original: str, project: Path, entries: dict) -> str:
    marker = ' (also at "'
    if message.count(marker) > 1:
        raise ValueError("duplicate-module source marker occurs more than once")
    if marker not in message:
        return message
    prefix, suffix = message.split(marker)
    if not suffix.endswith('\")') or '"' in suffix[:-2]:
        raise ValueError("duplicate-module frozen path token differs")
    token = suffix[:-2]
    root = original.rstrip("/") + "/"
    if not token.startswith(root) or token[len(root):] not in entries:
        raise ValueError("duplicate-module token is outside exact declared source entries")
    return prefix + marker + str(project / token[len(root):]) + '\")'


def compare(sources: Sources, case: dict, stdout: bytes, actual_exit: int, project: Path,
            metadata_site: Path | None = None) -> tuple[list[dict], list[dict]]:
    tool = case["context"]["tool"]
    if tool == "mypy":
        raw = json_lines(stdout)
    else:
        record = read_json(stdout)
        if type(record) is not dict or type(record.get("messages")) is not list:
            raise ValueError("Pylint must return its full JSON2 document")
        raw = record["messages"]
    comparisons = []
    for alias, binding in zip(case["source_aliases"], case["expected_bindings"]):
        expected, expected_kind = expected_record(sources, binding["source"])
        expected = copy.deepcopy(expected)
        value = alias_value(sources, alias)
        filename = Path(alias["case_source"]["source_id"]).name
        original_expected = copy.deepcopy(expected)
        owner = "\n".join(sources.raw(x["source_id"]).decode() for x in case["owner_sources"])
        if tool == "mypy":
            if expected and set(expected[0]) == RAW_MYPY_FIELDS:
                actual = copy.deepcopy(raw)
                for row in actual:
                    row["file"] = project_path(row["file"], project, metadata_site)
                if filename in {"duplicate_modules.json", "shadow_aliases.json"}:
                    for row in expected:
                        if filename == "duplicate_modules.json":
                            row["message"] = bind_duplicate_message(row["message"], value["original_case_root"], project, value["entries"])
                        elif row["file"].startswith(value["original_case_root"].rstrip("/") + "/"):
                            row["file"] = row["file"][len(value["original_case_root"].rstrip("/")) + 1:]
                        if row["file"] not in value["entries"]:
                            raise ValueError("bound diagnostic path lacks a declared source entry")
            else:
                actual = [normal_mypy(row, project, metadata_site) for row in raw]
                if expected and any(set(row) != NORMAL_FIELDS for row in expected):
                    raise ValueError("unknown source-owned mypy diagnostic projection")
        else:
            actual = [normal_pylint(row, project) for row in raw]
            if expected and "messageId" in expected[0]:
                # This exact complete native projection is owned by build_probes.rs.
                expected = [normal_pylint(row, project) for row in expected]
            elif expected and "location" not in expected[0]:
                actual = [{"rule": x["rule"], "message": x["message"], "path": x["path"], "severity": x["severity"], **x["location"]} for x in actual]
            if "crash_filename::normalize" in owner:
                actual, expected = crash_filename(actual), crash_filename(expected)
            if filename == "entry_io_data.json":
                # entry_io.rs binds this exact command's absolute root token.
                for row in actual:
                    row["message"] = row["message"].replace(str(project), "<project>")
            if any(item["source_id"] == "crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stores.rs" for item in case["owner_sources"]):
                # cross_file_stores.rs owns exactly this root-token projection.
                for row in actual:
                    row["message"] = row["message"].replace(str(project), "<project>")
            if filename in {"encoding_controls.json", "encoding_declaration_controls.json"}:
                target = TARGETS.index(case["context"]["target"])
                prefix = str(project) + "/"
                frozen_prefix = value["reference_case_roots"][target].rstrip("/") + "/"
                for row in expected:
                    row["message"] = row["message"].replace(frozen_prefix, prefix)
        code, exit_ref = explicit_exit(sources, case, alias)
        primary_match = capture.strict_json_bytes(actual) == capture.strict_json_bytes(expected)
        variant_matches = []
        variant_bindings = binding.get("authorized_variants", [])
        if variant_bindings:
            if (alias["case_source"]["source_id"] != "crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.json"
                    or alias["case_source"]["json_pointer"] != "/16"
                    or binding["authorized_variants_owner"]["source_id"] != "crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.rs"):
                raise ValueError("whole-list variants are outside the fixed source-owned row16")
            wanted_variants = [sources.child(alias["case_source"], "authorized_variants", TARGETS.index(case["context"]["target"]), index)
                for index in range(3)]
            if (capture.strict_json_bytes(variant_bindings) != capture.strict_json_bytes(wanted_variants)
                    or not any(capture.strict_json_bytes(binding["authorized_variants_owner"]) == capture.strict_json_bytes(owner_ref)
                        for owner_ref in case["owner_sources"])):
                raise ValueError("whole-list variant source bindings differ from fixed row16")
            for variant in variant_bindings:
                rows, _ = expected_record(sources, variant)
                if capture.strict_json_bytes(actual) == capture.strict_json_bytes(rows):
                    variant_matches.append(variant)
        diagnostic_match = primary_match or bool(variant_matches)
        exit_match = actual_exit == code if code is not None else None
        comparison = {"unit_id": alias["unit_id"], "expected_binding": binding["source"],
            "projection": expected_kind, "full_ordered_diagnostics_match": diagnostic_match,
            "primary_ordered_diagnostics_match": primary_match,
            "authorized_variant_bindings": variant_bindings, "matched_authorized_variants": variant_matches,
            "matched_complete_list_binding": binding["source"] if primary_match else variant_matches[0] if variant_matches else None,
            "expected_diagnostics": expected, "actual_diagnostics": actual,
            "expected_side_binding_applied": encode(original_expected) != encode(expected),
            "exit_binding": exit_ref, "expected_JSON_exit": code, "actual_JSON_exit": actual_exit,
            "exit_parity_status": "matched" if exit_match is True else "mismatched" if exit_match is False else "not_defined_for_JSON",
            "native_gate": "not_run"}
        comparisons.append(comparison)
    if not comparisons or any(not c["full_ordered_diagnostics_match"] or c["exit_parity_status"] == "mismatched" for c in comparisons):
        raise ComparisonError(raw, comparisons)
    return raw, comparisons


class ComparisonError(ValueError):
    def __init__(self, raw: list[dict], comparisons: list[dict]):
        super().__init__("complete frozen diagnostic fields or explicit JSON process exit differ")
        self.raw, self.comparisons = raw, comparisons


def declared_mypy_blockers(sources: Sources, case: dict) -> list[dict]:
    """Bind only frozen parser/shadow diagnostics to pinned blocker=True sites."""
    if case["context"]["tool"] != "mypy":
        return []
    bindings = []
    for binding in case["expected_bindings"]:
        expected, _ = expected_record(sources, binding["source"])
        matches = []
        for index, diagnostic in enumerate(expected):
            code = diagnostic.get("rule") if "rule" in diagnostic else diagnostic.get("code")
            # This exact frozen feature-version family uses check_version's
            # blocker=False path. A syntax label alone does not identify it as
            # a parser blocker. Other parser errors in the same alias still bind.
            feature_version = (case["context"]["target"] in {"3.12", "3.13"}
                and diagnostic.get("message") == "T-strings: requires Python 3.14 or newer "
                    f"(current target: Python {case['context']['target']})")
            if code == "syntax" and not feature_version:
                matches.append({"diagnostic_index": index, "kind": "parser_syntax", "upstream_source": "mypy/fastparse.py"})
            elif code is None:
                match = re.fullmatch(r'This file shadows library module "([A-Za-z_][A-Za-z0-9_.]*)"', diagnostic.get("message", ""))
                if match and diagnostic.get("hint") == f'A user-defined top-level module with name "{match[1]}" is not supported':
                    matches.append({"diagnostic_index": index, "kind": "shadowed_library_module", "upstream_source": "mypy/build.py"})
        if not matches:
            return []
        bindings.append({"expected_binding": binding["source"], "declared_blockers": matches})
    return bindings


def process_exit_contract(sources: Sources, case: dict) -> dict:
    if case["context"]["tool"] == "pylint":
        return {"allowed_exits": list(range(33)), "kind": "explicit_Pylint_source_exit", "blocker_bindings": []}
    codes = [explicit_exit(sources, case, alias)[0] for alias in case["source_aliases"]]
    blockers = declared_mypy_blockers(sources, case)
    if 2 in codes:
        return {"allowed_exits": [0, 1, 2], "kind": "explicit_frozen_JSON_blocker_exit", "blocker_bindings": blockers}
    if codes and all(code is None for code in codes) and blockers:
        return {"allowed_exits": [2], "kind": "pinned_source_declared_blocker", "blocker_bindings": blockers,
            "frozen_JSON_exit_parity": "not_defined_for_JSON"}
    return {"allowed_exits": [0, 1], "kind": "normal_JSON_terminal_contract", "blocker_bindings": []}


def host_guard() -> None:
    capture.pylint_reference.require_host_context(capture.pylint_reference.HOST_CONTEXT)
    if platform.machine() != "arm64" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise ValueError("contract execution requires the declared hosted Mac ARM64 job")
    for name in ("MYPYPATH", "PYTHONPATH", "PYTHONHOME", "PYTHONUSERBASE", "PYTHONSTARTUP"):
        if os.environ.get(name):
            raise ValueError("ambient Python search configuration is outside the sealed context: " + name)


def lexical_path(root: Path, text: str, *, allow_dot: bool = False) -> Path:
    """Preserve the caller's spelling, but reject escape from the sealed root."""
    if not isinstance(text, str) or not text or "\x00" in text or PurePosixPath(text).is_absolute():
        raise ValueError("fixture path must be a nonempty relative POSIX path")
    level = 0
    for part in text.split("/"):
        if part == "..":
            level -= 1
        elif part not in {"", "."}:
            level += 1
        if level < 0:
            raise ValueError("fixture path escapes its sealed child root")
    if level == 0 and not allow_dot:
        raise ValueError("fixture root cannot replace a source file")
    return Path(str(root) + "/" + text)


def literal_fixture_names(case: dict) -> set[str]:
    allowed = set()
    wanted = {"3.12": "/15", "3.13": "/20", "3.14": "/25"}
    for alias in case["source_aliases"]:
        ref = alias["case_source"]
        if (ref["source_id"] == "crates/mamba-quality/src/infrastructure/pylint/tests/entry_io_data.json"
                and ref["json_pointer"] == wanted[case["context"]["target"]]):
            if (case["context"]["tool"] != "pylint" or case["context"]["selection"]["paths"] != ["back\\slash.py"]
                    or case["context"]["layout"]["source_role_name_for_FS_operations"] != "backslash_path"):
                raise ValueError("fixed literal POSIX filename context differs")
            allowed.add("back\\slash.py")
    return allowed


def write_payloads(root: Path, files: dict, literal_names: set[str] | None = None) -> None:
    if literal_names and literal_names != {"back\\slash.py"}:
        raise ValueError("only the source-bound literal POSIX fixture basename is admitted")
    root.mkdir(parents=True, exist_ok=False)
    for name, row in files.items():
        if name not in (literal_names or set()):
            safe_path(name)
        raw = bytes.fromhex(row["bytes_hex"])
        if len(raw) != row["bytes"] or digest(raw) != row["sha256"]:
            raise ValueError("declared source bytes or SHA differ")
        path = lexical_path(root, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(raw)


def filesystem_snapshot(root: Path) -> dict:
    rows = {}
    for directory, names, files in os.walk(root, followlinks=False):
        parent = Path(directory)
        for name in sorted([*names, *files]):
            path = parent / name; relative = path.relative_to(root).as_posix(); mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                target = os.readlink(path)
                rows[relative] = {"kind": "symlink", "target": target, "bytes": len(target.encode()), "sha256": digest(target.encode()), "mode": stat.S_IMODE(mode)}
            elif stat.S_ISDIR(mode):
                rows[relative] = {"kind": "directory", "mode": stat.S_IMODE(mode)}
            elif stat.S_ISREG(mode):
                rows[relative] = {"kind": "regular", **capture.fact(path), "mode": stat.S_IMODE(mode)}
            else:
                raise ValueError("unsealed filesystem object")
    return dict(sorted(rows.items()))


def materialize(case: dict, root: Path) -> tuple[Path, dict]:
    layout = case["context"]["layout"]
    files = dict(layout["files"])
    if "pyproject.toml" not in files:
        files["pyproject.toml"] = payload(PYPROJECT)
    work = root / "work"
    literals = literal_fixture_names(case)
    write_payloads(work, files, literals)
    write_payloads(root / "input-snapshot", files, literals)
    for name in layout["declared_directories"]:
        lexical_path(work, name).mkdir(parents=True, exist_ok=True)
    for name, target in layout["symlinks"].items():
        path = lexical_path(work, name)
        # Check the target lexically. Resolving it would destroy loop contracts.
        lexical_path(work, str(PurePosixPath(name).parent / target), allow_dot=True)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to(target)
    for name, mode in layout["modes"].items():
        if mode is not None:
            lexical_path(work, name).chmod(mode & 0o7777)
    snapshot = filesystem_snapshot(work)
    write_json(root / "source-before.json", snapshot)
    return work, snapshot


def checked_command(argv: list[str], cwd: Path, root: Path, inputs: dict, exits=(0,)) -> dict:
    returned = run_command(argv, cwd, root, inputs=inputs, expected_exits=exits, timeout=TIMEOUT)
    write_json(root / "returned-receipt.json", returned)
    stored = read_json((root / "receipt.json").read_bytes())
    receipt = capture.require_saved_receipt(returned, stored, exits)
    terminal_receipt(receipt, exits)
    for filename, field in (("stdout.raw", "stdout_sha256"), ("stderr.raw", "stderr_sha256"),
                            ("stdin.raw", "stdin_sha256"), ("inputs.json", "inputs_sha256")):
        if digest((root / filename).read_bytes()) != receipt[field]:
            raise ValueError("raw process payload differs from its receipt")
    if (receipt["argv"] != argv or receipt["cwd"] != str(cwd)
            or encode(read_json((root / "inputs.json").read_bytes())) != encode(inputs)):
        raise ValueError("full process argv/cwd/input differs")
    return receipt


def terminal_receipt(receipt: dict, exits: tuple[int, ...]) -> None:
    require_receipt(receipt, exits)
    if type(receipt.get("waited_pid")) is not int:
        raise ValueError("waited PID must be an integer, not a JSON boolean")


def final_reconcile(row: dict, root: Path) -> None:
    if not row["process_launched"]:
        return
    stored = read_json((root / "command/receipt.json").read_bytes())
    returned = read_json((root / "command/returned-receipt.json").read_bytes())
    if (capture.strict_json_bytes(stored) != capture.strict_json_bytes(returned)
            or capture.strict_json_bytes(stored) != capture.strict_json_bytes(row["receipt"])):
        raise ValueError("final complete returned/stored/in-memory receipts differ")
    terminal_receipt(stored, tuple(stored["expected_exits"]))
    for name, key in (("stdout.raw", "stdout_sha256"), ("stderr.raw", "stderr_sha256"),
                      ("stdin.raw", "stdin_sha256"), ("inputs.json", "inputs_sha256")):
        if digest((root / "command" / name).read_bytes()) != stored[key]:
            raise ValueError("final raw command payload changed")
    row["final_receipt_reconciled"] = True
    metadata = read_json((root / "command/inputs.json").read_bytes()).get("metadata")
    if metadata:
        finish_metadata(metadata, stored["pid"])
        row["metadata_final_reconciled"] = True


def metadata_executable_identity(python: Path, base_python: Path) -> dict:
    before = python.lstat()
    if not stat.S_ISLNK(before.st_mode):
        raise ValueError("metadata interpreter must be a real venv symlink")
    target = os.readlink(python)
    resolved, base = python.resolve(strict=True), base_python.resolve(strict=True)
    if resolved != base:
        raise ValueError("metadata interpreter symlink does not resolve to the exact pinned base")
    binary = capture.fact(resolved)
    after = python.lstat()
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
            after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns) or os.readlink(python) != target:
        raise ValueError("metadata interpreter symlink changed during capture")
    raw_target = os.fsencode(target)
    return {"kind": "symlink", "path": str(python), "target": target,
        "target_bytes": len(raw_target), "target_sha256": digest(raw_target),
        "resolved_base": str(resolved), "resolved_binary": binary}


def selected_library_members(names: list[str], selection: dict) -> list[str]:
    if len(names) != len(set(names)):
        raise ValueError("reference-library wheel has duplicate members")
    result = []
    for name in names:
        selected = name == selection.get("exact") or bool(selection.get("prefix") and name.startswith(selection["prefix"]))
        if selected and (name.endswith((".py", ".pyi")) or PurePosixPath(name).name == "py.typed") and name not in selection["excluded"]:
            safe_path(name)
            result.append(name)
    result.sort()
    if len(result) != selection["files"]:
        raise ValueError("fixed reference-library type-file denominator differs")
    return result


def copy_reference_libraries(case: dict, executable: Path, root: Path, site: Path) -> dict | None:
    assets = case["context"].get("reference_library_assets")
    if assets is None:
        return None
    if (case["context"]["tool"] != "mypy" or len(case["source_aliases"]) != 1
            or case["source_aliases"][0]["case_source"]["source_id"] != "crates/mamba-quality/src/infrastructure/mypy/tests/dependencies.json"
            or case["source_aliases"][0]["case_source"]["json_pointer"] != "/29"
            or [origin["name"] for origin in assets["origins"]] != list(REFERENCE_LIBRARY_SELECTIONS)):
        raise ValueError("reference-library assets are outside the fixed typed-Pydantic case")
    environment = executable.parent.parent.parent
    identity_path, before_path = environment / "identity.json", environment / "payloads-before.json"
    identity, before = read_json(identity_path.read_bytes()), read_json(before_path.read_bytes())
    checker_site = executable.parent.parent / "lib" / ("python" + case["context"]["target"]) / "site-packages"
    if identity["executable"] != str(executable):
        raise ValueError("reference-library checker identity differs")
    files, manifest = {}, []
    for origin in assets["origins"]:
        if capture.strict_json_bytes(origin["member_selection"]) != capture.strict_json_bytes(REFERENCE_LIBRARY_SELECTIONS[origin["name"]]):
            raise ValueError("fixed reference-library member selection differs")
        pin = origin["wheel_pin"]
        wheel_identity = next(item for item in identity["wheel_artifacts"] if item["filename"] == pin["filename"])
        download_root = environment.parent.parent / "downloads/mypy"
        candidates = [directory / pin["filename"] for directory in download_root.glob("wheel-*") if (directory / pin["filename"]).exists()]
        if len(candidates) != 1:
            raise ValueError("reference-library wheel has no unique retained download")
        wheel = candidates[0]
        if (wheel.name != pin["filename"] or not stat.S_ISREG(wheel.lstat().st_mode) or not wheel.resolve().is_relative_to(download_root.resolve())
                or digest(wheel.read_bytes()) != pin["sha256"] or wheel_identity["sha256"] != pin["sha256"]
                or wheel.stat().st_size != wheel_identity["bytes"]):
            raise ValueError("reference-library wheel differs from the exact locked/attested source")
        distribution = next(item for name, item in identity["distributions"].items()
            if name.replace("-", "_") == origin["name"].replace("-", "_"))
        if distribution["version"] != origin["version"]:
            raise ValueError("reference-library installed version differs")
        with zipfile.ZipFile(wheel) as archive:
            members = selected_library_members(archive.namelist(), origin["member_selection"])
            for name in members:
                info = archive.getinfo(name)
                if info.is_dir() or stat.S_ISLNK(info.external_attr >> 16):
                    raise ValueError("reference-library type member is not a regular file")
                raw = archive.read(name); descriptor = {"bytes": len(raw), "sha256": digest(raw)}
                source = checker_site / name
                if (capture.strict_json_bytes(distribution["distribution_files"][name]) != capture.strict_json_bytes(descriptor)
                        or capture.strict_json_bytes(before[str(source)]) != capture.strict_json_bytes(descriptor)
                        or source.read_bytes() != raw):
                    raise ValueError("reference-library wheel/installed/attested type bytes differ")
                if name in files or (site / name).exists():
                    raise ValueError("reference-library type file collides with declared fixture data")
                files[name] = payload(raw)
                destination = lexical_path(site, name); destination.parent.mkdir(parents=True, exist_ok=True)
                with destination.open("xb") as stream:
                    stream.write(raw)
                manifest.append({"relative_path": name, **descriptor, "library": origin["name"], "version": origin["version"],
                    "wheel_pin": pin, "wheel_binding": origin["wheel_binding"],
                    "retained_wheel": str(wheel.relative_to(environment.parent.parent)),
                    "metadata_site_before_fact": capture.fact(destination),
                    "installed_source": str(source), "installed_file_fact": capture.fact(source),
                    "distribution_file_fact": distribution["distribution_files"][name], "attested_before_fact": before[str(source)]})
    if len(files) != sum(selection["files"] for selection in REFERENCE_LIBRARY_SELECTIONS.values()):
        raise ValueError("typed-Pydantic reference-library file denominator differs")
    write_payloads(root / "reference-library-input-snapshot", files)
    for row in manifest:
        row["input_snapshot_fact"] = capture.fact(root / "reference-library-input-snapshot" / row["relative_path"])
    path = root / "reference-library-assets.json"
    fixture_facts = {name: {key: descriptor[key] for key in ("bytes", "sha256")} for name, descriptor in case["context"]["layout"]["model_packages"].items()}
    union = {name: {"kind": "regular", **descriptor} for name, descriptor in fixture_facts.items()}
    union.update({name: {"kind": "regular", "bytes": descriptor["bytes"], "sha256": descriptor["sha256"]} for name, descriptor in files.items()})
    if capture.strict_json_bytes(capture.artifact_inventory(site, [])) != capture.strict_json_bytes(union):
        raise ValueError("metadata site differs from the exact declared model/reference-asset union")
    write_json(path, {"source_binding": assets["source_binding"], "files": manifest, "file_count": len(files),
        "target": case["context"]["target"], "python_version": VERSIONS[int(case["context"]["target"].split(".")[1])],
        "declared_model_package_files": fixture_facts, "expected_site_union_paths": sorted([*fixture_facts, *files]),
        "checker_identity": {"path": str(identity_path), **capture.fact(identity_path)},
        "checker_payloads_before": {"path": str(before_path), **capture.fact(before_path)},
        "native_binaries_copied": False, "libraries_installed_or_imported_into_metadata": False})
    return {"manifest_path": str(path), "manifest": capture.fact(path), "file_count": len(files)}


def metadata_query_transport(python: Path, helper: Path, version: str, root: Path, site: Path) -> tuple[Path, dict]:
    """Route only mypy's exact query through the genuine isolated interpreter."""
    guard, policy_path, launcher = root / "query-guard.py", root / "query-policy.json", root / "query-transport"
    helper_fact = capture.fact(helper)
    if helper_fact["sha256"] != PYINFO_SHA:
        raise ValueError("metadata transport requires the unmodified pinned helper")
    proof = root / "query-observations"; proof.mkdir()
    policy = {"real_python": str(python), "venv": str(root / "venv"), "python": [int(x) for x in version.split(".")],
        "helper": str(helper), "helper_sha256": PYINFO_SHA, "site": str(site), "proof_root": str(proof),
        "fixture_roots": [str(site), str(root.parent / "work")]}
    write_json(policy_path, policy)
    guard.write_text(METADATA_QUERY_GUARD)
    launcher.write_text("#!/bin/sh\nexec " + shlex.quote(str(python)) + " -I -B " + shlex.quote(str(guard))
        + " " + shlex.quote(str(policy_path)) + ' "$@"\n')
    launcher.chmod(0o700)
    state = {"kind": "guarded_query_transport_not_Python_identity", "path": str(launcher),
        "launcher": capture.fact(launcher), "launcher_mode": stat.S_IMODE(launcher.stat().st_mode),
        "guard_path": str(guard), "guard": capture.fact(guard), "policy_path": str(policy_path), "policy": capture.fact(policy_path),
        "real_python": str(python), "helper": helper_fact, "helper_path": str(helper), "proof_root": str(proof)}
    write_json(root / "query-transport-identity.json", state)
    return launcher, state


def save_or_verify_metadata_json(path: Path, value: object) -> None:
    if path.exists():
        if capture.strict_json_bytes(read_json(path.read_bytes())) != capture.strict_json_bytes(value):
            raise ValueError("immutable metadata observation changed")
    else:
        write_json(path, value)


def verify_metadata_queries(state: dict, checker_pid: int | None) -> list[dict]:
    transport, root = state["query_transport"], Path(state["root"])
    for path_key, fact_key in (("path", "launcher"), ("guard_path", "guard"), ("policy_path", "policy"), ("helper_path", "helper")):
        if capture.fact(Path(transport[path_key])) != transport[fact_key]:
            raise ValueError("metadata query transport/helper bytes changed")
    if stat.S_IMODE(Path(transport["path"]).stat().st_mode) != transport["launcher_mode"]:
        raise ValueError("metadata query transport mode changed")
    proof, result = Path(transport["proof_root"]), []
    completed = sorted(proof.glob("*.completed.json"))
    if not completed:
        raise ValueError("metadata query has no actual completed observation")
    expected_names = set()
    for completed_path in completed:
        record = read_json(completed_path.read_bytes()); identifier = completed_path.name.removesuffix(".completed.json")
        pid = record["pid_self_reported"]
        if (type(pid) is not int or pid <= 0 or str(pid) != identifier
                or type(record["parent_pid_self_reported"]) is not int or record["parent_pid_self_reported"] <= 0
                or record["parent_wait_claimed"] is not False or record["completion_self_reported"] is not True
                or type(record["self_reported_exit"]) is not int or record["self_reported_exit"] != 0
                or record["fixture_execution_guard"] is not True or record["real_python"] != transport["real_python"]
                or record["helper_sha256"] != PYINFO_SHA or record["helper_bytes"] != transport["helper"]["bytes"]
                or capture.strict_json_bytes(record["python"]) != capture.strict_json_bytes(state["python"]) or record["prefix"] != str(root / "venv")
                or record["base_prefix"] != str(Path(state["base_executable"]).parent.parent)
                or record["transport_argv"] != [transport["guard_path"], transport["policy_path"], transport["helper_path"], "getsearchdirs"]
                or type(record["isolated"]) is not int or record["isolated"] != 1
                or record["safe_path"] is not True or type(record["dont_write_bytecode"]) is not int
                or record["dont_write_bytecode"] != 1 or record["user_site"] is not False):
            raise ValueError("metadata query observation identity/health differs")
        stdlib = Path(record["base_prefix"]) / "lib" / ("python" + ".".join(str(x) for x in state["python"][:2]))
        if (type(record.get("module_origins")) is not dict or not {"site", "sysconfig", "types"}.issubset(record["module_origins"])):
            raise ValueError("metadata query lacks its complete real module origins")
        for module in record["module_origins"].values():
            module_path = Path(module["path"])
            if (not module_path.is_absolute() or not (module_path.is_relative_to(stdlib) or str(module_path) == transport["guard_path"])
                    or module_path.is_relative_to(stdlib / "site-packages") or module_path.is_relative_to(Path(state["site"]))
                    or type(module["bytes"]) is not int or module["bytes"] < 0
                    or type(module["sha256"]) is not str or re.fullmatch(r"[0-9a-f]{64}", module["sha256"]) is None):
                raise ValueError("metadata query module origin escaped the exact stdlib/control code")
            if capture.strict_json_bytes(capture.fact(module_path)) != capture.strict_json_bytes({key: module[key] for key in ("bytes", "sha256")}):
                raise ValueError("metadata query module origin bytes changed")
        started_path = proof / (identifier + ".started.json"); started = read_json(started_path.read_bytes())
        started_keys = {"pid_self_reported", "parent_pid_self_reported", "parent_wait_claimed", "completion_self_reported",
            "transport_argv", "real_python", "python", "prefix", "base_prefix", "isolated", "safe_path", "dont_write_bytecode", "user_site"}
        if (set(started) != started_keys or started.get("completion_self_reported") is not False
                or any(capture.strict_json_bytes(record[key]) != capture.strict_json_bytes(value)
                    for key, value in started.items() if key != "completion_self_reported")):
            raise ValueError("metadata query initial observation changed")
        pointers = {}
        for stream in ("stdout", "stderr"):
            raw_path = proof / (identifier + "." + stream + ".raw"); raw = raw_path.read_bytes()
            if (type(record[stream + "_bytes"]) is not int or len(raw) != record[stream + "_bytes"]
                    or digest(raw) != record[stream + "_sha256"] or stream == "stderr" and raw):
                raise ValueError("metadata query full raw stream differs")
            pointers[stream] = {"path": str(raw_path.relative_to(root)), **capture.fact(raw_path)}
            if stream == "stdout":
                search = ast.literal_eval(raw.decode())
                if (type(search) is not tuple or len(search) != 2 or any(type(group) is not list for group in search)
                        or any(type(value) is not str for group in search for value in group) or state["site"] not in search[1]
                        or any(state["checker_venv"] in value for group in search for value in group)):
                    raise ValueError("actual metadata query search result leaked checker code or missed fixture metadata")
        is_preflight = pid == state["query_preflight_pid"]
        if is_preflight:
            if (proof / (identifier + ".stdout.raw")).read_bytes() != (root / "search-query/stdout.raw").read_bytes():
                raise ValueError("outer waited preflight differs from actual query stdout")
        elif type(checker_pid) is not int or record["parent_pid_self_reported"] != checker_pid:
            raise ValueError("internal query lacks the declared outer checker parent")
        for suffix in ("started.json", "completed.json", "stdout.raw", "stderr.raw"):
            expected_names.add(identifier + "." + suffix)
        result.append({"pid_self_reported": pid, "parent_pid_self_reported": record["parent_pid_self_reported"],
            "completion_self_reported": True, "parent_wait_claimed": False, "kind": "waited_preflight" if is_preflight else "internal_checker_query",
            "outer_waited_preflight_pid": state["query_preflight_pid"] if is_preflight else None,
            "observation": {"path": str(completed_path.relative_to(root)), **capture.fact(completed_path)}, "raw": pointers})
    if {path.name for path in proof.iterdir()} != expected_names:
        raise ValueError("metadata query has missing or unrecognized observation members")
    if checker_pid is not None and not any(row["kind"] == "internal_checker_query" for row in result):
        raise ValueError("launched checker has no actual internal metadata query observation")
    save_or_verify_metadata_json(root / ("queries-preflight.json" if checker_pid is None else "queries-final.json"), result)
    return result


def prepare_metadata(base_python: Path, executable: Path, version: str, case: dict, root: Path) -> tuple[Path, dict]:
    """Create a genuine empty venv; fixture packages are data, never imports."""
    root.mkdir(parents=True, exist_ok=False)
    packages = case["context"]["layout"]["model_packages"]
    if not packages:
        raise ValueError("metadata venv needs a nonzero source-owned package model")
    for name in packages:
        safe_path(name)
        if name.endswith(".pth") or PurePosixPath(name).parts[0].removesuffix(".py") in {"site", "sitecustomize", "usercustomize"}:
            raise ValueError("metadata fixture cannot install startup code")
    venv = root / "venv"
    create = checked_command([str(base_python), "-I", "-B", "-m", "venv", "--symlinks", "--without-pip", str(venv)],
        root, root / "create-venv", {"base_executable": capture.fact(base_python.resolve(strict=True)), "python_version": version})
    python = venv / "bin/python"
    executable_identity = metadata_executable_identity(python, base_python)
    write_json(root / "executable-before.json", executable_identity)
    cfg = (venv / "pyvenv.cfg").read_bytes()
    (root / "pyvenv.cfg.before").write_bytes(cfg)
    if (b"include-system-site-packages = false" not in cfg
            or ("version = " + version).encode() not in cfg):
        raise ValueError("real metadata venv lacks its exact patch/no-system-packages identity")
    target = case["context"]["target"]
    site = venv / "lib" / ("python" + target) / "site-packages"
    if not site.is_dir() or any(site.iterdir()):
        raise ValueError("--without-pip metadata site is not empty")
    # write_payloads requires a new root; the site was created by real venv.
    write_payloads(root / "input-snapshot", packages)
    for name, descriptor in packages.items():
        path = lexical_path(site, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(bytes.fromhex(descriptor["bytes_hex"]))
    library_assets = copy_reference_libraries(case, executable, root, site)
    before = capture.artifact_inventory(site, [])
    write_json(root / "site-tree-before.json", before)
    probe = ("import sys;print(repr({'python':list(sys.version_info[:3]),'executable':sys.executable,"
        "'prefix':sys.prefix,'base_prefix':sys.base_prefix,'isolated':sys.flags.isolated,'path':sys.path,"
        "'modules':{k:getattr(v,'__file__',None) for k,v in sys.modules.copy().items()}}))")
    checked_command([str(python), "-I", "-B", "-c", probe], root, root / "identity-query", {"cfg_sha256": digest(cfg), "package_model": before})
    identity = ast.literal_eval((root / "identity-query/stdout.raw").read_text())
    if identity["python"] != [int(x) for x in version.split(".")] or identity["prefix"] != str(venv) or identity["isolated"] != 1:
        raise ValueError("metadata Python identity differs")
    if any(isinstance(path, str) and path.startswith(str(site) + "/") for path in identity["modules"].values()):
        raise ValueError("metadata startup imported a fixture package")
    pyinfo = executable.parent.parent / "lib" / ("python" + target) / "site-packages/mypy/pyinfo.py"
    # The installed package payload and before/after custody pin this helper.
    helper = capture.fact(pyinfo)
    if helper["sha256"] != PYINFO_SHA:
        raise ValueError("metadata query helper differs from its pinned source")
    transport, transport_state = metadata_query_transport(python, pyinfo, version, root, site)
    query_argv = [str(transport), str(pyinfo), "getsearchdirs"]
    query = checked_command(query_argv, root, root / "search-query", {"helper": helper, "metadata_identity": identity,
        "real_python": executable_identity, "query_transport": transport_state, "site_tree": before})
    search = ast.literal_eval((root / "search-query/stdout.raw").read_text())
    if (type(search) is not tuple or len(search) != 2 or any(type(x) is not list for x in search)
            or str(site) not in search[1] or any(str(executable.parent.parent) in p for group in search for p in group)):
        raise ValueError("metadata query leaked checker packages or missed the declared model site")
    write_json(root / "identity.json", {"actual_identity": identity, "actual_search_dirs": search,
        "helper": helper, "interpreter": executable_identity["resolved_binary"],
        "interpreter_symlink": executable_identity, "real_pyvenv_cfg": payload(cfg),
        "query_transport": transport_state,
        "reference_library_assets": library_assets,
        "source_fixture_pyvenv_cfg_is_not_actual_identity": True, "fixture_imports": [], "creation_wait_completed": create["wait_completed"]})
    state = {"root": str(root), "site": str(site), "site_tree_before": before, "cfg_sha256": digest(cfg),
        "base_executable": str(base_python.resolve(strict=True)), "executable_identity": executable_identity,
        "executable_sha256": executable_identity["resolved_binary"]["sha256"], "python": [int(x) for x in version.split(".")],
        "query_transport": transport_state, "query_preflight_pid": query["pid"], "checker_venv": str(executable.parent.parent)}
    state["reference_library_assets"] = library_assets
    verify_metadata_queries(state, None)
    return transport, state


def finish_metadata(state: dict, checker_pid: int | None = None) -> None:
    root, site = Path(state["root"]), Path(state["site"])
    after = capture.artifact_inventory(site, [])
    save_or_verify_metadata_json(root / "site-tree-after.json", after)
    cfg = (root / "venv/pyvenv.cfg").read_bytes()
    cfg_after = root / "pyvenv.cfg.after"
    if cfg_after.exists():
        if cfg_after.read_bytes() != cfg:
            raise ValueError("immutable metadata config observation changed")
    else:
        with cfg_after.open("xb") as stream:
            stream.write(cfg)
    executable_identity = metadata_executable_identity(root / "venv/bin/python", Path(state["base_executable"]))
    save_or_verify_metadata_json(root / "executable-after.json", executable_identity)
    if (after != state["site_tree_before"] or digest(cfg) != state["cfg_sha256"]
            or capture.strict_json_bytes(executable_identity)
                != capture.strict_json_bytes(state["executable_identity"])):
        raise ValueError("metadata interpreter/config/static packages changed")
    if "query_transport" in state:
        verify_metadata_queries(state, checker_pid)
    if state.get("reference_library_assets"):
        assets = state["reference_library_assets"]
        if capture.fact(Path(assets["manifest_path"])) != assets["manifest"]:
            raise ValueError("immutable reference-library asset manifest changed")
        manifest = read_json(Path(assets["manifest_path"]).read_bytes())
        if len(manifest["files"]) != assets["file_count"]:
            raise ValueError("reference-library manifest file denominator differs")
        if manifest["expected_site_union_paths"] != sorted(state["site_tree_before"]):
            raise ValueError("reference-library/model union differs from its captured before tree")
        for row in manifest["files"]:
            expected = {key: row[key] for key in ("bytes", "sha256")}
            for path in (root / "reference-library-input-snapshot" / row["relative_path"], site / row["relative_path"]):
                if capture.strict_json_bytes(capture.fact(path)) != capture.strict_json_bytes(expected):
                    raise ValueError("reference-library snapshot/site type bytes changed")
        after_rows = []
        for row in manifest["files"]:
            expected = {key: row[key] for key in ("bytes", "sha256")}
            source_after = capture.fact(Path(row["installed_source"]))
            if capture.strict_json_bytes(source_after) != capture.strict_json_bytes(expected):
                raise ValueError("reference-library attested checker type source changed")
            after_rows.append({"relative_path": row["relative_path"], "target": manifest["target"],
                "metadata_site_after_fact": capture.fact(site / row["relative_path"]),
                "input_snapshot_after_fact": capture.fact(root / "reference-library-input-snapshot" / row["relative_path"]),
                "installed_source_after_fact": source_after})
        save_or_verify_metadata_json(root / "reference-library-assets-after.json", {
            "before_manifest": assets["manifest"], "target": manifest["target"], "files": after_rows,
            "model_and_asset_union_unchanged": True, "library_programs_executed": False})


def apply_filesystem(case: dict, project: Path, restores: list) -> dict:
    observations = []
    for operation in filesystem_operations(case["context"]["layout"], case["context"]["selection"]):
        path = lexical_path(project, operation["path"])
        if operation["operation"] == "chmod_parent":
            path = path.parent
        if operation["operation"].startswith("chmod"):
            old = stat.S_IMODE(path.stat().st_mode)
            restores.append((path, old)); path.chmod(operation["mode"])
            if stat.S_IMODE(path.stat().st_mode) != 0:
                raise ValueError("permission guard was not materialized")
            try:
                if path.is_dir():
                    os.stat(path / "__q016_permission_probe__")
                else:
                    with path.open("rb") as stream:
                        stream.read(1)
            except OSError as error:
                if error.errno != errno.EACCES:
                    raise ValueError("permission guard did not produce EACCES") from error
                observations.append({"operation": operation, "errno": error.errno, "errno_name": "EACCES"})
            else:
                raise ValueError("permission guard is ineffective for the current user")
        elif operation["operation"] == "symlink":
            path.parent.mkdir(parents=True, exist_ok=True); path.symlink_to(operation["target"])
        elif operation["operation"] == "mkdir":
            path.mkdir()
    return {"operations": observations, "passed": True}


def argv_for(executable: Path, case: dict, project: Path, config: Path, metadata: Path | None) -> list[str]:
    context = case["context"]
    names = []
    for name in context["selection"]["paths"]:
        lexical_path(project, name, allow_dot=True)
        names.append(str(project) + "/" + name if context["selection"]["absolute"] else name)
    if not names:
        raise ValueError("empty selected input denominator")
    if context["tool"] == "mypy":
        argv = [str(executable), "-I", "-m", "mypy", "--strict", "--config-file", str(config),
            "--python-version", context["target"], "--cache-dir=/dev/null", "--no-incremental", "--output=json"]
        if metadata:
            argv += ["--python-executable", str(metadata)]
        return argv + names
    argv = [str(executable), "-I", "-m", "pylint", "--rcfile=/dev/null", "--persistent=n", "--max-line-length=88",
        "--py-version=" + context["target"], "--output-format=json2"]
    if context.get("module_source_roots") is not None:
        if (context["module_source_roots"] != ["."] or len(case["source_aliases"]) != 1
                or case["source_aliases"][0]["case_source"]["source_id"] != "crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_selected_imports.json"
                or case["source_aliases"][0]["case_source"]["json_pointer"] != "/1"):
            raise ValueError("module-discovery source root is outside the fixed namespace case")
        argv += ["--source-roots=" + str(project.resolve(strict=True))]
    if context.get("search_roots") is not None:
        if context["search_roots"] != [".", "src"]:
            raise ValueError("only the exact source-owned ordered project roots are admitted")
        roots = []
        for name in context["search_roots"]:
            root = lexical_path(project, name, allow_dot=True).resolve(strict=True)
            if not root.is_dir() or not root.is_relative_to(project.resolve(strict=True)):
                raise ValueError("declared Pylint root escaped the materialized project")
            roots.append(str(root))
        # Pinned Pylint executes this fixed hook for import search roots. The
        # only import is the already loaded built-in sys module.
        argv += ["--init-hook=import sys;sys.path[:0]=" + repr(roots)]
    return argv + names


def initial_row(case: dict, version: str) -> dict:
    return {"schema": "mamba.q0.16.contract-lint-observation.v1", "request_id": case["request_id"],
        "tool": case["context"]["tool"], "python_version": version, "target": case["context"]["target"],
        "status": "not_run", "process_launched": False, "capture_process_fact": None, "receipt_reconciled": False,
        "comparison_results": [], "clean_tool_health": None, "errors": [], "source_programs_executed": False,
        "native_gate": "not_run", "parent_accepted": False, "exit_scope": "one whole checker command; no per-file exit"}


def run_case(sources: Sources, case: dict, version: str, executable: Path, base: Path, environment: dict, root: Path) -> dict:
    row = initial_row(case, version); root.mkdir(parents=True, exist_ok=False)
    metadata, restores, project, before, initial_saved = None, [], None, None, False
    try:
        project, before = materialize(case, root)
        control = root / "control"; control.mkdir()
        config = control / "mypy.ini"; config.write_bytes(CONFIG_BYTES)
        metadata_python = None
        if case["context"]["layout"]["model_packages"]:
            metadata_python, metadata = prepare_metadata(base, executable, version, case, root / "metadata")
        # These are actual platform errno checks, without execution of a fixture.
        for alias in case["source_aliases"]:
            value = alias_value(sources, alias)
            if isinstance(value, dict) and value.get("expected_errno_name"):
                path = lexical_path(project, case["context"]["selection"]["paths"][0])
                try:
                    path.stat()
                except OSError as error:
                    if error.errno != getattr(errno, value["expected_errno_name"]):
                        raise ValueError("source-owned errno guard differs") from error
                    write_json(root / "errno-guard.json", {"path": str(path), "errno": error.errno, "errno_name": value["expected_errno_name"], "passed": True})
                else:
                    raise ValueError("source-owned errno path unexpectedly exists")
        guard = apply_filesystem(case, project, restores)
        write_json(root / "filesystem-guard.json", guard)
        argv = argv_for(executable, case, project, config, metadata_python)
        inputs = {"request_id": case["request_id"], "context": public_context(case["context"]), "source_aliases": case["source_aliases"],
            "expected_bindings": case["expected_bindings"], "source_snapshot": before, "environment": environment,
            "checker_config": capture.fact(config), "metadata": metadata}
        row["terminal_contract"] = process_exit_contract(sources, case)
        exits = tuple(row["terminal_contract"]["allowed_exits"])
        with capture.capture_environment(row["tool"], root / "error-cache"):
            returned = run_command(argv, project, root / "command", inputs=inputs, expected_exits=exits, timeout=TIMEOUT)
        row["capture_process_fact"] = {k: returned.get(k) for k in ("pid", "waited_pid", "wait_completed", "actual_exit", "timed_out", "launch_error")}
        row["process_launched"] = type(returned.get("pid")) is int and returned["pid"] > 0
        row["status"] = "observed_unreconciled" if row["process_launched"] else "launch_failed"
        write_json(root / "command/returned-receipt.json", returned)
        write_json(root / "initial-result.json", row); initial_saved = True
        stored = read_json((root / "command/receipt.json").read_bytes())
        row["returned_receipt"] = read_json((root / "command/returned-receipt.json").read_bytes())
        row["receipt"] = stored
        capture.require_saved_receipt(returned, stored, exits); terminal_receipt(stored, exits)
        if stored["argv"] != argv or stored["cwd"] != str(project):
            raise ValueError("stored command identity differs")
        for filename, key in (("stdout.raw", "stdout_sha256"), ("stderr.raw", "stderr_sha256"), ("stdin.raw", "stdin_sha256"), ("inputs.json", "inputs_sha256")):
            if digest((root / "command" / filename).read_bytes()) != stored[key]:
                raise ValueError("stored raw process hash differs")
        if encode(read_json((root / "command/inputs.json").read_bytes())) != encode(inputs):
            raise ValueError("stored complete command inputs differ")
        row["receipt_reconciled"] = True; row["receipt"] = stored
        stdout, stderr = (root / "command/stdout.raw").read_bytes(), (root / "command/stderr.raw").read_bytes()
        actual_raw = read_json(stdout).get("messages", []) if row["tool"] == "pylint" else json_lines(stdout)
        fatal = row["tool"] == "pylint" and any(d.get("messageId") == "F0002" for d in actual_raw)
        row["raw_diagnostics"] = actual_raw
        row["clean_tool_health"] = not fatal and not stderr
        declared_fatal = any(any(d.get("rule", d.get("messageId")) == "F0002" for d in expected_record(sources, b["source"])[0]) for b in case["expected_bindings"])
        row["expected_reference_tool_error"] = fatal and declared_fatal
        if stderr and not fatal:
            raise ValueError("checker produced undeclared stderr")
        if fatal:
            reports = [p for p in (root / "error-cache").rglob("*") if p.is_file()]
            row["crash_reports"] = [{"path": str(p.relative_to(root)), **capture.fact(p)} for p in reports]
            if not declared_fatal or not stderr or not reports:
                raise ValueError("F0002 lacks declared source/full stderr/crash report custody")
        try:
            raw, comparisons = compare(sources, case, stdout, stored["actual_exit"], project, Path(metadata["site"]) if metadata else None)
        except ComparisonError as error:
            row["raw_diagnostics"], row["comparison_results"] = error.raw, error.comparisons
            raise
        row["raw_diagnostics"], row["comparison_results"] = raw, comparisons
        row["status"] = "passed"
    except Exception as error:
        row["status"] = "failed" if row["process_launched"] else "not_run"
        row["errors"].append({"stage": "request", "type": type(error).__name__, "message": str(error)})
    finally:
        for path, mode in reversed(restores):
            try:
                path.chmod(mode)
            except OSError as error:
                row["status"] = "failed"; row["errors"].append({"stage": "permission_restore", "message": str(error)})
    try:
        if metadata:
            finish_metadata(metadata, row["capture_process_fact"]["pid"] if row["process_launched"] else None)
        if project:
            after = filesystem_snapshot(project)
            write_json(root / "source-after.json", after)
            allowed = set(before)
            for operation in filesystem_operations(case["context"]["layout"], case["context"]["selection"]):
                if operation["operation"] in {"symlink", "mkdir"}:
                    name = PurePosixPath(operation["path"])
                    allowed.update(str(p) for p in (name, *name.parents) if str(p) != ".")
            row["source_files_unchanged"] = set(after) == allowed and all(after.get(name) == fact for name, fact in before.items())
            for operation in filesystem_operations(case["context"]["layout"], case["context"]["selection"]):
                if operation["operation"] == "symlink":
                    item = after.get(operation["path"], {})
                    row["source_files_unchanged"] &= item.get("kind") == "symlink" and item.get("target") == operation["target"]
                elif operation["operation"] == "mkdir":
                    row["source_files_unchanged"] &= after.get(operation["path"], {}).get("kind") == "directory"
            if not row["source_files_unchanged"] or capture.fact(root / "control/mypy.ini")["sha256"] != digest(CONFIG_BYTES):
                raise ValueError("source bytes/entries or sealed checker configuration changed")
    except Exception as error:
        row["status"] = "failed"; row["errors"].append({"stage": "preservation", "message": str(error)})
    if not initial_saved:
        write_json(root / "initial-result.json", row)
    row["raw_pointers"] = {name: "command/" + name for name in ("receipt.json", "returned-receipt.json", "stdin.raw", "stdout.raw", "stderr.raw", "inputs.json") if (root / "command" / name).is_file()}
    write_json(root / "observation.json", row)
    return row


def known_wait(fact: dict | None) -> bool:
    fact = fact or {}
    return (type(fact.get("pid")) is int and fact["pid"] > 0
            and type(fact.get("waited_pid")) is int and fact["waited_pid"] == fact["pid"]
            and fact.get("wait_completed") is True and type(fact.get("actual_exit")) is int)


def run(repo: Path, output: Path, python: Path, version: str, tool: str) -> int:
    output.mkdir(parents=True, exist_ok=False)
    rows, planned, environment, common, source_before, lock = [], [], {}, None, None, None
    target = ".".join(version.split(".")[:2]); excluded = []
    try:
        frozen = read_json((repo / LOCK).read_bytes())
        intended = [c for c in frozen["requests"] if c["context"]["tool"] == tool and c["context"]["target"] == target]
        write_json(output / "intended-requests.json", intended)
        (output / "request-lock.json").write_bytes((repo / LOCK).read_bytes())
        if len(intended) != COUNTS[tool] // 3:
            raise ValueError("lane request denominator differs")
        lock, cases = load_inputs(repo)
        planned = [c for c in cases if c["context"]["tool"] == tool and c["context"]["target"] == target]
        source_before = {**lock["source_files"], **capture.source_manifest(repo)}
        write_json(output / "source-before.json", source_before)
        write_json(output / "run-identity.json", {"schema": "mamba.q0.16.contract-lint-run.v1", "run_id": os.environ.get("GITHUB_RUN_ID"),
            "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "source_sha": os.environ.get("GITHUB_SHA"), "baseline_source_head": BASELINE,
            "python_version": version, "tool": tool, "planned_requests": len(planned), "lock_sha256": digest((repo / LOCK).read_bytes()),
            "source_programs_executed": False, "native_gate": "not_run", "issue_accepted": False})
        host_guard()
        if not python.is_absolute() or version not in VERSIONS.values():
            raise ValueError("exact absolute pinned bootstrap Python is required")
        profile = Sources(repo).object("crates/mamba-quality/assets/profile.json")
        locks = {name: read_json((repo / f"tools/ci/q0_{name}_reference_lock.json").read_bytes()) for name in ("mypy", "pylint", "pydantic")}
        capture.pydantic_reference.validate_lock(locks["pydantic"])
        if digest(python.resolve(strict=True).read_bytes()) != locks["pydantic"]["interpreters"][version]["executable_sha256"]:
            raise ValueError("bootstrap Python binary differs from the pinned interpreter")
        executable, environment = capture.prepare_environment(repo, python, version, output, tool, profile, locks)
        excluded.append(output / "environments" / tool / "venv")
        sources = Sources(repo)
        if tool == "mypy":
            main = executable.parent.parent / "lib" / ("python" + target) / "site-packages/mypy/main.py"
            util = main.with_name("util.py")
            if capture.fact(main)["sha256"] != MYPY_MAIN_SHA or capture.fact(util)["sha256"] != MYPY_UTIL_SHA:
                raise ValueError("pinned mypy JSON terminal-contract source differs")
            modulefinder = main.with_name("modulefinder.py")
            if capture.fact(modulefinder)["sha256"] != MODULEFINDER_SHA:
                raise ValueError("pinned mypy internal metadata-query caller differs")
            blocker_sources = {}
            nonblocker_sources = {}
            for member, expected in {**MYPY_BLOCKER_SOURCES, **MYPY_NONBLOCKER_SOURCES}.items():
                path = main.parent / PurePosixPath(member).name
                observed = capture.fact(path)
                if observed["sha256"] != expected["sha256"]:
                    raise ValueError("pinned mypy blocker source differs")
                destination = blocker_sources if member in MYPY_BLOCKER_SOURCES else nonblocker_sources
                destination[member] = {**observed, "line_spans": expected["line_spans"]}
            write_json(output / "mypy-terminal-source.json", {"source": capture.fact(main), "line_spans": [[184, 187]],
                "count_stats_source": capture.fact(util), "count_stats_line_spans": [[502, 509]],
                "metadata_query_caller": capture.fact(modulefinder), "metadata_query_line_spans": [[846, 881], [945, 949]],
                "blocker_sources": blocker_sources,
                "nonblocker_sources": nonblocker_sources,
                "allowed_JSON_terminal_exits": [0, 1], "explicit_source_bound_blocker_exit": 2,
                "unexpected_exit_2": "health failure; investigate raw output", "no_severity_inference": True})
        else:
            init_hook_owner = executable.parent.parent / "lib" / ("python" + target) / "site-packages" / PYLINT_INIT_HOOK_SOURCE["member"]
            if capture.fact(init_hook_owner)["sha256"] != PYLINT_INIT_HOOK_SOURCE["sha256"]:
                raise ValueError("pinned Pylint import-root init-hook owner differs")
            namespace_owner = init_hook_owner.parent.parent / "lint/expand_modules.py"
            if capture.fact(namespace_owner)["sha256"] != PYLINT_NAMESPACE_SOURCE["sha256"]:
                raise ValueError("pinned Pylint namespace-discovery owner differs")
            write_json(output / "pylint-import-root-source.json", {"source": capture.fact(init_hook_owner),
                "line_spans": PYLINT_INIT_HOOK_SOURCE["line_spans"], "hook_imports": ["sys"], "fixture_imports": [],
                "namespace_discovery_source": capture.fact(namespace_owner), "namespace_line_spans": PYLINT_NAMESPACE_SOURCE["line_spans"],
                "ordered_roots": ["project", "project/src"]})
        with (output / "initial-index.jsonl").open("x") as index:
            for case in planned:
                root = output / "requests" / case["request_id"]
                if case["context"]["layout"]["model_packages"]:
                    excluded.append(root / "metadata/venv")
                row = run_case(sources, case, version, executable, python, environment, root)
                rows.append(row)
                index.write((root / "initial-result.json").read_text().replace("\n", "") + "\n"); index.flush()
        envroot = output / "environments" / tool
        identity = read_json((envroot / "identity.json").read_bytes())
        records = capture.capture_installed_records(envroot, identity, tool, "after")
        after = capture.environment_payloads(envroot, identity, tool)
        write_json(envroot / "payloads-after.json", after)
        if (after != read_json((envroot / "payloads-before.json").read_bytes()) or not records["all_sources_match"]
                or digest(python.resolve(strict=True).read_bytes()) != environment["base_executable_sha256"]):
            raise ValueError("checker package/interpreter custody changed")
        source_after = {path: capture.fact(repo / path) for path in source_before}
        write_json(output / "source-after.json", source_after)
        if source_after != source_before:
            raise ValueError("locked repository source changed")
    except Exception as error:
        common = {"type": type(error).__name__, "message": str(error)}
    if not planned:
        planned = locals().get("intended", [])
    indexed = {r["request_id"] for r in rows}
    for case in planned:
        if case["request_id"] not in indexed:
            row = initial_row(case, version); row["errors"].append({"stage": "preparation", "message": str(common)})
            rows.append(row)
    for row in rows:
        row["lane_context_verified"] = common is None
        if common and row["status"] == "passed":
            row["status"] = "failed"; row["errors"].append({"stage": "lane_context", **common})
        root = output / "requests" / row["request_id"]; root.mkdir(parents=True, exist_ok=True)
        try:
            final_reconcile(row, root)
        except Exception as error:
            row["status"] = "failed"; row["final_receipt_reconciled"] = False
            row["errors"].append({"stage": "final_receipt", "type": type(error).__name__, "message": str(error)})
        if not (root / "initial-result.json").exists():
            write_json(root / "initial-result.json", row)
        write_json(root / "final-result.json", row)
    write_json(output / "case-results.json", rows)
    with (output / "raw-index.jsonl").open("x") as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n")
    passed = bool(planned and len(rows) == COUNTS[tool] // 3 and common is None and all(r["status"] == "passed" for r in rows))
    comparisons = [c for r in rows for c in r["comparison_results"]]
    summary = {"schema": "mamba.q0.16.contract-lint-summary.v1", "tool": tool, "python_version": version,
        "intended_requests": len(planned), "results": len(rows), "passed_requests": sum(r["status"] == "passed" for r in rows),
        "not_run_requests": [r["request_id"] for r in rows if r["status"] == "not_run"],
        "failed_requests": [r["request_id"] for r in rows if r["status"] not in {"passed", "not_run"}],
        "actual_checker_processes": sum(r["process_launched"] for r in rows),
        "waited_checker_processes": sum(known_wait(r.get("capture_process_fact")) for r in rows),
        "JSON_exit_parity": {s: sum(c["exit_parity_status"] == s for c in comparisons) for s in ("matched", "mismatched", "not_defined_for_JSON")},
        "unhealthy_F0002_observations": sum(r.get("expected_reference_tool_error") is True for r in rows),
        "common_error": common, "capture_passed": passed, "native_gate": "not_run", "issue_accepted": False,
        "excluded_venv_trees": [str(p.relative_to(output)) for p in excluded]}
    write_json(output / "summary.json", summary)
    write_json(output / "artifact-manifest.json", capture.artifact_inventory(output, excluded))
    print(json.dumps({"capture_passed": passed, "intended_requests": len(planned), "tool": tool, "issue_accepted": False}))
    return 0 if passed else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--python-version", choices=VERSIONS.values())
    parser.add_argument("--python", type=Path)
    parser.add_argument("--tool", choices=COUNTS)
    parser.add_argument("--write-lock", action="store_true")
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()
    if args.write_lock:
        lock, _ = build_lock(args.repo)
        (args.repo / LOCK).write_bytes(json.dumps(lock, ensure_ascii=True, separators=(",", ":")).encode() + b"\n")
        return 0
    if args.plan_only:
        lock, _ = load_inputs(args.repo)
        print(json.dumps({"request_counts": lock["request_counts"], "native_parity": "not_run"}))
        return 0
    if any(x is None for x in (args.artifact_dir, args.python_version, args.python, args.tool)):
        parser.error("hosted capture needs --artifact-dir, --python-version, --python and --tool")
    return run(args.repo.resolve(), args.artifact_dir.resolve(), args.python, args.python_version, args.tool)


if __name__ == "__main__":
    raise SystemExit(main())
