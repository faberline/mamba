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
import stat
import sys

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
MYPY_BLOCKER_SOURCES = {
    "mypy/fastparse.py": {"sha256": "9bea678209bab98c6f6b7aeed0174905be3c9a456b1ca0730d613c29a8e0f5d7", "line_spans": [[260, 270]]},
    "mypy/build.py": {"sha256": "61e1295a840159ea32f78dcc2fe253c49be7d1915f47551ad74be91be76df641", "line_spans": [[932, 937]]},
}
MYPY_NONBLOCKER_SOURCES = {
    "mypy/nativeparse.py": {"sha256": "6621e8f6e863dbc6e6bee8139528a0d76ec6df8c91a6abbf6f9dd2444360bc3a", "line_spans": [[192, 208]]},
}


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
    requests, corrections = {}, {}
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
        if manifest["source"]["source_id"] == "crates/mamba-quality/src/infrastructure/mypy/tests/duplicate_modules.json":
            proofs = duplicate_command_selection(sources, record["record_pointer"], value, section["target"])
            old_selection = copy.deepcopy(context["selection"])
            context["selection"]["absolute"] = True
            correction = {"old_request_id": old_identity, "new_request_id": value_sha(context),
                "reason": "frozen_JSON_reporter_commands_use_absolute_selected_inputs",
                "old_selection": old_selection, "new_selection": copy.deepcopy(context["selection"]),
                "source_command_bindings": proofs}
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
            "helper": {"member": "mypy/pyinfo.py", "sha256": PYINFO_SHA}, "fixture_modules_executed": False},
        "mypy_JSON_terminal_sources": {"mypy/main.py": MYPY_MAIN_SHA, "mypy/util.py": MYPY_UTIL_SHA},
        "mypy_JSON_blocker_sources": MYPY_BLOCKER_SOURCES,
        "mypy_JSON_nonblocker_sources": MYPY_NONBLOCKER_SOURCES,
        "generated_pyproject": {"source_owner": "tools/ci/q0_16_project_contracts.py", "bytes": len(PYPROJECT), "sha256": digest(PYPROJECT), "explicit_fixture_takes_precedence": True},
        "selection_corrections_from_R4": list(corrections.values()),
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


def write_payloads(root: Path, files: dict) -> None:
    root.mkdir(parents=True, exist_ok=False)
    for name, row in files.items():
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
    write_payloads(work, files)
    write_payloads(root / "input-snapshot", files)
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
    query_argv = [str(python), str(pyinfo), "getsearchdirs"]
    checked_command(query_argv, root, root / "search-query", {"helper": helper, "metadata_identity": identity, "site_tree": before})
    search = ast.literal_eval((root / "search-query/stdout.raw").read_text())
    if (type(search) is not tuple or len(search) != 2 or any(type(x) is not list for x in search)
            or str(site) not in search[1] or any(str(executable.parent.parent) in p for group in search for p in group)):
        raise ValueError("metadata query leaked checker packages or missed the declared model site")
    write_json(root / "identity.json", {"actual_identity": identity, "actual_search_dirs": search,
        "helper": helper, "interpreter": executable_identity["resolved_binary"],
        "interpreter_symlink": executable_identity, "real_pyvenv_cfg": payload(cfg),
        "source_fixture_pyvenv_cfg_is_not_actual_identity": True, "fixture_imports": [], "creation_wait_completed": create["wait_completed"]})
    return python, {"root": str(root), "site": str(site), "site_tree_before": before, "cfg_sha256": digest(cfg),
        "base_executable": str(base_python.resolve(strict=True)), "executable_identity": executable_identity,
        "executable_sha256": executable_identity["resolved_binary"]["sha256"]}


def finish_metadata(state: dict) -> None:
    root, site = Path(state["root"]), Path(state["site"])
    after = capture.artifact_inventory(site, [])
    write_json(root / "site-tree-after.json", after)
    cfg = (root / "venv/pyvenv.cfg").read_bytes()
    (root / "pyvenv.cfg.after").write_bytes(cfg)
    executable_identity = metadata_executable_identity(root / "venv/bin/python", Path(state["base_executable"]))
    write_json(root / "executable-after.json", executable_identity)
    if (after != state["site_tree_before"] or digest(cfg) != state["cfg_sha256"]
            or capture.strict_json_bytes(executable_identity)
                != capture.strict_json_bytes(state["executable_identity"])):
        raise ValueError("metadata interpreter/config/static packages changed")


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
    return [str(executable), "-I", "-m", "pylint", "--rcfile=/dev/null", "--persistent=n", "--max-line-length=88",
        "--py-version=" + context["target"], "--output-format=json2", *names]


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
            finish_metadata(metadata)
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
                "blocker_sources": blocker_sources,
                "nonblocker_sources": nonblocker_sources,
                "allowed_JSON_terminal_exits": [0, 1], "explicit_source_bound_blocker_exit": 2,
                "unexpected_exit_2": "health failure; investigate raw output", "no_severity_inference": True})
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
