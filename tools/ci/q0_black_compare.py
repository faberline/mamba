"""Pure checks for the frozen Q0 Black API records.

The primary comparison follows black/tests/reference.rs and safety.rs.
Record validation and secondary metadata equality are separate checks.
Nothing here imports Black or proves native stability or AST equivalence.
"""
from __future__ import annotations

import hashlib
from pathlib import PurePosixPath
import re


SUCCESS_FIELDS = (
    "output", "sha256", "reference_converges", "target_syntax_hint",
    "reference_check_exit", "newline",
)
ERROR_FIELDS = ("reference_error", "target_syntax_hint", "reference_check_exit")
MESSAGE_MARKER = "This diff might be helpful: "
_LOG_NAME = re.compile(r"blk_[A-Za-z0-9_]+\.log")
_SHA256 = re.compile(r"[0-9a-f]{64}")


def _kind(record: dict) -> str | None:
    if list(record) == list(SUCCESS_FIELDS):
        return "success"
    if list(record) == list(ERROR_FIELDS):
        return "error"
    return None


def _split_error(message: str) -> tuple[str, str] | None:
    if ": " not in message:
        return None
    kind, text = message.split(": ", 1)
    return (kind, text) if kind else None


def _record_errors(record: dict) -> list[str]:
    """Check the record shape without an input-dependent status check."""
    if not isinstance(record, dict):
        return ["record must be a dict"]
    kind = _kind(record)
    if kind is None:
        return ["record keys must match the exact success or error schema in order"]
    errors = []
    hint = record["target_syntax_hint"]
    if hint is not None and type(hint) is not str:
        errors.append("target_syntax_hint must be a string or None")
    if type(record["reference_check_exit"]) is not int:
        errors.append("reference_check_exit must be an int, not a bool")
    if kind == "error":
        error = record["reference_error"]
        if type(error) is not str or _split_error(error) is None:
            errors.append("reference_error must contain a nonempty kind and ': '")
        elif not _is_utf8(error):
            errors.append("reference_error must encode as UTF-8")
    else:
        output = record["output"]
        if type(output) is not str:
            errors.append("output must be a string")
        elif not _is_utf8(output):
            errors.append("output must encode as UTF-8")
        digest = record["sha256"]
        if type(digest) is not str or _SHA256.fullmatch(digest) is None:
            errors.append("sha256 must be 64 lowercase hexadecimal characters")
        if type(record["reference_converges"]) is not bool:
            errors.append("reference_converges must be a bool")
        if type(record["newline"]) is not str or record["newline"] not in ("\n", "\r\n", "\r"):
            errors.append("newline must be LF, CRLF or CR")
    if type(hint) is str and not _is_utf8(hint):
        errors.append("target_syntax_hint must encode as UTF-8")
    return errors


def _is_utf8(text: str) -> bool:
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        return False
    return True


def validate_record(record: dict, case: dict) -> list[str]:
    """Validate exact original API fields, bytes and computed status.

    A false reference_converges value is valid recorded evidence. It is never
    changed to true. The status is computed data, not an observed process exit.
    """
    errors = _record_errors(record)
    if not isinstance(case, dict) or type(case.get("source")) is not str:
        return errors + ["case source must be a string"]
    source = case["source"]
    if not _is_utf8(source):
        return errors + ["case source must encode as UTF-8"]
    if errors:
        return errors
    if _kind(record) == "error":
        if record["reference_check_exit"] != 123:
            errors.append("error reference_check_exit must be computed status 123")
    else:
        output = record["output"]
        if hashlib.sha256(output.encode("utf-8")).hexdigest() != record["sha256"]:
            errors.append("sha256 does not match UTF-8 output bytes")
        if record["reference_check_exit"] != int(source != output):
            errors.append("reference_check_exit does not match int(source != output)")
    return errors


def validate_batch(records: list[dict], cases: list[dict]) -> list[str]:
    """Reject zero/partial batches before checking every ordered record."""
    if type(records) is not list or type(cases) is not list:
        return ["records and cases must be lists"]
    if not cases:
        return ["case denominator must be nonzero"]
    if len(records) != len(cases):
        return ["record count does not match case count"]
    return [f"record {index}: {error}" for index, (record, case) in
            enumerate(zip(records, cases)) for error in validate_record(record, case)]


def _message_prefix(message: str) -> str | None:
    if MESSAGE_MARKER not in message:
        return None
    prefix, path = message.rsplit(MESSAGE_MARKER, 1)
    # Rust Path::file_name on the supported POSIX hosts ignores trailing '/'.
    # This checks the existing basename grammar. It performs no filesystem IO.
    name = PurePosixPath(path).name
    return prefix if _LOG_NAME.fullmatch(name) is not None else None


def compare_records(actual: dict, expected: dict) -> dict:
    """Apply only the published primary comparator to validated API records.

    Call validate_record for both records before accepting an observation.
    The ASTSafetyError path rule deliberately has no parent-directory gate,
    matching message_equal. Separate log/source checks remain required.
    """
    for label, record in (("actual", actual), ("expected", expected)):
        errors = _record_errors(record)
        if errors:
            return {"equal": False, "reason": f"{label} record invalid: {errors[0]}",
                    "approved_path_variant": False}
    actual_kind, expected_kind = _kind(actual), _kind(expected)
    if actual_kind != expected_kind:
        return {"equal": False, "reason": "success/error kinds differ",
                "approved_path_variant": False}
    if expected_kind == "success":
        equal = actual["output"].encode("utf-8") == expected["output"].encode("utf-8")
        return {"equal": equal, "reason": "output bytes match" if equal else "output bytes differ",
                "approved_path_variant": False}
    actual_class, actual_message = _split_error(actual["reference_error"])
    expected_class, expected_message = _split_error(expected["reference_error"])
    if actual_class != expected_class:
        return {"equal": False, "reason": "error classes differ",
                "approved_path_variant": False}
    if expected_class == "ASTSafetyError":
        actual_prefix = _message_prefix(actual_message)
        expected_prefix = _message_prefix(expected_message)
        equal = (actual_prefix is not None and expected_prefix is not None
                 and actual_prefix == expected_prefix)
        variant = equal and actual_message != expected_message
        return {"equal": equal,
                "reason": ("ASTSafetyError message matches the existing log-path rule" if equal
                           else "ASTSafetyError message fails the existing log-path rule"),
                "approved_path_variant": variant}
    equal = actual_message == expected_message
    return {"equal": equal, "reason": "error messages match" if equal else "error messages differ",
            "approved_path_variant": False}


def compare_secondary_fields(actual: dict, expected: dict) -> dict:
    """Compare all frozen fields outside the original output/error comparator."""
    for label, record in (("actual", actual), ("expected", expected)):
        errors = _record_errors(record)
        if errors:
            return {"equal": False, "reason": f"{label} record invalid: {errors[0]}",
                    "differing_fields": []}
    if _kind(actual) != _kind(expected):
        return {"equal": False, "reason": "success/error kinds differ", "differing_fields": []}
    primary = "output" if _kind(actual) == "success" else "reference_error"
    different = [key for key in expected if key != primary and actual[key] != expected[key]]
    return {"equal": not different,
            "reason": "secondary fields match" if not different else "secondary fields differ",
            "differing_fields": different}
