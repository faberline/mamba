"""Rebuild the fixed Ruff inputs from pinned upstream source, without results."""

from __future__ import annotations

import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile

SELECT = "E4,E7,E9,F,UP,B,SIM,RUF100"
GROUPS = ("pyflakes", "pycodestyle", "pyupgrade", "flake8_bugbear", "flake8_simplify", "ruff")
GROUP_COUNTS = (185, 76, 106, 57, 36, 130)


def baseline(crate_bytes: bytes) -> dict[str, bytes]:
    """Read 590 upstream fixtures and add the six original baseline controls."""
    sources: dict[str, bytes] = {}
    prefix = "ruff_linter-0.16.10/resources/test/fixtures/"
    with tarfile.open(fileobj=io.BytesIO(crate_bytes), mode="r:gz") as archive:
        for member in sorted(archive.getmembers(), key=lambda value: value.name):
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not (member.isfile() or member.isdir()):
                raise ValueError(f"unsafe source member: {member.name}")
            if not member.name.startswith(prefix) or not member.isfile():
                continue
            relative = member.name[len(prefix):]
            if relative.split("/", 1)[0] not in GROUPS or path.suffix not in (".py", ".pyi"):
                continue
            if relative in sources:
                raise ValueError(f"duplicate source member: {relative}")
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError(f"missing source bytes: {relative}")
            sources[relative] = stream.read()
    counts = tuple(sum(name.startswith(group + "/") for name in sources) for group in GROUPS)
    if counts != GROUP_COUNTS:
        raise ValueError(f"upstream fixture counts differ: {counts}")
    sources.update({
        "generated/F621.py": (", ".join(f"item{i}" for i in range(256)) + ", *rest = range(300)\n").encode(),
        "generated/E902.py": b"# coding: latin-1\nlabel = 'caf\xe9'\n",
        "generated/utf8_latin1.py": "# coding: latin-1\ncafé = 1\nprint(café, absent)\n".encode(),
        "generated/invalid_utf8.py": b"label = '\xff'\n",
        "generated/unknown_encoding.py": b"# coding: does-not-exist\nprint(absent)\n",
        "generated/bom.py": b"\xef\xbb\xbfimport os\nprint('water', absent)\n",
    })
    if len(sources) != 596:
        raise ValueError("baseline must have 596 inputs")
    return dict(sorted(sources.items()))


def expanded(
    sources: dict[str, bytes],
    raw_reports: dict[int, list[dict]],
    project_roots: dict[int, Path],
    rules: list[str],
) -> tuple[dict[str, bytes], dict[int, dict[str, dict[str, str]]]]:
    """Select directive witnesses from fresh original diagnostics, not expected data."""
    if len(sources) != 596 or len(rules) != 174 or len(set(rules)) != 174:
        raise ValueError("baseline/profile denominator differs")
    observed: dict[int, dict[str, set[str]]] = {}
    for minor in (12, 13, 14):
        observed[minor] = {}
        for message in raw_reports[minor]:
            code = message["code"]
            if code is None:
                continue
            name = Path(message["filename"]).relative_to(project_roots[minor]).as_posix()
            if name not in sources:
                raise ValueError(f"diagnostic outside baseline: {name}")
            observed[minor].setdefault(code, set()).add(name)
    if set(rules) - set(observed[14]) != {"F621"}:
        raise ValueError("original dormant-rule set differs")
    full = dict(sources)
    witnesses: dict[int, dict[str, dict[str, str]]] = {minor: {} for minor in (12, 13, 14)}
    for code in rules:
        if code == "F621":
            continue
        active = [observed[minor][code] for minor in (12, 13, 14) if code in observed[minor]]
        common = set.intersection(*active)
        for minor in (12, 13, 14):
            names = common or observed[minor].get(code, observed[14][code])
            name = sorted(names)[0]
            raw = sources[name]
            if code == "E902":
                raw = raw.decode("latin-1").encode("utf-8")
            else:
                bom = raw[:3] if raw.startswith(b"\xef\xbb\xbf") else b""
                raw = bom + f"# ruff: noqa: {code}\n".encode() + raw[len(bom):]
            destination = f"directives/{code}/{name}"
            if destination in full and full[destination] != raw:
                raise ValueError(f"target-dependent directive source: {destination}")
            full[destination] = raw
            witnesses[minor][code] = {"negative": name, "positive": destination}
    controls = {
        "clean.py": b"VALUE = 1\n", "clean.pyi": b"VALUE: int\n",
        "safe_unsafe_conflict.py": b"import os\nassert False\n",
        "noqa.py": b"import os  # noqa: F401\nimport sys  # noqa: F821\n",
        "noqa_next.py": b"# noqa\nimport os\n",
        "noqa_all.py": b"# ruff: noqa\nimport os\nprint(missing)\n",
        "crlf.py": b"import os\r\nVALUE = 1\r\n", "cr.py": b"import os\rVALUE = 1\r",
        "unicode.py": "import os\nprint('水', absent)\n".encode(),
        "bom.py": b"\xef\xbb\xbfimport os\nVALUE = 1\n",
        "stub.pyi": b"import os\nimport sys as sys\nVALUE: int\n",
        "invalid.py": b"import os\ndef broken(:\n",
        "package/__init__.py": b"from .member import VALUE\nimport os\n",
        "package/member.py": b"VALUE = 1\n",
        "missing_module.py": b"from missing_package import member\n",
    }
    full.update({f"controls/{name}": raw for name, raw in controls.items()})
    if len(full) != 784:
        raise ValueError(f"fixed corpus must have 784 inputs, got {len(full)}")
    return dict(sorted(full.items())), witnesses


def verify_frozen_inputs(repo: Path, sources: dict[str, bytes]) -> dict:
    """Compare input hashes only; never read expected outcomes to create inputs."""
    root = repo / "crates/mamba-quality/src/infrastructure/ruff/tests/catalog"
    expected: dict[str, str] = {}
    for part in sorted(root.glob("part_*.rs")):
        match = re.search(r'pub const CASES: &str = r(?P<marks>#+)"(?P<body>.*)"(?P=marks);', part.read_text(), re.S)
        if match is None:
            raise ValueError(f"cannot read published input identities: {part.name}")
        for row in json.loads(match["body"]):
            if row["path"] in expected:
                raise ValueError(f"duplicate published input: {row['path']}")
            expected[row["path"]] = row["source_sha256"]
    actual = {name: hashlib.sha256(raw).hexdigest() for name, raw in sources.items()}
    if len(expected) != 784 or actual != expected:
        missing = sorted(expected.keys() - actual.keys())
        extra = sorted(actual.keys() - expected.keys())
        different = sorted(name for name in expected.keys() & actual.keys() if expected[name] != actual[name])
        raise ValueError(f"frozen input mismatch: expected={len(expected)} actual={len(actual)} missing={missing} extra={extra} different={different}")
    encoded = json.dumps(actual, sort_keys=True, separators=(",", ":")).encode()
    return {"cases": 784, "all_input_hashes_match": True,
            "input_index_sha256": hashlib.sha256(encoded).hexdigest(),
            "identity_source": "published input SHA fields only; expected results were not used",
            "historical_corpus_recovered": False}
