"""One independent, complete-list original-tool reference observation gate.

This module never changes a source comparison or accepts a native gate.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import stat

PACKET = "tools/ci/q0_16_cases/reference/mypy_shadow_order.json"
CASE_SOURCE = "crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.json"
REQUEST = "2a11abe6244b5c3bfc236c5869513a7f5f03f420fe6eb6da75fdd74ffcc6f58d"
CATEGORY = "mypy ordering when both collections and collections.abc are shadowed."
BINDING = "independent_shadow_reference_gate"


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def encoded(value: object, *, ordered: bool = False) -> bytes:
    return json.dumps(value, ensure_ascii=True, sort_keys=not ordered, separators=(",", ":"), allow_nan=False).encode()


def same(left: object, right: object) -> bool:
    return encoded(left) == encoded(right)


def value_sha(value: object) -> str:
    return digest(encoded(value, ordered=True))


def read_json(raw: bytes) -> object:
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate shadow-reference JSON key")
            result[key] = value
        return result
    def invalid(value):
        raise ValueError("nonfinite shadow-reference JSON")
    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def selected(value: object, pointer: str) -> object:
    require(pointer == "" or pointer.startswith("/"), "shadow source pointer differs")
    for token in pointer.split("/")[1:] if pointer else []:
        require("~" not in token.replace("~0", "").replace("~1", ""), "shadow source pointer escape differs")
        token = token.replace("~1", "/").replace("~0", "~")
        if type(value) is list:
            require(token == "0" or token.isdecimal() and not token.startswith("0"), "shadow source array index differs")
            value = value[int(token)]
        else:
            value = value[token]
    return value


def load_packet(repo: Path, binding: dict) -> tuple[dict, bytes]:
    require(binding["source_id"] == PACKET, "independent shadow packet path differs")
    raw = (repo / PACKET).read_bytes()
    require(digest(raw) == binding["sha256"], "independent shadow packet bytes changed")
    packet = read_json(raw)
    require(packet["schema"] == "mamba.q0.16.mypy-shadow-order-reference.v1", "shadow packet schema differs")
    policy = packet["policy"]
    require(policy["already_approved_category"] == CATEGORY and policy["parent_issue"] == 15
        and policy["scope"] == "original_tool_reference_eligibility_only"
        and policy["strict_source_comparison_preserved"] is True and policy["no_seed_or_retry_selection"] is True
        and policy["native_gate"] == "not_run" and packet["native_acceptance"] is False
        and packet["parent_acceptance"] is False, "shadow packet policy differs")
    for ref in packet["source_bindings"]:
        path = Path(ref["source_id"])
        require(not path.is_absolute() and ".." not in path.parts, "shadow source path differs")
        source = (repo / path).read_bytes()
        require(digest(source) == ref["sha256"], "shadow bound repository source changed")
        require(value_sha(selected(read_json(source), ref["json_pointer"])) == ref["value_sha256"], "shadow selected source value changed")
    require({observation["id"] for observation in packet["observations"]} == {"collections", "collections_abc"}
        and len(packet["observations"]) == 2, "shadow complete observation denominator differs")
    for observation in packet["observations"]:
        stdout = bytes.fromhex(observation["stdout_hex"])
        rows = [read_json(line) for line in stdout.splitlines()]
        require(digest(stdout) == observation["stdout_sha256"] and same(rows, observation["diagnostics"])
            and len(rows) == 1 and observation["stdin_hex"] == observation["stderr_hex"] == ""
            and type(observation["actual_exit"]) is int and observation["actual_exit"] == 2, "shadow observation evidence changed")
        require(set(rows[0]) == {"file", "line", "column", "end_line", "end_column", "message", "hint", "code", "severity"}
            and rows[0]["code"] is None, "shadow complete diagnostic shape differs")
        origin = observation["origin"]
        require(origin["request_id"] == REQUEST and type(origin["pid"]) is int and origin["pid"] > 0
            and type(origin["waited_pid"]) is int and origin["pid"] == origin["waited_pid"]
            and origin["wait_completed"] is True and type(origin["actual_exit"]) is int and origin["actual_exit"] == 2
            and origin["stdout"]["sha256"] == digest(stdout)
            and origin["stdout"]["bytes"] == len(stdout)
            and origin["stdin"]["sha256"] == origin["stderr"]["sha256"] == digest(b""), "shadow original evidence envelope differs")
    return packet, raw


def evaluate(packet: dict, case: dict, root: Path, row: dict, receipt: dict, inputs: dict,
             stdout: bytes, stderr: bytes, stdin: bytes) -> dict:
    """Verify complete source/input/process tuples. This does no file writes."""
    scope, binding, context = packet["scope"], packet["binding"], case["context"]
    require(case["request_id"] == scope["request_id"] == REQUEST and context["tool"] == "mypy"
        and context["target"] == scope["target"] == "3.13" and row["python_version"] == scope["python_version"] == "3.13.5",
        "independent shadow case or target differs")
    require(len(case["source_aliases"]) == len(case["expected_bindings"]) == 1
        and same(case["source_aliases"][0]["case_source"], scope["case_source"])
        and scope["case_source"]["source_id"] == CASE_SOURCE and scope["case_source"]["json_pointer"] == "/16",
        "independent shadow original source unit differs")
    require(value_sha(context) == binding["context_sha256"] == REQUEST
        and value_sha(case["source_aliases"]) == binding["source_aliases_sha256"]
        and value_sha(case["expected_bindings"]) == binding["expected_bindings_sha256"]
        and same(context["selection"], binding["selection"]) and same(context["profile_source"], binding["profile_source"]),
        "independent shadow source/profile/input/selection binding differs")
    public = json.loads(json.dumps(context))
    for group in ("files", "model_packages"):
        for descriptor in public["layout"][group].values():
            descriptor.pop("bytes_hex")
    require(value_sha(public) == binding["public_context_sha256"] and same(inputs["context"], public)
        and inputs["request_id"] == REQUEST and same(inputs["source_aliases"], case["source_aliases"])
        and same(inputs["expected_bindings"], case["expected_bindings"])
        and same(inputs["source_snapshot"], binding["source_snapshot"])
        and same(inputs["checker_config"], binding["checker_config"]) and inputs["metadata"] is None,
        "independent shadow actual input proof differs")
    require(set(inputs) == {"request_id", "context", "source_aliases", "expected_bindings", "source_snapshot", "environment", "checker_config", "metadata"},
        "independent shadow complete input fields differ")
    environment = inputs["environment"]
    require(same({key: value for key, value in environment.items() if key not in {"executable", "base_executable"}}, binding["environment"]),
        "independent shadow pinned interpreter/package identity differs")
    executable = root.parent.parent / "environments/mypy/venv/bin/python"
    arguments = [str(root / "control/mypy.ini") if arg == "<CASE_ROOT>/control/mypy.ini" else arg for arg in packet["command"]["arguments"]]
    require(receipt["argv"] == [str(executable), *arguments] and receipt["cwd"] == str(root / "work")
        and environment["executable"] == str(executable) and same(receipt["expected_exits"], packet["command"]["expected_exits"]),
        "independent shadow full command differs")
    require(type(receipt["pid"]) is int and receipt["pid"] > 0 and type(receipt["waited_pid"]) is int
        and receipt["waited_pid"] == receipt["pid"] and receipt["wait_completed"] is True
        and receipt["timed_out"] is False and receipt["launch_error"] is None and receipt["passed"] is True
        and type(receipt["actual_exit"]) is int and receipt["actual_exit"] == 2 and stderr == stdin == b"",
        "independent shadow actual wait/exit/streams differ")
    matches = [observation for observation in packet["observations"] if stdout == bytes.fromhex(observation["stdout_hex"])]
    require(len(matches) == 1, "independent shadow complete diagnostic observation differs")
    observation = matches[0]
    require(same([read_json(line) for line in stdout.splitlines()], observation["diagnostics"]), "independent shadow full parsed diagnostic differs")
    comparisons = row["comparison_results"]
    require(len(comparisons) == 1 and comparisons[0]["exit_parity_status"] == "not_defined_for_JSON",
        "independent shadow strict exit comparison changed")
    primary = observation["id"] == "collections"
    require(comparisons[0]["full_ordered_diagnostics_match"] is primary,
        "independent shadow strict comparison was replaced")
    if primary:
        require(row["errors"] == [], "primary shadow observation has unresolved errors")
    else:
        require(len(row["errors"]) == 1 and row["errors"][0]["stage"] == "request"
            and row["errors"][0]["type"] == "ComparisonError", "shadow variant has errors beyond its retained strict mismatch")
    require(row["source_files_unchanged"] is True and row["clean_tool_health"] is True
        and row["receipt_reconciled"] is True and row["process_launched"] is True
        and row["native_gate"] == "not_run" and row["parent_accepted"] is False,
        "independent shadow preservation/health boundary differs")
    return {"schema": "mamba.q0.16.independent-shadow-reference-proof.v1", "request_id": REQUEST,
        "reference_eligible": True, "observation_id": observation["id"],
        "classification": "primary_source_match" if primary else "approved_reference_variant",
        "strict_source_match": primary, "strict_comparisons_sha256": digest(encoded(comparisons)),
        "retained_errors_sha256": digest(encoded(row["errors"])), "source_comparison_rewritten": False,
        "actual_pid": receipt["pid"], "waited_pid": receipt["waited_pid"], "wait_completed": True, "actual_exit": 2,
        "complete_receipt_sha256": digest(encoded(receipt)), "complete_inputs_sha256": digest(encoded(inputs)),
        "stdout_sha256": digest(stdout), "stderr_sha256": digest(stderr), "stdin_sha256": digest(stdin),
        "packet_sha256": case[BINDING]["sha256"], "native_gate": "not_run", "parent_acceptance": False}


def check_identity(root: Path, inputs: dict, packet: dict) -> None:
    environment = inputs["environment"]
    require(environment["identity_path"] == packet["binding"]["environment"]["identity_path"] == "environments/mypy/identity.json",
        "independent shadow identity member differs")
    identity_raw = (root.parent.parent / environment["identity_path"]).read_bytes()
    require(digest(identity_raw) == packet["binding"]["environment"]["identity_sha256"], "independent shadow real interpreter identity changed")
    identity = read_json(identity_raw)
    require(environment["executable"] == identity["executable"] and environment["base_executable"] == identity["base_identity"]["executable"],
        "independent shadow real interpreter paths differ")


def assess_saved(repo: Path, case: dict, root: Path, row: dict) -> tuple[dict, bytes]:
    packet, packet_raw = load_packet(repo, case[BINDING])
    command = root / "command"
    receipt_raw = (command / "receipt.json").read_bytes()
    returned_raw = (command / "returned-receipt.json").read_bytes()
    receipt, returned = read_json(receipt_raw), read_json(returned_raw)
    require(same(receipt, returned) and same(receipt, row["receipt"]) and same(returned, row["returned_receipt"]),
        "independent shadow complete receipt copies differ")
    raw = {name: (command / (name + (".json" if name == "inputs" else ".raw"))).read_bytes() for name in ("stdout", "stderr", "stdin", "inputs")}
    for name, value in raw.items():
        require(digest(value) == receipt[name + "_sha256"], "independent shadow saved raw evidence changed")
    inputs = read_json(raw["inputs"])
    check_identity(root, inputs, packet)
    config = (root / "control/mypy.ini").read_bytes()
    require(same({"bytes": len(config), "sha256": digest(config)}, packet["binding"]["checker_config"]), "independent shadow checker configuration changed")
    snapshot = packet["binding"]["source_snapshot"]
    for name in ("source-before.json", "source-after.json"):
        require(same(read_json((root / name).read_bytes()), snapshot), "independent shadow complete source tree proof changed")
    actual_names = set()
    for path in (root / "work").rglob("*"):
        name = path.relative_to(root / "work").as_posix(); actual_names.add(name)
        fact = snapshot.get(name); require(fact is not None, "independent shadow source path added")
        mode = path.lstat().st_mode
        require(stat.S_IMODE(mode) == fact["mode"], "independent shadow source mode changed")
        if fact["kind"] == "directory":
            require(stat.S_ISDIR(mode), "independent shadow source directory changed")
        else:
            require(stat.S_ISREG(mode), "independent shadow source is not regular")
            content = path.read_bytes()
            require(len(content) == fact["bytes"] and digest(content) == fact["sha256"], "independent shadow source bytes changed")
    require(actual_names == set(snapshot), "independent shadow source path missing")
    proof = evaluate(packet, case, root, row, receipt, inputs, raw["stdout"], raw["stderr"], raw["stdin"])
    proof.update(saved_receipt_file_sha256=digest(receipt_raw), returned_receipt_file_sha256=digest(returned_raw),
        inputs_file_sha256=digest(raw["inputs"]), source_before_file_sha256=digest((root / "source-before.json").read_bytes()),
        source_after_file_sha256=digest((root / "source-after.json").read_bytes()))
    return proof, packet_raw


def capture(repo: Path, case: dict, root: Path, row: dict) -> dict:
    proof, packet_raw = assess_saved(repo, case, root, row)
    with (root / "reference-variant-packet.json").open("xb") as stream:
        stream.write(packet_raw)
    with (root / "reference-variant-gate.json").open("xb") as stream:
        stream.write(json.dumps(proof, ensure_ascii=True, indent=2).encode() + b"\n")
    return proof


def reconcile(repo: Path, case: dict, root: Path, row: dict) -> None:
    proof, packet_raw = assess_saved(repo, case, root, row)
    require((root / "reference-variant-packet.json").read_bytes() == packet_raw, "independent shadow captured packet changed")
    require((root / "reference-variant-gate.json").read_bytes() == json.dumps(proof, ensure_ascii=True, indent=2).encode() + b"\n"
        and same(row["independent_shadow_reference"], proof), "independent shadow original gate evidence changed")


def reference_pass(row: dict) -> bool:
    if row["status"] == "passed":
        return True
    proof = row.get("independent_shadow_reference", {})
    return (row["status"] == "passed_reference_variant" and proof.get("reference_eligible") is True
        and proof.get("classification") == "approved_reference_variant" and proof.get("strict_source_match") is False
        and row.get("independent_shadow_final_reconciled") is True)
