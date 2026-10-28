#!/usr/bin/env python3
"""Retain twelve fixed Pydantic asset collectors without issue acceptance."""
from __future__ import annotations

import argparse
import base64
import csv
import datetime as dt
import inspect
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from q0_mypy_reference import command, require_python_identity, require_receipt, validate_dependencies
from q0_pylint_reference import HOST_CONTEXT, require_host_context
from q0_ruff_reference import checked_bytes, digest, download, inventory, write_json

VERSIONS = {12:"3.12.11",13:"3.13.5",14:"3.14.0"}
FAMILIES = ("field-types","model-types","model-methods","model-config-field")
PINS = {"mypy":"2.4.0","mypy_extensions":"1.1.0","pathspec":"1.1.1","librt":"0.16.0",
        "ast_serialize":"0.12.1","typing_extensions":"4.16.0","pydantic":"2.13.5",
        "pydantic_core":"2.46.5","annotated-types":"0.8.0","typing-inspection":"0.4.4"}
MODULE_NAMES = {"mypy":"mypy","mypy_extensions":"mypy_extensions","pathspec":"pathspec","librt":"librt",
                "ast_serialize":"ast_serialize","typing_extensions":"typing_extensions","pydantic":"pydantic",
                "pydantic_core":"pydantic_core","annotated-types":"annotated_types","typing-inspection":"typing_inspection"}

INSTALLER = """import runpy,sys
wheel=sys.argv[1]
sys.path.insert(0,wheel)
sys.argv=['pip',*sys.argv[2:]]
runpy.run_module('pip',run_name='__main__')
"""

