//! Public API regression for assignment binding order and walrus flow.
use mamba_quality::application::quality::CompletionStatus;
use mamba_quality::{run, QualityOperation, QualityRequest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

fn compare_fixed_python312(
    source: &str,
    source_sha256: &str,
    original: &str,
    original_sha256: &str,
) {
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        source_sha256
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        original_sha256
    );
    let project = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
    )
    .unwrap();
    let path = root.join("main.py");
    fs::write(&path, source).unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Check,
        vec![path.clone()],
    ));
    assert_eq!(report.exit_code, 2);
    assert!(!report.analysis_completed);
    let tool = report
        .tools
        .iter()
        .find(|tool| tool.tool.name() == "mypy")
        .unwrap();
    assert_eq!(tool.completion, CompletionStatus::Incomplete);
    assert_eq!(tool.reference_exit_code, None);
    assert_eq!(tool.checked_files, 1);
    assert_eq!(report.files.written, 0);
    let actual: Vec<_> = tool
        .diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })
        })
        .collect();
    let expected: Vec<Value> = if original.as_bytes() == b"\n"
        && original_sha256 == "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b"
    {
        Vec::new()
    } else {
        original
            .lines()
            .map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                json!({
                    "rule": row["code"], "message": row["message"], "hint": row["hint"],
                    "path": row["file"], "severity": row["severity"],
                    "location": {"line": row["line"], "column": row["column"].as_u64().unwrap() + 1,
                        "end_line": row["end_line"], "end_column": row["end_column"]},
                })
            })
            .collect()
    };
    assert_eq!(actual, expected);
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
}

#[test]
fn nested_first_assignment_matches_fixed_original_on_py312() {
    // Moved unchanged from the existing nested_first_binding unit regression.
    let nested_first_binding = r###"def after_try(text: str) -> int:
    try:
        number = int(text)
    except ValueError:
        return 0
    first = number
    number = 0
    return first + number


def actual_forward_read() -> int:
    result = number
    number = 1
    return result
"###;
    let nested_first_binding_original = r###"{"file": "main.py", "line": 12, "column": 13, "end_line": 12, "end_column": 19, "message": "Cannot determine type of \"number\"", "hint": null, "code": "has-type", "severity": "error"}
{"file": "main.py", "line": 12, "column": 13, "end_line": 12, "end_column": 19, "message": "Name \"number\" is used before definition", "hint": null, "code": "used-before-def", "severity": "error"}
{"file": "main.py", "line": 14, "column": 4, "end_line": 14, "end_column": 17, "message": "Returning Any from function declared to return \"int\"", "hint": null, "code": "no-any-return", "severity": "error"}
"###;
    compare_fixed_python312(
        nested_first_binding,
        "a8d84ebf804aca99db0827864dca2eeb8f1f1ba9a11f5b41caaa2017f00870c1",
        nested_first_binding_original,
        "c2207ac481909db860c7a87655468d047b89c901810420fc7e879834ec91f8eb",
    );
}

#[test]
fn comprehension_filter_walrus_matches_fixed_original_on_py312() {
    // Fixed original reproducer. The first comprehension is valid; the second
    // reads m before the filter binds it.
    let comprehension_walrus_order = r###"def filtered(values: list[int]) -> dict[int, int]:
    return {value: m for value in values if (m := value)}


def read_before_filter_bind(values: list[int]) -> dict[int, int]:
    return {value: m for value in values if m if (m := value)}
"###;
    let comprehension_walrus_order_original = r###"{"file": "main.py", "line": 6, "column": 44, "end_line": 6, "end_column": 45, "message": "Cannot determine type of \"m\"", "hint": null, "code": "has-type", "severity": "error"}
{"file": "main.py", "line": 6, "column": 44, "end_line": 6, "end_column": 45, "message": "Name \"m\" is used before definition", "hint": null, "code": "used-before-def", "severity": "error"}
"###;
    compare_fixed_python312(
        comprehension_walrus_order,
        "1e8caee901029aee7396d05b1a01805d3d7d021c6c75ecc67530767d04fd6303",
        comprehension_walrus_order_original,
        "07cdf4ddb71cd484a65b6b7f5d6e79e7bf01229ead4f92d05dc903b628108988",
    );
}

