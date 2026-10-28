#!/usr/bin/env python3
"""Prepare an exact, archive-bound Mac interpreter only on the hosted runner."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import stat
import tarfile
from urllib.parse import quote, urlsplit
from urllib.request import urlopen

from q0_pylint_reference import HOST_CONTEXT, VERSIONS, require_host_context
from q0_mypy_reference import command, require_python_identity
from q0_ruff_reference import digest, inventory, write_json


def require_pin(version: str, pin: dict) -> None:
    if version not in VERSIONS.values() or pin.get("version") != version:
        raise ValueError("interpreter version differs from the fixed target")
    release = pin.get("release", "")
    flavor = pin.get("flavor")
    filename = f"cpython-{version}+{release}-aarch64-apple-darwin-{flavor}.tar.gz"
    url = f"https://github.com/astral-sh/python-build-standalone/releases/download/{release}/{quote(filename, safe='')}"
    if (flavor not in {"install_only", "install_only_stripped"}
            or len(release) != 8 or not release.isdigit() or pin.get("filename") != filename
            or pin.get("url") != url or not isinstance(pin.get("bytes"), int)
            or not 0 < pin["bytes"] <= 100_000_000):
        raise ValueError("interpreter artifact identity differs")
    for field in ("sha256", "executable_sha256"):
        value = pin.get(field, "")
        if len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
            raise ValueError("interpreter digest is missing or invalid")


def download(pin: dict, output: Path) -> Path:
    record = {"url": pin["url"], "filename": pin["filename"],
              "expected_sha256": pin["sha256"], "expected_bytes": pin["bytes"],
              "new_download": True, "passed": False}
    path = output / pin["filename"]
    try:
        with urlopen(pin["url"], timeout=120) as response:
            final = urlsplit(response.geturl())
            if final.scheme != "https" or final.hostname not in {
                    "github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"}:
                raise ValueError("interpreter redirect left the official release hosts")
            raw = response.read(100_000_001)
            # Signed redirect queries are access-bearing and are not evidence fields.
            record.update(final_host=final.hostname, http_status=response.status,
                          bytes=len(raw), actual_sha256=digest(raw))
        path.write_bytes(raw)
        if len(raw) != pin["bytes"] or digest(raw) != pin["sha256"]:
            raise ValueError("interpreter archive bytes differ from the fixed pin")
        record["passed"] = True
        return path
    except Exception as error:
        record["error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        write_json(output / "download.json", record)


def extract(archive: Path, prefix: Path) -> tuple[list[dict], dict]:
    prefix.mkdir(parents=True, exist_ok=False)
    members = []
    with tarfile.open(archive, "r:gz") as source:
        seen = set()
        for member in source.getmembers():
            name = PurePosixPath(member.name)
            if (name.is_absolute() or ".." in name.parts or not name.parts
                    or name.parts[0] != "python" or str(name) != member.name.rstrip("/") or str(name) in seen
                    or not (member.isfile() or member.isdir() or member.issym() or member.islnk())):
                raise ValueError("unsafe or duplicate interpreter archive member")
            seen.add(str(name))
            tarfile.data_filter(member, str(prefix))
            row = {"name": member.name, "bytes": member.size, "mode": member.mode,
                   "type": member.type.decode("ascii"), "linkname": member.linkname}
            if member.isfile():
                with source.extractfile(member) as stream:
                    row["sha256"] = digest(stream.read())
            members.append(row)
        # The data filter also checks links against the actual extraction tree.
        source.extractall(prefix, filter="data")
    installed = {}
    for path in sorted(prefix.rglob("*")):
        name = str(path.relative_to(prefix))
        if path.is_symlink():
            if not path.resolve().is_relative_to(prefix.resolve()):
                raise ValueError("interpreter link escapes the fresh prefix")
            installed[name] = {"type": "symlink", "target": os.readlink(path)}
        elif path.is_file():
            raw = path.read_bytes()
            installed[name] = {"type": "file", "bytes": len(raw), "sha256": digest(raw)}
        elif path.is_dir():
            installed[name] = {"type": "directory"}
        else:
            raise ValueError("unexpected installed interpreter file type")
        installed[name]["mode"] = stat.S_IMODE(path.lstat().st_mode)
    for row in members:
        if "sha256" in row:
            actual = installed[row["name"]]
            if ({key: actual.get(key) for key in ("type", "bytes", "sha256")}
                    != {"type": "file", "bytes": row["bytes"], "sha256": row["sha256"]}):
                raise ValueError("extracted interpreter payload differs from its archive")
    return members, installed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--python-version", required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    summary = {"passed": False, "historical_environment_recovered": False,
               "issue_accepted": False, "python_version": args.python_version}
    try:
        require_host_context(HOST_CONTEXT)
        repository = args.repository.resolve()
        lock_path = repository / "tools/ci/q0_pylint_reference_lock.json"
        lock = json.loads(lock_path.read_bytes())
        if set(lock["interpreters"]) != set(VERSIONS.values()):
            raise ValueError("interpreter target denominator differs")
        require_host_context(lock["platform"])
        pin = lock["interpreters"][args.python_version]
        require_pin(args.python_version, pin)
        archive = download(pin, output)
        prefix = args.prefix.resolve()
        members, installed = extract(archive, prefix)
        write_json(output / "archive-members.json", members)
        write_json(output / "installed-payload.json", installed)
        minor = int(args.python_version.split(".")[1])
        executable = prefix / "python/bin" / f"python3.{minor}"
        if digest(executable.read_bytes()) != pin["executable_sha256"]:
            raise ValueError("interpreter executable differs from its fixed archive payload")
        probe = ("import json,platform,sys; print(json.dumps({'python':list(sys.version_info[:3]),"
                 "'executable':sys.executable,'isolated':sys.flags.isolated,"
                 "'machine':platform.machine(),'sys_platform':sys.platform}))")
        command([str(executable), "-I", "-B", "-c", probe], output, output / "identity-probe",
                inputs={"archive_sha256": pin["sha256"], "executable_sha256": pin["executable_sha256"]})
        identity = json.loads((output / "identity-probe/stdout.raw").read_bytes())
        require_python_identity(identity, args.python_version)
        if identity["machine"] != "arm64" or identity["sys_platform"] != "darwin":
            raise ValueError("interpreter architecture or OS differs")
        summary.update(passed=True, executable=str(executable), pin=pin,
                       lock_sha256=digest(lock_path.read_bytes()), identity=identity,
                       source_sha=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository, text=True).strip(),
                       run_id=os.environ.get("GITHUB_RUN_ID"), run_attempt=os.environ.get("GITHUB_RUN_ATTEMPT"),
                       helper_sha256=digest(Path(__file__).read_bytes()))
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
