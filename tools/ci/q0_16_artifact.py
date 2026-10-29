#!/usr/bin/env python3
"""Pack hosted receipts without following fixture links or including venvs."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def excluded(parts: tuple[str, ...], tool: str) -> bool:
    if parts[:4] == ("run", "environments", tool, "venv"):
        return True
    return (len(parts) >= 5 and parts[:2] == ("run", "requests")
            and re.fullmatch(r"[0-9a-f]{64}", parts[2]) is not None
            and parts[3:5] == ("metadata", "venv"))


def pack(root: Path, output: Path, manifest: Path, tool: str) -> dict:
    if tool not in {"mypy", "pylint"} or root.is_symlink() or not root.is_dir():
        raise ValueError("a real receipts directory and exact lint tool are required")
    root = root.resolve(strict=True)
    if output.is_symlink() or manifest.is_symlink():
        raise ValueError("archive and manifest cannot replace symbolic links")
    output, manifest = output.resolve(), manifest.resolve()
    if (output == manifest or output.is_relative_to(root) or manifest.is_relative_to(root)
            or output.exists() or manifest.exists()):
        raise ValueError("new archive and manifest must be outside the input tree")
    pending = output.with_name(output.name + ".pending")
    if pending.exists():
        raise ValueError("pending archive already exists")
    rows, omitted = [], []

    def visit(archive: tarfile.TarFile, path: Path, relative: PurePosixPath) -> None:
        if excluded(relative.parts, tool):
            omitted.append(relative.as_posix())
            return
        before = path.lstat()
        name = "receipts/" + relative.as_posix()
        info = tarfile.TarInfo(name)
        info.mode = stat.S_IMODE(before.st_mode)
        info.uid, info.gid, info.mtime = before.st_uid, before.st_gid, before.st_mtime
        row = {"path": relative.as_posix(), "mode": info.mode}
        if stat.S_ISLNK(before.st_mode):
            target = os.readlink(path)
            info.type, info.linkname = tarfile.SYMTYPE, target
            archive.addfile(info)
            row.update(kind="symlink", target=target, bytes=len(os.fsencode(target)),
                       sha256=digest(os.fsencode(target)))
        elif stat.S_ISDIR(before.st_mode):
            info.type = tarfile.DIRTYPE
            archive.addfile(info)
            row.update(kind="directory")
            for child in sorted(path.iterdir(), key=lambda p: p.name):
                visit(archive, child, relative / child.name)
        elif stat.S_ISREG(before.st_mode):
            raw = path.read_bytes()
            info.type, info.size = tarfile.REGTYPE, len(raw)
            archive.addfile(info, io.BytesIO(raw))
            row.update(kind="regular", bytes=len(raw), sha256=digest(raw))
        else:
            raise ValueError("unsupported evidence node: " + str(relative))
        after = path.lstat()
        fields = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
        if any(getattr(before, key) != getattr(after, key) for key in fields):
            raise ValueError("evidence changed during packing: " + str(relative))
        rows.append(row)

    with tarfile.open(pending, mode="x", format=tarfile.PAX_FORMAT, dereference=False) as archive:
        for child in sorted(root.iterdir(), key=lambda p: p.name):
            visit(archive, child, PurePosixPath(child.name))
    archive_hash = hashlib.sha256()
    with pending.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            archive_hash.update(block)
    result = {"schema": "mamba.q0.16.safe-lint-artifact.v1", "tool": tool,
              "archive": {"filename": output.name, "bytes": pending.stat().st_size,
                          "sha256": archive_hash.hexdigest()},
              "entries": sorted(rows, key=lambda r: r["path"]),
              "excluded_venv_roots": sorted(omitted), "symlinks_followed": False,
              "fixture_bytes_changed": False}
    with manifest.open("x") as stream:
        json.dump(result, stream, ensure_ascii=True, indent=2)
        stream.write("\n")
    pending.rename(output)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--tool", choices=("mypy", "pylint"), required=True)
    args = parser.parse_args()
    result = pack(args.receipts, args.output, args.manifest, args.tool)
    print(json.dumps({"archive": result["archive"], "entries": len(result["entries"]),
                      "symlinks_followed": False}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