#[test]
fn first_definition_matches_fixed_original_on_py312() {
    let first_definition = r###"def from_pairs(pairs: list[tuple[int, str]]) -> str:
    for index, prompt in pairs:
        return prompt
    prompt = ""
    return prompt
"###;
    let first_definition_original = "\n";
    compare_fixed_python312(
        first_definition,
        "1f35e39af7f4efa94fe33adf23019e54be30713a1a5c940a10bed258584610ad",
        first_definition_original,
        "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    );
}

#[test]
fn partial_set_membership_matches_fixed_original_on_py312() {
    let partial_set_membership = r###"def unique(values: list[str]) -> set[str]:
    seen = set()
    for value in values:
        if value not in seen:
            seen.add(value)
    return seen

def read_before_add() -> None:
    values = set()
    reveal_type(values)
    values.add(1)
    reveal_type(values)

"###;
    let partial_set_membership_original = r###"{"file": "main.py", "line": 9, "column": 4, "end_line": 9, "end_column": 10, "message": "Need type annotation for \"values\" (hint: \"values: set[<type>] = ...\")", "hint": null, "code": "var-annotated", "severity": "error"}
{"file": "main.py", "line": 10, "column": 16, "end_line": 10, "end_column": 22, "message": "Revealed type is \"set[Any]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 12, "column": 16, "end_line": 12, "end_column": 22, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    compare_fixed_python312(
        partial_set_membership,
        "38a175f43daaf0c20a7fc8ff17709e475eb266aa18153decc5548c417c01546f",
        partial_set_membership_original,
        "539dd0caa7ba6dbba50433294ce77da932272b6abf2cf2614fe257304197be7a",
    );
}

#[test]
fn contextmanager_entered_type_matches_fixed_original_on_py312() {
    let contextmanager_entered_type = r###"from collections.abc import Iterator
from contextlib import contextmanager
@contextmanager
def managed() -> Iterator[None]:
    yield
with managed() as value:
    reveal_type(value)
"###;
    let contextmanager_entered_type_original = r###"{"file": "main.py", "line": 7, "column": 16, "end_line": 7, "end_column": 21, "message": "Revealed type is \"None\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    compare_fixed_python312(
        contextmanager_entered_type,
        "c63b0dc8d425b3f05a442e3060f14c7ccf3e83ddb6d0ad4cdfb238760e84b376",
        contextmanager_entered_type_original,
        "a5eea078e24af2b6927d978802b25bd9fea9b3c775693a96ce38c7bf4c0a6f6f",
    );
}

#[test]
fn union_iterable_items_match_fixed_original_on_py312() {
    let union_iterable = r###"from typing import Any

class ScenarioFamily:
    pass

def inspect(rows: list[dict[str, Any]] | list[ScenarioFamily]) -> None:
    reveal_type([row for row in rows])
    [row["id"] for row in rows]
"###;
    let union_iterable_original = r###"{"file": "main.py", "line": 7, "column": 16, "end_line": 7, "end_column": 37, "message": "Revealed type is \"list[dict[str, Any] | main.ScenarioFamily]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 8, "column": 5, "end_line": 8, "end_column": 14, "message": "Value of type \"dict[str, Any] | ScenarioFamily\" is not indexable", "hint": null, "code": "index", "severity": "error"}
"###;
    compare_fixed_python312(
        union_iterable,
        "8c78d88637394563cc0a865bff23254e40fd9f41f64cef4695fc3354884227ab",
        union_iterable_original,
        "498713f36e7d584698ddeae3b190c888deedace505bfe5c44883030ee911591b",
    );
}

#[test]
fn http_handler_inherited_field_matches_fixed_original_on_py312() {
    let http_handler = r###"from http.server import BaseHTTPRequestHandler

class Handler(BaseHTTPRequestHandler):
    def may_send_body(self) -> bool:
        return self.command != "HEAD"
"###;
    let http_handler_original = "\n";
    compare_fixed_python312(
        http_handler,
        "142c8988f348370b40b002f9b6e400e645979354de26c54d7ee8d18e3dfef0cf",
        http_handler_original,
        "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    );
}

#[test]
fn tuple_instance_fields_match_fixed_original_on_py312() {
    let tuple_fields = r###"class Pair:
    def __init__(self, number: int, text: str) -> None:
        self.number, self.text = number, text
    def read(self) -> str:
        return self.text
"###;
    let tuple_fields_original = "\n";
    compare_fixed_python312(
        tuple_fields,
        "deb90db775ceaf609477fdce47ec522723cd58c9190b32081a12d5fadad83447",
        tuple_fields_original,
        "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b",
    );
}
