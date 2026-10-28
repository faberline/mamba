"""Apply only the published Pylint comparator's two message normalizers."""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import re

VARIANTS_SHA256 = "b45fab2b60a6e0bdaf20d30e018444901eba22c450d7ac31e1122482a9646afe"
AUTHORIZATION_SHA256 = "1ee035820d71d178981e605f2ffdd9b624ecfa14862c52ba0a5d9d598a5d782b"
_ENCODING_PREFIX = "error while code parsing: Wrong or no encoding specified for "
_CRASH_MESSAGE = re.compile(
    r"(.*?): Fatal error while checking '(.*?)'\. Please open an issue in our bug tracker so we address this\. "
    r"There is a pre-filled template that you can use in '(.*/)?pylint-crash-"
    r"([0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2})\.txt'\."
)


def _same_path(value: str, path: str) -> bool:
    return value == path or value.endswith("/" + path)


def normalize_diagnostics(diagnostics: list[dict]) -> list[dict]:
    """Return a copy; keep list order, fields, and all unapproved text exact."""
    if not isinstance(diagnostics, list):
        raise ValueError("Pylint diagnostics must be a list")
    normalized = copy.deepcopy(diagnostics)
    # The owning Rust callsite applies path_prefix before crash_filename.
    for diagnostic in normalized:
        if diagnostic.get("rule") != "F0010":
            continue
        message, path = diagnostic["message"], diagnostic["path"]
        if not isinstance(message, str) or not isinstance(path, str):
            raise ValueError("encoding diagnostic message and path must be strings")
        if message.startswith(_ENCODING_PREFIX) and message.endswith("."):
            embedded = message[len(_ENCODING_PREFIX):-1]
            if _same_path(embedded, path):
                diagnostic["message"] = f"{_ENCODING_PREFIX}<project>/{path}."
    for diagnostic in normalized:
        if diagnostic.get("rule") != "F0002":
            continue
        message, path = diagnostic["message"], diagnostic["path"]
        if not isinstance(message, str) or not isinstance(path, str):
            raise ValueError("fatal diagnostic message and path must be strings")
        match = _CRASH_MESSAGE.fullmatch(message)
        if match and _same_path(match[1], path) and _same_path(match[2], path):
            diagnostic["message"] = (
                f"{path}: Fatal error while checking '{path}'. Please open an issue in our bug tracker so we address this. "
                "There is a pre-filled template that you can use in '<cache>/pylint-crash-<timestamp>.txt'."
            )
    return normalized


def _json_equal(left: object, right: object) -> bool:
    """Keep JSON booleans distinct from numbers, as serde_json::Value does."""
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(_json_equal(left[k], right[k]) for k in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(_json_equal(a, b) for a, b in zip(left, right))
    return left == right


class PylintComparator:
    """Read the unchanged approval fixture once and compare complete lists."""

    def __init__(self, repo: Path):
        path = repo / "crates/mamba-quality/src/infrastructure/pylint/tests/variants.json"
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != VARIANTS_SHA256:
            raise ValueError("published Pylint variants SHA-256 differs")
        fixture = json.loads(raw)
        entries = fixture["cases"]
        if (fixture["schema"] != "mamba.quality.pylint-observed-variants.v1"
                or fixture["pylint"] != "4.1.2" or fixture["astroid"] != "4.3.3"
                or fixture["human_authorization_sha256"] != AUTHORIZATION_SHA256
                or len(entries) != 29 or sum(len(row["runs"]) for row in entries) != 87
                or len({(row["name"], row["python"]) for row in entries}) != 29
                or any(d["rule"] == "F0002" for row in entries for run in row["runs"] for d in run["diagnostics"])):
            raise ValueError("published Pylint variant approval identity differs")
        self._variants = {(row["name"], row["python"]): row for row in entries}

    def compare(self, case: dict, minor: int, actual: list[dict], actual_exit: int) -> dict:
        if type(minor) is not int or minor not in (12, 13, 14):
            raise ValueError("unsupported Pylint comparison target")
        target = minor - 12
        if len(case["expected"]) != 3 or len(case["reference_exits"]) != 3:
            raise ValueError("Pylint comparison requires all three frozen targets")
        normalized_actual = normalize_diagnostics(actual)
        normalized_expected = normalize_diagnostics(case["expected"][target])
        frozen_exit = case["reference_exits"][target]
        exit_matches = type(actual_exit) is int and type(frozen_exit) is int and actual_exit == frozen_exit
        matched_by = None
        if exit_matches and _json_equal(normalized_actual, normalized_expected):
            matched_by = "canonical"
        else:
            entry = self._variants.get((case["name"], f"3.{minor}"))
            if (exit_matches and entry is not None
                    and _json_equal(entry["source_hashes"], case["source_sha256"])
                    and any(type(run["exit_code"]) is int and run["exit_code"] == actual_exit
                            and _json_equal(run["diagnostics"], normalized_actual) for run in entry["runs"])):
                matched_by = "approved_whole_list_variant"
        return {"matched": matched_by is not None, "matched_by": matched_by,
                "normalized_actual": normalized_actual, "normalized_expected": normalized_expected,
                "actual_exit": actual_exit, "reference_exit": frozen_exit,
                "variant_fixture_sha256": VARIANTS_SHA256}
