"""Build the fixed Black inputs from pinned source bytes and public input fields."""
from __future__ import annotations

from collections import Counter
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile

CASE_COUNT = 4172
CHUNK_COUNT = 131
UPSTREAM_COUNT = 260
EXTRA_COUNT = 3912
INPUT_BYTES = 2036273
INPUT_SHA256 = "7c293118b1f493b4244049116fe6308cdebc4b1a3c161b6988cae3a1bdc2246e"
EXTRA_BYTES = 641041
EXTRA_SHA256 = "d9918c69d50d04e4cc801911a92be1893139909001468a8621eb0cf4febcfbd4"
CORPUS_SHA256 = "9adbd99901efc776a87117d3546eab85b4d75ad56bbd72cc8868d97533fa60da"
ARCHIVE_BYTES = 728037
ARCHIVE_SHA256 = "b3476ce71b494fc6e39d77e75488e8339a87583029bbf608416e955c83582bd6"
CATALOG_INVENTORY_SHA256 = "80112297058fed872122e5019f18536c955195b29f36d539b49a500d5b5b57a5"
PROFILE_SHA256 = "59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7"
TEST_ROOT = "crates/mamba-quality/src/infrastructure/black/tests"
EXTRA_ORIGIN = "tools/python_quality/black_cases.json"
EMPTY_MARKER = "# EMPTY LINE WITH WHITESPACE (this comment will be removed)"
COMPARATOR_SOURCES = {
    TEST_ROOT + "/reference.rs": "9b51dec1ed171a31b86eb2fd73853d00e0a9b278e328eb81d5936da068348bc7",
    TEST_ROOT + "/safety.rs": "02f125bc578583e85e968062241f90cad1120601244a435a400c509b5be44b02",
    TEST_ROOT + "/errors.rs": "96a8db2b9112b01e47c57372f601089f7593dabaef8c153fb7c4bf8bfc6efe6b",
    TEST_ROOT + "/catalog.rs": "1c4a8c81dd6e348186b7cefe6a43c88c6feebcf628d38213d5f37709da9543d0",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _parts(repository: Path) -> list[Path]:
    root = repository / TEST_ROOT
    header = (root / "catalog.rs").read_text()
    names = [f"part_{index:03}" for index in range(CHUNK_COUNT)]
    if (re.findall(r"(part_\d+)::CASES", header) != names
            or re.findall(r"^mod (part_\d+);", header, re.M) != names
            or f"CASE_COUNT: usize = {CASE_COUNT};" not in header
            or CORPUS_SHA256 not in header):
        raise ValueError("published Black index identity or order differs")
    paths = [root / "catalog" / (name + ".rs") for name in names]
    if set(paths) != set((root / "catalog").glob("*.rs")):
        raise ValueError("published Black chunk denominator differs")
    return [root / "catalog.rs", *paths]


def source_inventory(repository: Path) -> list[dict]:
    return [{"path": path.relative_to(repository).as_posix(), "sha256": digest(path.read_bytes())}
            for path in _parts(repository)]


def _verify_repository(repository: Path) -> list[dict]:
    sources = source_inventory(repository)
    if digest(json.dumps(sources, sort_keys=True).encode()) != CATALOG_INVENTORY_SHA256:
        raise ValueError("published Black source inventory differs")
    if digest((repository / "crates/mamba-quality/assets/profile.json").read_bytes()) != PROFILE_SHA256:
        raise ValueError("published Black profile differs")
    for name, expected in COMPARATOR_SOURCES.items():
        if digest((repository / name).read_bytes()) != expected:
            raise ValueError("owning Black comparator source differs: " + name)
    lock = json.loads((repository / "tools/ci/q0_black_reference_lock.json").read_bytes())
    pins = {"version": "26.10.0", "case_count": CASE_COUNT,
            "input_sha256": INPUT_SHA256, "input_bytes": INPUT_BYTES,
            "extra_input_sha256": EXTRA_SHA256, "historical_corpus_sha256": CORPUS_SHA256,
            "profile_sha256": PROFILE_SHA256,
            "published_catalog_inventory_sha256": CATALOG_INVENTORY_SHA256}
    if any(lock.get(key) != value for key, value in pins.items()):
        raise ValueError("Black reference input lock differs")
    if lock.get("comparator_sources") != COMPARATOR_SOURCES:
        raise ValueError("Black reference comparator lock differs")
    if lock["sdist"]["sha256"] != ARCHIVE_SHA256:
        raise ValueError("Black reference archive lock differs")
    return sources


def _published(repository: Path) -> list[dict]:
    rows = []
    for index, path in enumerate(_parts(repository)[1:]):
        match = re.fullmatch(r'.*?pub const CASES: &str = r(?P<h>#+)"(?P<body>.*)"(?P=h);\s*',
                             path.read_text(), re.S)
        if match is None:
            raise ValueError("invalid published Black JSON container")
        chunk = json.loads(match["body"])
        if not isinstance(chunk, list) or len(chunk) != (12 if index == CHUNK_COUNT - 1 else 32):
            raise ValueError("published Black chunk row count differs")
        rows.extend(chunk)
    if len(rows) != CASE_COUNT or len({row["id"] for row in rows}) != CASE_COUNT:
        raise ValueError("published Black case denominator differs")
    for row in rows:
        _validate_source(row)
    return rows


def _validate_source(row: dict) -> None:
    if not isinstance(row["source"], str) or digest(row["source"].encode()) != row["source_sha256"]:
        raise ValueError("published Black source bytes differ")
    if len(row["origins"]) != 1 or row.get("extension", "py") not in ("py", "pyi"):
        raise ValueError("published Black input layout differs")


def read_source(raw: bytes) -> tuple[str, str]:
    lines, flags = [], ""
    for line in raw.decode().splitlines(keepends=True):
        if not lines and line.startswith("# flags: "):
            flags += line[len("# flags: "):].strip()
            continue
        if line.rstrip() == "# output":
            break
        lines.append(line.replace(EMPTY_MARKER, ""))
    return "".join(lines).strip() + "\n", flags


def archive_inputs(archive: bytes) -> tuple[list[dict], list[dict], list[dict]]:
    """Read regular source members only. Never extract an archive member."""
    cases, files, inventory, seen = [], [], [], set()
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as bundle:
        members = bundle.getmembers()
        for member in members:
            name = member.name
            parts = name.split("/")
            if (not name or "\\" in name or name.startswith("/")
                    or any(part in ("", ".", "..") for part in parts)
                    or PurePosixPath(name).as_posix() != name or name in seen
                    or not (member.isfile() or member.isdir())):
                raise ValueError("unsafe Black source archive member")
            seen.add(name)
            inventory.append({"path": name, "type": "file" if member.isfile() else "directory",
                              "bytes": member.size})
        for member in sorted(members, key=lambda item: item.name):
            if (not member.isfile() or not member.name.endswith((".py", ".pyi"))
                    or not any(part in member.name for part in
                               ("/tests/data/cases/", "/tests/data/miscellaneous/"))):
                continue
            stream = bundle.extractfile(member)
            if stream is None:
                raise ValueError("missing Black source archive payload")
            raw = stream.read()
            if len(raw) != member.size:
                raise ValueError("truncated Black source archive payload")
            source, flags = read_source(raw)
            sha = digest(raw)
            files.append({"path": member.name, "sha256": sha})
            cases.append({"id": digest((member.name + "\0" + source).encode()),
                          "source": source, "source_sha256": digest(source.encode()),
                          "extension": "pyi" if "--pyi" in flags or member.name.endswith(".pyi") else "py",
                          "origins": [{"path": member.name, "sha256": sha, "original_flags": flags}]})
    return cases, files, inventory


def custom_inputs(rows: list[dict]) -> tuple[list[dict], bytes]:
    extras, cases = [], []
    for row in rows:
        extra = {key: value for key, value in row.items()
                 if key not in {"id", "source_sha256", "origins", "expected"}}
        if (set(extra) not in ({"name", "source"}, {"name", "source", "extension"})
                or row["origins"] != [{"path": EXTRA_ORIGIN, "case": extra["name"]}]):
            raise ValueError("published Black custom input fields differ")
        _validate_source(row)
        case = {"id": digest(json.dumps(extra, sort_keys=True).encode()), **extra,
                "source_sha256": digest(extra["source"].encode()), "origins": row["origins"]}
        if case["id"] != row["id"]:
            raise ValueError("published Black custom input ID differs")
        extras.append(extra)
        cases.append(case)
    return cases, (json.dumps(extras, indent=2, ensure_ascii=True) + "\n").encode()


def _fixed_counts(cases: list[dict], upstream_count: int, extra_count: int) -> None:
    if (len(cases) != CASE_COUNT or upstream_count != UPSTREAM_COUNT or extra_count != EXTRA_COUNT
            or len({case["id"] for case in cases}) != CASE_COUNT
            or Counter(case.get("extension", "py") for case in cases) != {"py": 4131, "pyi": 41}):
        raise ValueError("Black fixed input denominators differ")


def build_inputs(repository: Path, archive: bytes) -> tuple[list[dict], dict]:
    sources = _verify_repository(repository)
    if len(archive) != ARCHIVE_BYTES or digest(archive) != ARCHIVE_SHA256:
        raise ValueError("Black source archive identity differs")
    rows = _published(repository)
    upstream, files, members = archive_inputs(archive)
    if len(members) != 495:
        raise ValueError("Black source archive member denominator differs")
    extras, extra_raw = custom_inputs(rows[UPSTREAM_COUNT:])
    if len(extra_raw) != EXTRA_BYTES or digest(extra_raw) != EXTRA_SHA256:
        raise ValueError("original Black custom input bytes differ")
    cases = upstream + extras
    _fixed_counts(cases, len(upstream), len(extras))
    # These are input fields only. Frozen expected values never enter this comparison.
    public_inputs = [{key: value for key, value in row.items() if key != "expected"} for row in rows]
    if cases != public_inputs or any(list(a) != list(b) for a, b in zip(cases, public_inputs)):
        raise ValueError("reconstructed Black input fields or order differ")
    raw = json.dumps(cases).encode()
    if len(raw) != INPUT_BYTES or digest(raw) != INPUT_SHA256:
        raise ValueError("Black fixed input transport bytes differ")
    proof = {"case_count": CASE_COUNT, "target_count": 3, "target_cells": CASE_COUNT * 3,
             "chunk_count": CHUNK_COUNT, "upstream_case_count": len(upstream),
             "extra_case_count": len(extras), "extension_counts": {"py": 4131, "pyi": 41},
             "input_bytes": len(raw), "input_sha256": digest(raw),
             "extra_input_bytes": len(extra_raw), "extra_input_sha256": digest(extra_raw),
             "archive_bytes": len(archive), "archive_sha256": digest(archive),
             "archive_member_count": len(members), "archive_members": members,
             "test_files": files, "published_sources": sources,
             "published_catalog_inventory_sha256": CATALOG_INVENTORY_SHA256,
             "all_published_input_fields_match": True, "input_fields_only": True,
             "expected_values_used_to_construct_inputs": False,
             "historical_original_evidence_recovered": False}
    return cases, proof


def read_expected(repository: Path) -> list[dict]:
    """Verify frozen output identity separately from input construction."""
    _verify_repository(repository)
    rows = _published(repository)
    for row in rows:
        if not isinstance(row["expected"], list) or len(row["expected"]) != 3:
            raise ValueError("Black frozen target denominator differs")
        for expected in row["expected"]:
            if "output" in expected and digest(expected["output"].encode()) != expected["sha256"]:
                raise ValueError("Black frozen output hash differs")
    files = [{"path": row["origins"][0]["path"], "sha256": row["origins"][0]["sha256"]}
             for row in rows[:UPSTREAM_COUNT]]
    errors = [row for row in rows if any("reference_error" in value for value in row["expected"])]
    corpus = {"schema": "mamba.quality.black-corpus.v1", "version": "26.10.0",
              "line_length": 88, "preview": False, "archive_sha256": ARCHIVE_SHA256,
              "test_files": files, "cases": rows, "reference_errors": errors, "excluded": [],
              "whole_suite_accepted": False}
    raw = (json.dumps(corpus, indent=2, ensure_ascii=True) + "\n").encode()
    if len(raw) != 8701905 or digest(raw) != CORPUS_SHA256 or len(errors) != 235:
        raise ValueError("Black frozen corpus content identity differs")
    return rows
