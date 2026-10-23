use super::*;

#[test]
fn empty_builtin_set_add_and_fixed_state_controls() {
    // Main's completed Python 3.12 source and original rows, including the old negative.
    let main = r###"def collect(roots: list[int]) -> None:
    workspaces = set()
    for root in roots:
        if root:
            workspaces.add(root)
    reveal_type(workspaces)
"###;
    let negative = r###"from typing import AbstractSet, Any, Iterable, Iterator, Sequence, TypeVar
result = set()
reveal_type(result)
"###;
    let original = r###"{"file": "case_18999.py", "line": 2, "column": 0, "end_line": 2, "end_column": 6, "message": "Need type annotation for \"result\" (hint: \"result: set[<type>] = ...\")", "hint": null, "code": "var-annotated", "severity": "error"}
{"file": "case_18999.py", "line": 3, "column": 12, "end_line": 3, "end_column": 18, "message": "Revealed type is \"set[Any]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 6, "column": 16, "end_line": 6, "end_column": 26, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    // Directory-root original rows, in the order captured from the original runner.
    let directory_original = r###"{"file": "main.py", "line": 6, "column": 16, "end_line": 6, "end_column": 26, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "case_18999.py", "line": 2, "column": 0, "end_line": 2, "end_column": 6, "message": "Need type annotation for \"result\" (hint: \"result: set[<type>] = ...\")", "hint": null, "code": "var-annotated", "severity": "error"}
{"file": "case_18999.py", "line": 3, "column": 12, "end_line": 3, "end_column": 18, "message": "Revealed type is \"set[Any]\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    let states = r###"def ignored_annotation(roots: list[int]) -> None:
    values = set()  # type: ignore[var-annotated]
    for root in roots:
        values.add(root)
    reveal_type(values)

def read_before_add() -> None:
    values = set()
    reveal_type(values)
    values.add(1)
    reveal_type(values)

def reassign_before_add() -> None:
    values = set()
    values = set()
    values.add(1)
    reveal_type(values)

def nested_add() -> None:
    values = set()
    def write() -> None:
        values.add(1)
    reveal_type(values)
"###;
    let states_original = r###"{"file": "main.py", "line": 2, "column": -1, "end_line": 2, "end_column": -1, "message": "Unused \"type: ignore\" comment", "hint": null, "code": "unused-ignore", "severity": "error"}
{"file": "main.py", "line": 5, "column": 16, "end_line": 5, "end_column": 22, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 8, "column": 4, "end_line": 8, "end_column": 10, "message": "Need type annotation for \"values\" (hint: \"values: set[<type>] = ...\")", "hint": null, "code": "var-annotated", "severity": "error"}
{"file": "main.py", "line": 9, "column": 16, "end_line": 9, "end_column": 22, "message": "Revealed type is \"set[Any]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 11, "column": 16, "end_line": 11, "end_column": 22, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 14, "column": 4, "end_line": 14, "end_column": 10, "message": "Need type annotation for \"values\" (hint: \"values: set[<type>] = ...\")", "hint": null, "code": "var-annotated", "severity": "error"}
{"file": "main.py", "line": 17, "column": 16, "end_line": 17, "end_column": 22, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
{"file": "main.py", "line": 23, "column": 16, "end_line": 23, "end_column": 22, "message": "Revealed type is \"set[int]\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    for (source, expected) in [
        (
            states,
            "1aa602ca5f7d4d8222ca0c87f30c0009a562daf054f398836e4324f5aa529d55",
        ),
        (
            states_original,
            "5b1e22aeb964f86fe91ce691e495e372c04bbc01c4db15fe3f49aaa548ea2136",
        ),
        (
            main,
            "d67d319c2939f8478761ba2b11d6893211583e3e78eb558f685122bb7382fb46",
        ),
        (
            negative,
            "8ddd22330cd9b91e5ae40bc29289d75fd66d2c56e5ca3cb708cd11f6ddaa1299",
        ),
        (
            original,
            "603c5add1ba54ed8c727f91d3f834dd494cf60a5e38e837d32af7f04546b1d22",
        ),
        (
            directory_original,
            "6904779171a2a4b81bb806ac124974f3d31f98f9e572aa19d0fc97e24f5e55a7",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(source.as_bytes())), expected);
    }
    let check = |files: Vec<(String, String)>, expected: Value, version: PythonVersion| {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        for (name, source) in &files {
            fs::write(root_path.join(name), source).unwrap();
        }
        let mut workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
        workspace.projects[0].python_version = version;
        let report = super::super::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(report.checked_files, files.len());
        let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
            "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
            "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
            "severity": diagnostic.severity,
            "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
        })).collect();
        assert_eq!(Value::Array(actual), expected);
        for (name, source) in files {
            assert_eq!(fs::read(root_path.join(name)).unwrap(), source.as_bytes());
        }
    };
    let original_rows = |raw: &str| {
        let rows: Vec<Value> = raw
            .lines()
            .map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                json!({"rule": row["code"], "message": row["message"], "hint": row["hint"],
                    "path": row["file"], "severity": row["severity"],
                    "location": {"line": row["line"], "column": row["column"].as_i64().unwrap() + 1,
                        "end_line": row["end_line"], "end_column": row["end_column"]}})
            })
            .collect();
        Value::Array(rows)
    };
    let name = "set_tuple_constructor1_set_shadowed_value";
    let mut cases = catalog::CHUNKS
        .iter()
        .filter(|chunk| chunk.contains(name))
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
        .filter(|case| case["name"].as_str() == Some(name));
    let case = cases.next().expect("existing set shadow control");
    assert!(cases.next().is_none(), "ambiguous existing set control");
    let source = case["source"].as_str().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        case["source_sha256"].as_str().unwrap()
    );
    // Match the existing pinned_original_corpus target order for catalog expected rows.
    let targets = [
        PythonVersion::Py312,
        PythonVersion::Py313,
        PythonVersion::Py314,
    ];
    assert_eq!(case["expected"].as_array().unwrap().len(), targets.len());
    for (target, version) in targets.into_iter().enumerate() {
        check(
            vec![
                ("main.py".into(), main.into()),
                ("case_18999.py".into(), negative.into()),
            ],
            original_rows(directory_original),
            version,
        );
        check(
            vec![("main.py".into(), states.into())],
            original_rows(states_original),
            version,
        );
        check(
            vec![(case["filename"].as_str().unwrap().into(), source.into())],
            case["expected"][target].clone(),
            version,
        );
    }
}
