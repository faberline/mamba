"""Fixed Pydantic symbol inputs and frozen asset projections, without imports."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath

FAMILIES = ("field-types", "model-types", "model-methods", "model-config-field")
VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
TARGETS = ("3.12", "3.13", "3.14")
SOURCE_INVENTORY_SHA256 = "d3d98eb06d8cdae1a658b0392755a2d517ea0e39f70906509d638ab81be4a278"
PROFILE_SHA256 = "59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7"
ASSET_ROOT = "crates/mamba-quality/assets/pydantic"
ASSET_PINS = {
    "field-types": {"path": ASSET_ROOT + "/field-types.json", "bytes": 344006,
                    "sha256": "3feeb55e6b8abf976d00d42bd7c5357a06264d171b49ec4f30528f4fe1c0c5ca"},
    "model-types": {"path": ASSET_ROOT + "/model-types.json", "bytes": 79871,
                    "sha256": "745e7173fbed74acf3924ff92d36c67a6ba271dae63951e6b666facd78cc71a9"},
    "model-methods": {"path": ASSET_ROOT + "/model-methods.json", "bytes": 1013691,
                      "sha256": "4d8b9edc282ed6e064aaf613b74adce81bed8f18b18cd9556bbe08f2c8db7fa9"},
    "model-config-field": {"path": ASSET_ROOT + "/model-config-field.json", "bytes": 713,
                           "sha256": "f4f4a01379dba139751a74465618a158ab80952e7a680e5bc8c0ffedf0853a96"},
}
CONFIG = ("[mypy]\nplugins = pydantic.mypy\n[pydantic-mypy]\ninit_typed = true\n"
          "init_forbid_extra = true\nwarn_required_dynamic_aliases = true\n")
STUBS = {
    "field-types": ("field_types.pyi", "from pydantic import Field as Field\n"),
    "model-types": ("field_types.pyi", "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict, Field as Field\n"),
    "model-methods": ("field_types.pyi", "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict, Field as Field\n"),
    "model-config-field": ("config_field.pyi", "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict\n"),
}


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _family(family: str) -> None:
    if family not in FAMILIES:
        raise ValueError("unknown fixed Pydantic asset family")


def _source_rows(source_files: dict) -> list[dict]:
    if type(source_files) is not dict or len(source_files) != 41:
        raise ValueError("Pydantic source inventory denominator differs")
    for name, sha256 in source_files.items():
        if type(name) is not str:
            raise ValueError("unsafe Pydantic source inventory row")
        path = PurePosixPath(name)
        if (not name or path.is_absolute() or "\\" in name
                or any(part in ("", ".", "..") for part in name.split("/"))
                or path.as_posix() != name or type(sha256) is not str):
            raise ValueError("unsafe Pydantic source inventory row")
    # The sealed inventory uses Path component order, not raw string order.
    rows = [{"path": name, "sha256": source_files[name]}
            for name in sorted(source_files, key=PurePosixPath)]
    if digest(json.dumps(rows, sort_keys=True).encode()) != SOURCE_INVENTORY_SHA256:
        raise ValueError("Pydantic fixed source inventory identity differs")
    return rows


def read_assets(repository: Path) -> dict[str, dict]:
    lock = json.loads((repository / "tools/ci/q0_pydantic_reference_lock.json").read_bytes())
    required = {"schema": "mamba.q0.164.pydantic-reference-lock.v1", "version": "2.13.5", "mypy": "2.4.0", "python_versions": list(VERSIONS.values()),
                "families": list(FAMILIES), "asset_target_count": 12, "reference_timeout_seconds": 120,
                "source_inventory_sha256": SOURCE_INVENTORY_SHA256, "profile_sha256": PROFILE_SHA256,
                "config_sha256": digest(CONFIG.encode()),
                "historical_oracle_lock_sha256": "3ede6b08cf4fb8a1a12ec148169667e6b22d484b380f901a1bb040c8a2669a35",
                "pydantic_fields_sha256": "6bc66125f23c143e934030fbf9c58c9b5657950bdad3031d4f6132e55bd57be3"}
    if any(lock.get(name) != value for name, value in required.items()):
        raise ValueError("Pydantic reference source/input lock differs")
    for row in _source_rows(lock["source_files"]):
        if digest((repository / row["path"]).read_bytes()) != row["sha256"]:
            raise ValueError("Pydantic owning source bytes differ: " + row["path"])
    if digest((repository / "crates/mamba-quality/assets/profile.json").read_bytes()) != PROFILE_SHA256:
        raise ValueError("Pydantic profile bytes differ")
    if set(lock["asset_pins"]) != set(FAMILIES):
        raise ValueError("Pydantic asset lock denominator differs")
    expected_stubs = {family: {"filename": spec["filename"], "bytes": spec["source_bytes"],
                               "sha256": spec["source_sha256"]}
                      for family in FAMILIES for spec in [input_spec(family)]}
    if lock.get("stub_pins") != expected_stubs:
        raise ValueError("Pydantic original stub input lock differs")
    assets = {}
    for family in FAMILIES:
        pin = ASSET_PINS[family]
        if any(lock["asset_pins"][family].get(key) != value for key, value in pin.items()):
            raise ValueError("Pydantic frozen asset lock differs")
        if lock["asset_pins"][family].get("schema") != "mamba.quality.pydantic-" + family + ".v1":
            raise ValueError("Pydantic frozen asset schema lock differs")
        raw = (repository / pin["path"]).read_bytes()
        if len(raw) != pin["bytes"] or digest(raw) != pin["sha256"]:
            raise ValueError("Pydantic frozen asset bytes differ")
        asset = json.loads(raw)
        if (list(asset) != ["schema", "mypy", "pydantic", "targets"]
                or asset["schema"] != "mamba.quality.pydantic-" + family + ".v1"
                or asset["mypy"] != "2.4.0" or asset["pydantic"] != "2.13.5"
                or list(asset["targets"]) != list(TARGETS)
                or (json.dumps(asset, indent=2) + "\n").encode() != raw):
            raise ValueError("Pydantic frozen asset envelope or serialization differs")
        assets[family] = asset
    return assets


def input_spec(family: str) -> dict:
    """Original import/config data only; no frozen output is consulted."""
    _family(family)
    filename, source = STUBS[family]
    return {"filename": filename, "source": source, "config_filename": "mypy.ini", "config": CONFIG,
            "source_bytes": len(source.encode()), "source_sha256": digest(source.encode()),
            "config_bytes": len(CONFIG.encode()), "config_sha256": digest(CONFIG.encode())}


def materialize_work(family: str, work: Path, profile: Path) -> dict:
    specification = input_spec(family)
    if not work.is_absolute() or not profile.is_absolute():
        raise ValueError("Pydantic work and profile paths must be absolute")
    if work.exists() or work.is_symlink():
        raise FileExistsError("Pydantic extraction work must be fresh")
    profile_raw = profile.read_bytes()
    if digest(profile_raw) != PROFILE_SHA256:
        raise ValueError("Pydantic materialization profile differs")
    files = {
        specification["filename"]: {"bytes": specification["source_bytes"], "sha256": specification["source_sha256"]},
        specification["config_filename"]: {"bytes": specification["config_bytes"], "sha256": specification["config_sha256"]},
    }
    create = family != "model-config-field"
    if create:
        work.mkdir(parents=True, exist_ok=False)
        for name, text in [(specification["filename"], specification["source"]),
                           (specification["config_filename"], specification["config"])]:
            with (work / name).open("xb") as stream:
                stream.write(text.encode())
    return {"family": family, "work": str(work), "profile": str(profile),
            "profile_bytes": len(profile_raw), "profile_sha256": digest(profile_raw),
            "files": files, "work_created_by_helper": create,
            "original_child_creates_work": not create, "expected_values_used_for_inputs": False}


def project_record(family: str, raw: dict, field: dict | None = None) -> dict:
    _family(family)
    if family == "model-config-field":
        if type(field) is not dict:
            raise ValueError("Pydantic config field requires its complete field.json sidecar")
        # The original outer process reads the whole sidecar. Do not hide extra keys.
        return dict(field)
    keys = ("serialized_type", "aliases", "variants", "fields_sha256") if family == "field-types" else ("model_types", "aliases")
    return {key: raw[key] for key in keys}


def assemble_asset(family: str, sections: dict[str, dict]) -> dict:
    _family(family)
    if type(sections) is not dict or set(sections) != set(TARGETS):
        raise ValueError("Pydantic assembly requires all three original targets")
    return {"schema": "mamba.quality.pydantic-" + family + ".v1", "mypy": "2.4.0", "pydantic": "2.13.5",
            "targets": {target: sections[target] for target in TARGETS}}


def json_type_errors(actual: object, expected: object, pointer: str = "") -> list[str]:
    """Separate schema/type integrity; this is not the original Python == predicate."""
    if type(actual) is not type(expected):
        return ["JSON type differs at " + (pointer or "/")]
    if type(actual) not in (dict, list, str, int, float, bool, type(None)):
        return ["non-JSON value at " + (pointer or "/")]
    errors = []
    if type(expected) is dict:
        if any(type(key) is not str for key in actual):
            return ["non-string JSON key at " + (pointer or "/")]
        if set(actual) != set(expected):
            errors.append("JSON object keys differ at " + (pointer or "/"))
        for key in expected:
            if key in actual:
                escaped = key.replace("~", "~0").replace("/", "~1")
                errors.extend(json_type_errors(actual[key], expected[key], pointer + "/" + escaped))
    elif type(expected) is list:
        if len(actual) != len(expected):
            errors.append("JSON array length differs at " + (pointer or "/"))
        for index, (a_value, e_value) in enumerate(zip(actual, expected)):
            errors.extend(json_type_errors(a_value, e_value, pointer + "/" + str(index)))
    return errors