# Only the separate preparation probe loads the archive-bound pip wheel. The
# twelve original collector commands do not add a parser or installer package.
PACKAGE_PROBE = "import sys\nsys.path.insert(0,sys.argv[2])\n" + inspect.getsource(validate_dependencies) + r'''
import base64, hashlib, importlib, importlib.machinery, importlib.metadata, json, pathlib
from pip._vendor.packaging.specifiers import SpecifierSet
from pip._vendor.packaging.utils import canonicalize_name
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
    requires_python = distribution.metadata.get('Requires-Python')
    if requires_python is None or not SpecifierSet(requires_python).contains('.'.join(map(str,sys.version_info[:3])),prereleases=True):
        raise RuntimeError('installed Requires-Python differs: '+name)
    distributions[name] = {'name':distribution.metadata['Name'],'version':distribution.version,
        'metadata_path':str(metadata_path),'requires_dist':distribution.requires or [],'requires_python':requires_python,
        'distribution_files':files,'missing_distribution_files':missing,'installed_records':records,'native_files':native}
verification = validate_dependencies(pins,distributions)
modules = {}
module_names = json.loads(sys.argv[3])
for pin_name, module_name in module_names.items():
    module = importlib.import_module(module_name)
    path = pathlib.Path(module.__file__).resolve()
    raw = path.read_bytes()
    modules[pin_name] = {'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
import mypy.main,mypy.version,pydantic
site_packages = pathlib.Path(importlib.metadata.distribution('pydantic')._path).parent
installed = {canonicalize_name(d.metadata['Name']):d.version for d in importlib.metadata.distributions(path=[str(site_packages)])}
print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated,
    'prefix':sys.prefix,'base_prefix':sys.base_prefix,'mypy_version':mypy.version.__version__,'pydantic_version':pydantic.__version__,
    'strict_flags':mypy.main.define_options()[1],'distributions':distributions,'modules':modules,
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

def require_pins(pins: list[dict]) -> None:
    if len(pins)!=10 or {pin["name"]:pin["version"] for pin in pins}!=PINS:
        raise ValueError("Pydantic replay requires all ten exact package pins")


def bundled_pip(python: Path, version: str) -> Path:
    base = python.resolve(strict=True).parent.parent
    wheels = list((base / "lib" / ("python" + ".".join(version.split(".")[:2])) / "ensurepip/_bundled").glob("pip-*.whl"))
    if len(wheels) != 1:
        raise ValueError("exact PBS bundled installer wheel is missing or ambiguous")
    return wheels[0]

def require_venv_link(executable: Path, base: Path) -> dict:
    if not executable.is_symlink() or executable.resolve(strict=True) != base.resolve(strict=True):
        raise ValueError("Pydantic venv must link to the exact pinned base")
    return {"venv_creation_mode":"symlinks-without-pip", "venv_executable_is_symlink":True,
            "venv_executable_resolved":str(executable.resolve(strict=True)),
            "base_executable_resolved":str(base.resolve(strict=True))}

def require_package_identity(identity: dict, version: str, pins: list[dict], venv: Path, strict_flags: list[str]) -> None:
    require_pins(pins)
    require_python_identity(identity, version)
    if identity["mypy_version"]!="2.4.0" or identity["pydantic_version"]!="2.13.5" or identity["strict_flags"]!=strict_flags:
        raise ValueError("Pydantic/mypy version or strict flags differ")
    if identity["prefix"]!=str(venv):
        raise ValueError("Pydantic preparation interpreter prefix differs")
    expected = {name.replace("_", "-"):value for name,value in PINS.items()}
    if identity["installed_site_distributions"] != expected:
        raise ValueError("Pydantic target site-packages has missing, extra or wrong distributions")
    markers = identity["dependency_verification"].get("marker_environment", {})
    if (markers.get("python_full_version") != version or markers.get("python_version") != ".".join(version.split(".")[:2])
            or markers.get("sys_platform") != "darwin" or markers.get("platform_system") != "Darwin"
            or markers.get("platform_machine") != "arm64"):
        raise ValueError("Pydantic dependency marker environment differs")
    if validate_dependencies(pins, identity["distributions"], markers) != identity["dependency_verification"]:
        raise ValueError("Pydantic dependency verification differs")
    for name,row in identity["distributions"].items():
        if not Path(row["metadata_path"]).is_relative_to(venv) or len(row["installed_records"]) != 1:
            raise ValueError(f"Pydantic distribution metadata/RECORD is incomplete: {name}")
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
        raise ValueError("Pydantic module identity denominator differs")
    payloads = installed_payloads(identity,venv)
    for name,module in identity["modules"].items():
        path = Path(module["path"])
        if not path.is_relative_to(venv) or payloads.get(str(path)) != {"bytes":module["bytes"],"sha256":module["sha256"]}:
            raise ValueError(f"Pydantic module import escaped or changed: {name}")

def prepare(python: Path, version: str, root: Path, pins: list[dict], wheels: list[Path], interpreter_pin: dict, strict_flags: list[str]) -> Path:
    root.mkdir(parents=True, exist_ok=False)
    require_pins(pins)
    checked_bytes(python.resolve(strict=True).read_bytes(), interpreter_pin["executable_sha256"])
    probe = "import json,sys; print(json.dumps({'python':list(sys.version_info[:3]),'executable':sys.executable,'isolated':sys.flags.isolated}))"
    command([str(python),"-I","-B","-c",probe], root, root / "base-identity")
    base = json.loads((root / "base-identity/stdout.raw").read_bytes())
    require_python_identity(base, version)
    if base["executable"]!=str(python):
        raise ValueError("Pydantic base executable path differs")
    command([str(python),"-I","-B","-m","venv","--symlinks","--without-pip",str(root / "venv")], root, root / "create-venv")
    executable = root / "venv/bin/python"
    link = require_venv_link(executable, python)
    installer = bundled_pip(python, version)
    installer_raw = installer.read_bytes()
    write_json(root / "installer-identity.json", {"path":str(installer), "bytes":len(installer_raw),
               "sha256":digest(installer_raw), "source":"exact archive-bound interpreter ensurepip bundle"})
    artifacts = []
    if len(wheels) != len(pins):
        raise ValueError("Pydantic wheel denominator differs")
    for pin,path in zip(pins,wheels):
        raw = checked_bytes(path.read_bytes(), pin["sha256"])
        artifacts.append({"name":pin["name"], "version":pin["version"], "filename":path.name, "bytes":len(raw), "sha256":digest(raw)})
    prefix = [str(executable),"-I","-B","-c",INSTALLER,str(installer)]
    command([*prefix,"install","--no-index","--no-deps","--no-compile",*map(str,wheels)], root,
            root / "install", inputs={"installer_sha256":digest(installer_raw), "wheels":artifacts})
    command([str(executable),"-I","-B","-c",PACKAGE_PROBE,json.dumps(pins),str(installer),json.dumps(MODULE_NAMES)], root, root / "package-identity")
    identity = json.loads((root / "package-identity/stdout.raw").read_bytes())
    require_package_identity(identity, version, pins, root / "venv", strict_flags)
    command([*prefix,"list","--format=json"], root, root / "installed-distributions")
    command([*prefix,"freeze","--all"], root, root / "pip-freeze")
    identity.update(base_identity=base, base_executable_sha256=digest(python.resolve().read_bytes()),
                    venv_executable_sha256=digest(executable.read_bytes()), wheel_artifacts=artifacts,
                    installer_sha256=digest(installer_raw), historical_environment=False, **link)
    write_json(root / "identity.json", identity)
    return executable


def installed_payloads(identity: dict, venv: Path) -> dict[str, dict]:
    files = {}
    for row in identity["distributions"].values():
        site = Path(row["metadata_path"]).parent
        for relative, fact in row["distribution_files"].items():
            path = (site / relative).resolve(strict=True)
            if not path.is_relative_to(venv) or str(path) in files:
                raise ValueError("installed payload escaped the venv or has duplicate ownership")
            if file_fact(path) != fact:
                raise ValueError("installed payload bytes changed: " + relative)
            files[str(path)] = fact
        for relative, fact in row["native_files"].items():
            if row["distribution_files"].get(relative) != fact:
                raise ValueError("native payload is not a recorded distribution file")
    return files


def file_fact(path: Path) -> dict:
    raw = path.read_bytes()
    return {"bytes":len(raw),"sha256":digest(raw)}


def inherited_stdin_identity() -> dict:
    result = {"inheritance":"unmodified; no Popen stdin argument","contents":"not_captured"}
    try:
        status = os.fstat(0)
        result["descriptor"] = {"device":status.st_dev,"inode":status.st_ino,"mode":status.st_mode,
                                "is_tty":os.isatty(0)}
    except OSError as error:
        result["descriptor_error"] = {"error_class":type(error).__name__,"message":str(error)}
    return result


def run_inherited(argv: list[str], output: Path, inputs: dict, *, timeout: float = 120) -> dict:
    """Preserve actual subprocess output; inherit cwd, env and stdin unchanged."""
    output.mkdir(parents=True,exist_ok=False)
    write_json(output / "inputs.json",inputs)
    write_json(output / "environment-before.json",environment_snapshot())
    stdin = inherited_stdin_identity()
    write_json(output / "stdin-identity.json",stdin)
    receipt = {"argv":argv,"cwd":os.getcwd(),"stdin":stdin,"environment_inherited":True,
               "expected_exits":[0],"timeout_seconds":timeout,"pid":None,"waited_pid":None,
               "wait_completed":False,"actual_exit":None,"timed_out":False,"launch_error":None,
               "started_utc":dt.datetime.now(dt.timezone.utc).isoformat()}
    started,stdout,stderr = time.monotonic(),b"",b""
    try:
        process = subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        receipt["pid"] = process.pid
        try:
            stdout,stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            receipt["timed_out"] = True
            try:
                os.killpg(process.pid,signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout,stderr = process.communicate()
        receipt.update(waited_pid=process.pid,wait_completed=True,actual_exit=process.returncode)
    except OSError as error:
        receipt["launch_error"] = {"error_class":type(error).__name__,"message":str(error)}
    (output / "stdout.raw").write_bytes(stdout)
    (output / "stderr.raw").write_bytes(stderr)
    write_json(output / "environment-after.json",environment_snapshot())
    receipt.update(duration_seconds=time.monotonic()-started,stdout_sha256=digest(stdout),stderr_sha256=digest(stderr),
                   inputs_sha256=digest((output / "inputs.json").read_bytes()),
                   environment_before_sha256=digest((output / "environment-before.json").read_bytes()),
                   environment_after_sha256=digest((output / "environment-after.json").read_bytes()),
                   stdin_identity_sha256=digest((output / "stdin-identity.json").read_bytes()))
    receipt["passed"] = bool(receipt["wait_completed"] and not receipt["timed_out"]
                             and receipt["launch_error"] is None and receipt["actual_exit"] == 0 and stderr == b"")
    write_json(output / "receipt.json",receipt)
    return receipt


def require_main_receipt(receipt: dict, command_root: Path, argv: list[str]) -> None:
    require_receipt(receipt)
    if (receipt["argv"]!=argv or receipt["cwd"]!=os.getcwd() or receipt["timeout_seconds"]!=120
            or receipt["environment_inherited"] is not True
            or receipt["stdin"]["inheritance"]!="unmodified; no Popen stdin argument"
            or receipt["stdin"]["contents"]!="not_captured"
            or (command_root / "stderr.raw").read_bytes()!=b""):
        raise ValueError("Pydantic child argv, inheritance, timeout or stderr differs")
    for name,key in [("stdout.raw","stdout_sha256"),("stderr.raw","stderr_sha256"),("inputs.json","inputs_sha256"),
                     ("environment-before.json","environment_before_sha256"),("environment-after.json","environment_after_sha256"),
                     ("stdin-identity.json","stdin_identity_sha256")]:
        checked_bytes((command_root / name).read_bytes(),receipt[key])
    if ((command_root / "environment-before.json").read_bytes()
            !=(command_root / "environment-after.json").read_bytes()):
        raise ValueError("controller environment changed during Pydantic capture")
    if json.loads((command_root / "stdin-identity.json").read_bytes())!=receipt["stdin"]:
        raise ValueError("inherited stdin identity differs")
    if (command_root / "stdin.raw").exists():
        raise ValueError("inherited stdin must not be reported as captured input bytes")


def declared_inputs(family: str) -> dict:
    from q0_pydantic_inputs import input_spec
    spec = input_spec(family)
    return {spec["filename"]:{"bytes":spec["source_bytes"],"sha256":spec["source_sha256"]},
            spec["config_filename"]:{"bytes":spec["config_bytes"],"sha256":spec["config_sha256"]}}


def require_work_files(family: str, work: Path, before: dict, after: dict) -> None:
    fixed = declared_inputs(family)
    expected = set(fixed) | ({"field.json","identity.json"} if family=="model-config-field" else set())
    if set(after)!=expected or any(after.get(name)!=fact for name,fact in fixed.items()):
        raise ValueError("Pydantic fixed input bytes or work file denominator differs")
    if before != ({} if family=="model-config-field" else fixed):
        raise ValueError("Pydantic original work creation boundary differs")
    if any(path.is_symlink() for path in work.rglob("*")):
        raise ValueError("Pydantic extraction work contains a symlink")


def validate_capture(family: str, minor: int, executable: Path, work: Path, capture: Path,
                     profile: Path, identity: dict, stdout: bytes) -> dict:
    observed = json.loads((capture / "collector-identity.json").read_bytes())
    venv = executable.parent.parent
    if (observed["family"]!=family or observed["python"]!=list(map(int,VERSIONS[minor].split(".")))
            or observed["executable"]!=str(executable) or observed["prefix"]!=str(venv)
            or observed["base_prefix"]!=identity["base_prefix"] or observed["cwd"]!=os.getcwd()
            or observed["flags"]["isolated"]!=1 or observed["flags"]["ignore_environment"]!=1
            or observed["flags"]["no_user_site"]!=1 or observed["flags"]["safe_path"] is not True
            or (family=="model-config-field" and observed["flags"]["dont_write_bytecode"]!=1)
            or observed["input_files"]!=declared_inputs(family)
            or observed["profile"]!={"path":str(profile),**file_fact(profile)}
            or observed["original_stdout_sha256"]!=digest(stdout)):
        raise ValueError("Pydantic collector interpreter, input or stdout identity differs")
    expected_sidecars = {name:file_fact(work / name) for name in ("field.json","identity.json")} if family=="model-config-field" else {}
    if observed["original_sidecars"]!=expected_sidecars:
        raise ValueError("Pydantic original sidecar denominator or bytes differ")
    custody = json.loads((capture / "import-custody.json").read_bytes())
    stdlib = (Path(identity["base_prefix"]) / "lib" / f"python3.{minor}").resolve(strict=True)
    if custody["unexpected"] or custody["allowed_roots"]!={"venv":str(venv),"base_stdlib":str(stdlib)}:
        raise ValueError("Pydantic collector imported code outside the fixed roots")
    files = installed_payloads(identity,venv)
    source_files = {str(work / name):fact for name,fact in declared_inputs(family).items()}
    count = 0
    for name,row in custody["modules"].items():
        if row.get("path") is None:
            if row.get("category")=="object-alias":
                validate_object_alias(name,row,custody["modules"],venv,stdlib,files)
                continue
            if row.get("category")=="none-module":
                if row!={"path":None,"category":"none-module"}:
                    raise ValueError("Pydantic negative import cache row contains code identity")
                continue
            if row.get("category") not in {"built-in","frozen","entrypoint","namespace"}:
                raise ValueError("Pydantic module has no explicit source identity: "+name)
            if row.get("category")=="namespace":
                paths = row.get("namespace_paths")
                if not paths or any(not Path(path).is_dir() or Path(path).resolve(strict=True)!=Path(path)
                                    or not (Path(path).is_relative_to(venv) or in_base_stdlib(Path(path),stdlib)) for path in paths):
                    raise ValueError("Pydantic namespace import escaped its fixed roots")
            continue
        validate_source_file(row,venv,stdlib,files,{})
        count += 1
    if not count or count!=custody["file_backed_module_count"]:
        raise ValueError("Pydantic imported module count is empty or incomplete")
    graph_capture = json.loads((capture / "build-graph.json").read_bytes())
    if graph_capture["unexpected"]:
        raise ValueError("Pydantic mypy graph escaped its fixed roots")
    graph = graph_capture["modules"]
    # Every source-backed graph node is checked without modifying its ID or path.
    if not isinstance(graph,dict) or not graph:
        raise ValueError("Pydantic mypy build graph is empty")
    for name,row in graph.items():
        if row.get("path") is None:
            if row.get("category")!="no-source-path":
                raise ValueError("Pydantic graph node has no explicit source identity: "+name)
            continue
        validate_source_file(row,venv,stdlib,files,source_files)
    return {"identity":observed,"import_custody":custody,"graph_node_count":len(graph),"imported_file_count":count}


def validate_object_alias(name: str, row: dict, modules: dict, venv: Path, stdlib: Path, files: dict) -> None:
    required = {"path","category","owner_module","owner_qualname","owner_members","identity_verified",
                "owner_path","owner_bytes","owner_sha256"}
    if (set(row)!=required or row["path"] is not None or row["category"]!="object-alias"
            or row["identity_verified"] is not True or type(row["owner_module"]) is not str
            or type(row["owner_qualname"]) is not str or type(row["owner_members"]) is not list
            or not row["owner_members"] or any(type(part) is not str or not part.isidentifier() for part in row["owner_members"])
            or ".".join(row["owner_members"])!=row["owner_qualname"]
            or name!=row["owner_module"]+"."+row["owner_qualname"]):
        raise ValueError("Pydantic fileless object alias identity or member trace differs")
    owner = modules.get(row["owner_module"])
    if (type(owner) is not dict or owner.get("path") is None or owner.get("category") not in {"venv","base_stdlib"}
            or {"path":row["owner_path"],"bytes":row["owner_bytes"],"sha256":row["owner_sha256"]}
                != {key:owner.get(key) for key in ("path","bytes","sha256")}):
        raise ValueError("Pydantic fileless object alias owner file is absent or differs")
    validate_source_file(owner,venv,stdlib,files,{})
    expected_category = "venv" if Path(owner["path"]).is_relative_to(venv) else "base_stdlib"
    if owner["category"]!=expected_category:
        raise ValueError("Pydantic fileless object alias owner category differs")


def validate_source_file(row: dict, venv: Path, stdlib: Path, files: dict, source_files: dict) -> None:
    path = Path(row["path"])
    if not path.is_absolute() or path.resolve(strict=True)!=path:
        raise ValueError("collector source path is not absolute and canonical")
    captured = {"bytes":row["bytes"],"sha256":row["sha256"]}
    if file_fact(path)!=captured:
        raise ValueError("collector source raw bytes changed")
    if path.is_relative_to(venv):
        if files.get(str(path))!=captured:
            raise ValueError("collector package source is outside installed RECORD payloads")
    elif str(path) in source_files:
        if source_files[str(path)]!=captured:
            raise ValueError("collector declared source bytes differ")
    elif not in_base_stdlib(path,stdlib):
        raise ValueError("collector source escaped pinned package, standard library or declared input roots")


def in_base_stdlib(path: Path, stdlib: Path) -> bool:
    return path.is_relative_to(stdlib) and not {"site-packages","dist-packages"}.intersection(path.relative_to(stdlib).parts)


def decode_record(family: str, stdout: bytes) -> dict:
    raw = json.loads(stdout)
    if type(raw) is not dict or stdout!=(json.dumps(raw)+"\n").encode():
        raise ValueError("Pydantic original stdout is not one exact JSON object plus LF")
    if family=="model-config-field":
        keys = ["python","class_var"]
    else:
        keys = ["python","mypy","pydantic","profile_sha256","source_sha256","fields_path","fields_sha256","serialized_type"]
        if family!="field-types":
            keys.append("model_types")
        keys += ["aliases","variants","project_code_executed","development_only","accepted"]
    if list(raw)!=keys:
        raise ValueError("Pydantic original stdout schema or field order differs")
    return raw


def validate_raw_identity(family: str, minor: int, raw: dict, work: Path, profile: Path,
                          identity: dict, lock: dict) -> dict | None:
    if family=="model-config-field":
        field_raw = (work / "field.json").read_bytes()
        field = json.loads(field_raw)
        original_raw = (work / "identity.json").read_bytes()
        original = json.loads(original_raw)
        package = Path(identity["modules"]["pydantic"]["path"]).parent
        sources = {name:digest((package / name).read_bytes()) for name in ("main.py","config.py","mypy.py")}
        if (raw["python"]!=f"3.{minor}" or type(raw["class_var"]) is not bool
                or type(field) is not dict or raw["class_var"]!=field.get("class_var")
                or field_raw!=(json.dumps(field,indent=2)+"\n").encode()
                or original_raw!=(json.dumps(original,indent=2)+"\n").encode()
                or original!={"mypy":"2.4.0","pydantic":"2.13.5","profile_sha256":lock["profile_sha256"],"source_sha256":sources}):
            raise ValueError("Pydantic original config-field output/identity differs")
        return field
    fields = Path(raw["fields_path"])
    actual_fields = Path(identity["modules"]["pydantic"]["path"]).parent / "fields.py"
    if (raw["python"]!=list(map(int,VERSIONS[minor].split("."))) or raw["mypy"]!="2.4.0" or raw["pydantic"]!="2.13.5"
            or raw["profile_sha256"]!=lock["profile_sha256"] or digest(profile.read_bytes())!=lock["profile_sha256"]
            or raw["source_sha256"]!=declared_inputs(family)["field_types.pyi"]["sha256"]
            or not fields.is_absolute() or fields.resolve(strict=True)!=actual_fields.resolve(strict=True)
            or digest(fields.read_bytes())!=raw["fields_sha256"] or raw["fields_sha256"]!=lock["pydantic_fields_sha256"]
            or type(raw["variants"]) is not list or len(raw["variants"])!=6
            or any(type(item) is not str for item in raw["variants"])
            or type(raw["aliases"]) is not dict or raw["project_code_executed"] is not False
            or raw["development_only"] is not True or raw["accepted"] is not False):
        raise ValueError("Pydantic original package, profile, input or six-overload identity differs")
    return None


def index_row(family: str, minor: int, *, reason: str = "not_run") -> dict:
    return {"unit":"asset_target_section","family":family,"python_minor":minor,"python_version":VERSIONS[minor],
            "target":f"3.{minor}","expected_pointer":{"file":f"frozen-assets/{family}.json","object_key":f"targets/3.{minor}"},
            "actual_pointer":None,"command_pointer":None,"actual_exit":None,"observation_status":"not_run",
            "process_launched":False,
            "evidence_complete":False,"original_comparison_passed":False,"raw_projected_equals_frozen":None,
            "secondary_type_errors":[],"job_passed":False,"errors":[{"stage":"not_run","message":reason}],
            "semantic_roles":{role:"unknown" for role in ("positive","negative","interaction","dormant","unsupported")},
            "runtime_case_count_claimed":False,"issue_accepted":False}


def run_cell(family: str, minor: int, executable: Path, cell: Path, profile: Path,
             expected: dict, lock: dict, environment_root: Path) -> tuple[dict,dict | None]:
    from q0_pydantic_collectors import reference_argv
    from q0_pydantic_inputs import json_type_errors, materialize_work, project_record
    cell.mkdir(parents=True,exist_ok=False)
    work,capture,command_root = cell / "work",cell / "capture",cell / "command"
    capture.mkdir()
    row = index_row(family,minor)
    errors,receipt,raw,projected = [],None,None,None
    before,after = {},{}
    try:
        proof = materialize_work(family,work,profile)
        write_json(cell / "input-proof.json",proof)
        before = inventory(work) if work.exists() else {}
        write_json(cell / "source-before.json",before)
        identity_path = environment_root / "identity.json"
        identity = json.loads(identity_path.read_bytes())
        argv = reference_argv(executable,family,minor,work,profile,capture)
        code_index = argv.index("-c")+1
        (cell / "collector-source.py").write_text(argv[code_index])
        receipt = run_inherited(argv,command_root,{"family":family,"target_patch":VERSIONS[minor],"input_files":proof["files"],
                    "profile":{"path":str(profile),**file_fact(profile)},"source_before":before,
                    "collector_sha256":digest(argv[code_index].encode()),"environment_identity_sha256":digest(identity_path.read_bytes())},timeout=120)
        try:
            require_main_receipt(receipt,command_root,argv)
        except Exception as error:
            errors.append({"stage":"original_process","error_class":type(error).__name__,"message":str(error)})
        stdout = (command_root / "stdout.raw").read_bytes()
        raw = decode_record(family,stdout)
        field = validate_raw_identity(family,minor,raw,work,profile,identity,lock)
        projected = project_record(family,raw,field=field)
        write_json(cell / "projected-section.json",projected)
        validate_capture(family,minor,executable,work,capture,profile,identity,stdout)
    except Exception as error:
        errors.append({"stage":"original_capture","error_class":type(error).__name__,"message":str(error)})
    try:
        after = inventory(work) if work.exists() else {}
        write_json(cell / "source-after.json",after)
        require_work_files(family,work,before,after)
        checked_bytes(profile.read_bytes(),lock["profile_sha256"])
    except Exception as error:
        errors.append({"stage":"input_preservation","error_class":type(error).__name__,"message":str(error)})
    launched = receipt is not None and type(receipt.get("pid")) is int and receipt["pid"]>0
    row.update(observation_status="observed" if launched else "launch_failed" if receipt else "not_run",process_launched=launched,
               errors=errors,actual_exit=receipt.get("actual_exit") if receipt else None,
               command_pointer=f"collections/{family}/python3{minor}/command/receipt.json" if receipt else None,
               original_stdout_pointer=f"collections/{family}/python3{minor}/command/stdout.raw" if receipt else None,
               original_stderr_pointer=f"collections/{family}/python3{minor}/command/stderr.raw" if receipt else None,
               source_before=before,source_after=after,input_files_unchanged=all(after.get(k)==v for k,v in declared_inputs(family).items()))
    if projected is not None:
        row.update(actual_pointer={"file":f"collections/{family}/python3{minor}/projected-section.json"},
                   raw_projected_equals_frozen=projected==expected,
                   secondary_type_errors=json_type_errors(projected,expected))
    row["evidence_complete"] = bool(not errors and projected is not None)
    row["original_comparison_passed"] = row["evidence_complete"] and row["raw_projected_equals_frozen"] is True
    row["job_passed"] = row["original_comparison_passed"] and not row["secondary_type_errors"]
    write_json(cell / "result.json",row)
    return row,projected


def write_index(path: Path, rows: list[dict]) -> None:
    with path.open("x",encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row)+"\n")


def validate_lock(lock: dict) -> None:
    if (lock.get("schema")!="mamba.q0.164.pydantic-reference-lock.v1" or lock.get("version")!="2.13.5"
            or lock.get("mypy")!="2.4.0" or lock.get("python_versions")!=list(VERSIONS.values())
            or lock.get("families")!=list(FAMILIES) or lock.get("asset_target_count")!=12
            or lock.get("reference_timeout_seconds")!=120 or lock.get("platform")!=HOST_CONTEXT
            or set(lock["dependencies"])!=set(VERSIONS.values()) or set(lock["interpreters"])!=set(VERSIONS.values())):
        raise ValueError("Pydantic reference lock scope differs")
    from q0_pydantic_inputs import input_spec
    for pins in lock["dependencies"].values():
        require_pins(pins)
    for family in FAMILIES:
        spec = input_spec(family)
        if (lock["stub_pins"][family]!={"filename":spec["filename"],"bytes":spec["source_bytes"],"sha256":spec["source_sha256"]}
                or lock["config_sha256"]!=spec["config_sha256"]):
            raise ValueError("Pydantic original stub or config identity differs")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    for minor in VERSIONS:
        parser.add_argument(f"--python3{minor}",type=Path,required=True)
    args = parser.parse_args()
    if not args.repository.is_absolute() or not args.output.is_absolute():
        parser.error("repository and output must be absolute paths")
    repository,output = args.repository.resolve(),args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    common_error,environments,rows,families = None,{},[],{}
    try:
        require_host_context(HOST_CONTEXT)
        from q0_pydantic_inputs import assemble_asset, json_type_errors, read_assets
        lock_path = repository / "tools/ci/q0_pydantic_reference_lock.json"
        lock = json.loads(lock_path.read_bytes())
        validate_lock(lock)
        assets = read_assets(repository)
        profile = repository / "crates/mamba-quality/assets/profile.json"
        strict_flags = json.loads(profile.read_bytes())["mypy"]["strict_flags"]
        write_json(output / "run-identity.json",{"source_sha":os.environ.get("GITHUB_SHA"),"run_id":os.environ.get("GITHUB_RUN_ID"),
            "run_attempt":os.environ.get("GITHUB_RUN_ATTEMPT"),"platform":HOST_CONTEXT,"cwd":os.getcwd(),
            "repository":str(repository),"lock":file_fact(lock_path),"profile":file_fact(profile),
            "source_inventory_sha256":lock["source_inventory_sha256"],"source_files":lock["source_files"],
            "families":list(FAMILIES),"target_patch_order":list(VERSIONS.values()),"asset_target_count":12,
            "process_timeout_seconds":120,"original_processes_shared":False,"reference_stdin":"inherited; bytes not captured",
            "helper_sources":{name:file_fact(repository / "tools/ci" / name) for name in
                ("q0_pydantic_reference.py","q0_pydantic_collectors.py","q0_pydantic_inputs.py","q0_mypy_reference.py","q0_ruff_reference.py")}})
        (output / "frozen-assets").mkdir()
        for family,pin in lock["asset_pins"].items():
            raw = checked_bytes((repository / pin["path"]).read_bytes(),pin["sha256"])
            if len(raw)!=pin["bytes"]:
                raise ValueError("Pydantic frozen asset size differs")
            (output / "frozen-assets" / (family+".json")).write_bytes(raw)
        archive = download(lock["sdist"],output / "downloads/sdist")
        if len(archive)!=lock["sdist"]["bytes"]:
            raise ValueError("Pydantic source archive size differs")
        cached = {}
        # Preparation is separate. Every original API child below has its own process.
        for minor,version in VERSIONS.items():
            environment_root = output / f"python3{minor}"
            try:
                python = getattr(args,f"python3{minor}")
                if not python.is_absolute():
                    raise ValueError("Pydantic interpreter path must be absolute")
                pins,wheels = lock["dependencies"][version],[]
                for pin in pins:
                    key = pin["filename"],pin["sha256"]
                    if key not in cached:
                        target = output / "downloads" / f"wheel-{len(cached):02}"
                        download(pin,target)
                        cached[key] = target / pin["filename"]
                    wheels.append(cached[key])
                executable = prepare(python,version,environment_root,pins,wheels,lock["interpreters"][version],strict_flags)
                environments[minor] = {"prepared":True,"executable":str(executable),"environment_root":str(environment_root)}
            except Exception as error:
                environments[minor] = {"prepared":False,"error_class":type(error).__name__,"message":str(error)}
                write_json(output / f"python3{minor}-failure.json",environments[minor])
        for family in FAMILIES:
            sections,cell_rows = {},[]
            for minor in VERSIONS:
                if environments[minor]["prepared"]:
                    try:
                        row,projected = run_cell(family,minor,Path(environments[minor]["executable"]),
                            output / "collections" / family / f"python3{minor}",profile,assets[family]["targets"][f"3.{minor}"],
                            lock,output / f"python3{minor}")
                        if projected is not None:
                            sections[f"3.{minor}"] = projected
                    except Exception as error:
                        row = index_row(family,minor,reason=type(error).__name__+": "+str(error))
                else:
                    row = index_row(family,minor,reason="target environment preparation failed")
                rows.append(row)
                cell_rows.append(row)
            assembled = assemble_asset(family,sections) if len(sections)==3 else None
            equality = assembled==assets[family] if assembled is not None else None
            type_errors = json_type_errors(assembled,assets[family]) if assembled is not None else ["asset assembly incomplete"]
            family_root = output / "collections" / family
            family_root.mkdir(parents=True,exist_ok=True)
            if assembled is not None:
                (family_root / "assembled-asset.json").write_text(json.dumps(assembled,indent=2)+"\n")
            families[family] = {"complete_sections":sum(r["evidence_complete"] for r in cell_rows),
                "original_python_equality":equality,"secondary_type_errors":type_errors,
                "job_passed":bool(equality is True and not type_errors and all(r["job_passed"] for r in cell_rows))}
            write_json(family_root / "comparison.json",families[family])
    except Exception as error:
        common_error = {"error_class":type(error).__name__,"message":str(error)}
    present = {(r["family"],r["python_minor"]) for r in rows}
    for family in FAMILIES:
        for minor in VERSIONS:
            if (family,minor) not in present:
                rows.append(index_row(family,minor,reason="common prerequisite failed"))
    rows.sort(key=lambda r:(FAMILIES.index(r["family"]),list(VERSIONS).index(r["python_minor"])))
    coverage = [(r["family"],r["python_minor"]) for r in rows]==[(f,m) for f in FAMILIES for m in VERSIONS]
    write_index(output / "index.jsonl",rows)
    passed = bool(common_error is None and coverage and all(r["job_passed"] for r in rows)
                  and len(families)==4 and all(f["job_passed"] for f in families.values()))
    summary = {"schema":"mamba.q0.164.pydantic-hosted-reference.v1","issue":164,"common_error":common_error,
               "environments":environments,"families":families,"asset_target_units":len(rows),"unique_ordered_coverage":coverage,
               "observed_original_processes":sum(r["process_launched"] for r in rows),
               "evidence_complete":coverage and all(r["evidence_complete"] for r in rows),
               "original_comparison_passed":coverage and all(r["original_comparison_passed"] for r in rows),
               "job_passed":passed,"issue_accepted":False,"whole_suite_accepted":False,"full_reference_accepted":False,
               "historical_original_evidence_recovered":False,"native_parity":"not_run","ancillary_pydantic_config":"not_run",
               "ancillary_pydantic_construct":"not_run","SAME217_SOM":"not_run","runtime_case_count_claimed":False}
    write_json(output / "summary.json",summary)
    write_json(output / "artifact-manifest.json",inventory(output,exclude_venvs=True))
    print(json.dumps({"job_passed":passed,"asset_target_units":len(rows),"issue_accepted":False}))
    return 0 if passed else 1


if __name__=="__main__":
    raise SystemExit(main())
