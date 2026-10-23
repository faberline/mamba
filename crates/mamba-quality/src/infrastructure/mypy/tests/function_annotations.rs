//! Fixed native-parser annotation presence and messages retain every original row.
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn function_annotations_match_fixed_original_contract() {
    let fixed = [
        (
            r###"def no_value():
    pass

def value():
    return 1

def generator():
    yield 1
"###,
            r###"{"file": "main.py", "line": 1, "column": 0, "end_line": 2, "end_column": 8, "message": "Function is missing a return type annotation", "hint": "Use \"-> None\" if function does not return a value", "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 4, "column": 0, "end_line": 5, "end_column": 12, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 7, "column": 0, "end_line": 8, "end_column": 11, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
"###,
            "3784549cb99cd956b03eaf1860ec6970f4ea4018e6140043d64f0a26c225d708",
            "13f5ebaba4ffcc3cbd1e79ef3d32ce5fc7754acd60597e1ff2705e2fe7734aa0",
        ),
        (
            r###"def f():
    # type: int
    pass
"###,
            r###"{"file": "main.py", "line": 1, "column": 0, "end_line": 3, "end_column": 8, "message": "Function is missing a return type annotation", "hint": "Use \"-> None\" if function does not return a value", "code": "no-untyped-def", "severity": "error"}
"###,
            "80087f16927500cb89e13235bee323ba8b657f4dbfc8e0b592e30317d33c2200",
            "995ac3872f613abeeb768d53e1cdd3205bb6c307a492b688561f2742a15957ec",
        ),
        (
            r###"def inline_comment():  # type: () -> None
    pass

def leading_comment():
    # type: () -> None
    pass

def parameter_comment(
    value,  # type: int
):
    pass
"###,
            r###"{"file": "main.py", "line": 8, "column": 0, "end_line": 11, "end_column": 8, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
"###,
            "ff735195507c2cfd039545a79332e7f5ca28ceaf46d707212244d45d7de17ccb",
            "75f87b189d58d008c68e5fa2328a93029226d211fb943671753fd1eddf0c7b8b",
        ),
        (
            r###"def no_annotations(value):
    pass

def return_only(value) -> None:
    pass

def parameter_only(value: int):
    pass

def partial(first: int, second):
    pass

def free_self(self):
    pass
"###,
            r###"{"file": "main.py", "line": 1, "column": 0, "end_line": 2, "end_column": 8, "message": "Function is missing a type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 4, "column": 0, "end_line": 5, "end_column": 8, "message": "Function is missing a type annotation for one or more parameters", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 7, "column": 0, "end_line": 8, "end_column": 8, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 10, "column": 0, "end_line": 11, "end_column": 8, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 10, "column": 0, "end_line": 11, "end_column": 8, "message": "Function is missing a type annotation for one or more parameters", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 13, "column": 0, "end_line": 14, "end_column": 8, "message": "Function is missing a return type annotation", "hint": "Use \"-> None\" if function does not return a value", "code": "no-untyped-def", "severity": "error"}
"###,
            "e9df663a2337ab70fbf1e8fa86867ba8cddbe66e7908f2630f030c3e8693199c",
            "37c833d680819d560f6961b7c1993f0a5a7035e37ffae1dd53749de0f05abbfc",
        ),
        (
            r###"def key_function():
    key = lambda pair: pair[1]

def none_function():
    key = lambda: None
"###,
            r###"{"file": "main.py", "line": 1, "column": 0, "end_line": 2, "end_column": 30, "message": "Function is missing a return type annotation", "hint": null, "code": "no-untyped-def", "severity": "error"}
{"file": "main.py", "line": 4, "column": 0, "end_line": 5, "end_column": 22, "message": "Function is missing a return type annotation", "hint": "Use \"-> None\" if function does not return a value", "code": "no-untyped-def", "severity": "error"}
"###,
            "2f920acbb6d42c9c8c5f43f4eb1e66824391afb49e7d2c56d58e814306961128",
            "f66af7f58f364d5a590f445661704eb581b667d734d55789dc1cf2885aa985cb",
        ),
    ];
    for &(source, original, source_sha, original_sha) in &fixed {
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            source_sha
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(original.as_bytes())),
            original_sha
        );
    }
    let controls: Vec<Value> = [
        "class_untyped_constructor",
        "new_constructor_unannotated_return_boundary_literal",
    ]
    .into_iter()
    .map(|name| {
        let mut cases = super::catalog::CHUNKS
            .iter()
            .filter(|chunk| chunk.contains(name))
            .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
            .filter(|case| case["name"].as_str() == Some(name));
        let case = cases.next().expect("existing annotation control");
        assert!(
            cases.next().is_none(),
            "ambiguous existing annotation control"
        );
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
        assert_eq!(case["expected"].as_array().unwrap().len(), 3);
        case
    })
    .collect();
    // Use the existing pinned_original_corpus expected-row target order.
    for (target, version) in [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ]
    .into_iter()
    .enumerate()
    {
        for &(source, original, _, _) in &fixed {
            let rows: Vec<Value> = original
                .lines()
                .map(|line| {
                    let row: Value = serde_json::from_str(line).unwrap();
                    json!({"rule": row["code"], "message": row["message"], "hint": row["hint"],
                        "path": row["file"], "severity": row["severity"],
                        "location": {"line": row["line"], "column": row["column"].as_i64().unwrap() + 1,
                            "end_line": row["end_line"], "end_column": row["end_column"]}})
                })
                .collect();
            check("main.py", source, version, &Value::Array(rows));
        }
        for case in &controls {
            check(
                case["filename"].as_str().unwrap(),
                case["source"].as_str().unwrap(),
                version,
                &case["expected"][target],
            );
        }
    }
}

fn check(filename: &str, source: &str, version: PythonVersion, expected: &Value) {
    let root = tempfile::tempdir().unwrap();
    let root_path = fs::canonicalize(root.path()).unwrap();
    fs::write(
        root_path.join("pyproject.toml"),
        "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
    )
    .unwrap();
    let path = root_path.join(filename);
    fs::write(&path, source).unwrap();
    let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
    workspace.projects[0].python_version = version;
    let report = super::super::check(&workspace);
    assert_eq!(report.completion, CompletionStatus::Incomplete);
    assert_eq!(report.reference_exit_code, None);
    assert_eq!(report.checked_files, 1);
    let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
        "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
        "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
        "severity": diagnostic.severity,
        "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
            "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
    })).collect();
    assert_eq!(
        &Value::Array(actual),
        expected,
        "{filename} at {}",
        version.name()
    );
    assert_eq!(fs::read(path).unwrap(), source.as_bytes());
}
