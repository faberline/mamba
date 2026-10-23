//! Original diagnostics retain their order, hints, messages, and exact locations.
mod ast_constant;
mod ast_constant_controls;
mod ast_setter_context;
mod ast_typed_module;
mod ast_union_identity;
mod async_iteration;
mod async_iteration_protocol_controls;
mod builtin_values;
mod builtin_values_regressions;
mod call_span_boundaries;
mod catalog;
mod chained_assignment_controls;
mod class_object_metadata;
mod closed_constructors;
mod compound_function_binding;
mod comprehension_filter;
mod decorator_boundaries;
mod dependencies;
mod dependency_directives;
mod dependency_error_interactions;
mod dependency_source_errors;
mod dict_fromkeys;
mod exception_alias;
mod function_annotations;
#[cfg(target_os = "macos")]
mod generator_unpack;
mod generic_starred_calls;
mod implicit_call_name;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod importlib_child;
mod importlib_path_modules;
mod json_file_functions;
mod lambda_binding;
mod lambda_scope_boundaries;
mod local_dependencies;
mod local_owner;
mod local_owner_priority;
mod math_binding_controls;
mod math_functions;
mod name_directives;
mod names;
mod narrowing;
mod operator_semantic_equality;
mod ordinary_decorator_boundaries;
mod ordinary_decorators;
mod os_environ;
mod partial_none;
mod partial_sets;
mod patterns;
mod predicate_bindings;
mod predicate_contracts;
mod projects;
mod pydantic_config;
mod pydantic_construct;
mod runtime_narrowing;
mod standard_functions;
mod standard_shadows;
mod star_export;
mod starred_call_controls;
mod stdlib_io_protocols;
mod string_joins;
mod template_strings;
mod truthiness;
mod tuple_assignment_edges;
mod tuple_assignment_order;
mod tuple_binding;
mod tuple_binding_controls;
mod typing_cast;
mod typing_cast_error_controls;
mod union_any_constraints;
mod union_boundary;
mod union_callable_any;
mod union_known_members;
mod union_missing_members;
mod update_operator_controls;
mod with_binding;
mod with_boundaries;
mod with_protocol;
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

#[test]
fn pinned_original_corpus() {
    let mut builtin = builtin_values_regressions::BuiltinRegressions::new();
    let mut evidence = std::env::var_os("MAMBA_QUALITY_MYPY_EVIDENCE").map(|destination| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .expect("create new Mypy evidence file");
        let mut writer = std::io::BufWriter::new(file);
        writer
            .write_all(b"{\"cases\":[")
            .expect("write Mypy evidence header");
        writer
    });
    let mut first_result = true;
    let mut cases_seen = 0usize;
    let mut comparisons = 0usize;
    let mut failures = 0;
    for case in catalog::CHUNKS
        .iter()
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
    {
        cases_seen += 1;
        let selected = builtin.select(case["name"].as_str().unwrap());
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let filename = case["filename"].as_str().unwrap();
        let path = root_path.join(filename);
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        for (target, version) in [
            PythonVersion::Py312,
            PythonVersion::Py313,
            PythonVersion::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            workspace.projects[0].python_version = version;
            let report = super::check(&workspace);
            assert_eq!(report.completion, CompletionStatus::Incomplete);
            assert_eq!(report.reference_exit_code, None);
            assert_eq!(report.checked_files, 1);
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": if diagnostic.rule.is_empty() { Value::Null } else { Value::String(diagnostic.rule.clone()) }, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let expected = &case["expected"][target];
            let actual = Value::Array(actual);
            let equal = actual == *expected;
            failures += usize::from(!equal);
            comparisons += 1;
            if selected {
                builtin.record(&case, version, target, &report, &actual);
            }
            if let Some(writer) = evidence.as_mut() {
                if !first_result {
                    writer
                        .write_all(b",")
                        .expect("write Mypy evidence separator");
                }
                serde_json::to_writer(
                    &mut *writer,
                    &json!({"name":case["name"],"source":source,"python":version.name(),
                    "expected":expected,"actual":actual,"equal":equal}),
                )
                .expect("write Mypy evidence row");
                first_result = false;
            }
        }
    }
    assert_eq!(cases_seen, catalog::CASE_COUNT);
    assert_eq!(comparisons, catalog::CASE_COUNT * 3);
    if let Some(writer) = evidence.as_mut() {
        writer
            .write_all(b"],\"corpus_sha256\":")
            .expect("write Mypy evidence footer");
        serde_json::to_writer(&mut *writer, catalog::CORPUS_SHA256)
            .expect("write Mypy corpus hash");
        write!(
            writer,
            ",\"failed\":{failures},\"schema\":\"mamba.quality.mypy-comparison.v1\",\"whole_suite_accepted\":false}}"
        )
        .expect("write Mypy evidence completion");
        writer.flush().expect("flush complete Mypy evidence");
    }
    builtin.finish();
    assert_eq!(failures, 0, "Pinned mypy diagnostic comparisons failed");
}

mod overload_annotations;

mod typeddict_unpack;

mod special_decorators;

mod callable_originals;

mod duplicate_modules;

mod shadow_aliases;

mod set_comprehensions;

mod str_bytes_safe;

mod fstring_flow_boundaries;

mod filtered_set_comprehensions;

#[test]
fn bare_builtin_dict_annotation_and_existing_controls() {
    // Main's completed Python 3.12 reproducer. Keep both source files and raw original rows.
    let main = r###"def task_text(prompt: dict) -> None:
    reveal_type(prompt)

def make() -> dict[str, int]:
    return dict(x=1)
"###;
    let control = r###"class dict:
    pass

def control(prompt: dict) -> None:
    pass
"###;
    let original = r###"{"file": "main.py", "line": 1, "column": 22, "end_line": 1, "end_column": 26, "message": "Missing type arguments for generic type \"dict\"", "hint": null, "code": "type-arg", "severity": "error"}
{"file": "main.py", "line": 2, "column": 16, "end_line": 2, "end_column": 22, "message": "Revealed type is \"dict[Any, Any]\"", "hint": null, "code": "misc", "severity": "note"}
"###;
    assert_eq!(
        format!("{:x}", Sha256::digest(main.as_bytes())),
        "b6f4404a63e509201d00157d8fcb3364d8c93e9d73a535ed59b3f0006a787289"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(control.as_bytes())),
        "77e5f8b8cd2306c13e371474df8ef84a1acfd32c3b28c6365847cdbbff9bc773"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "cfbdf432d2f98d5c41fe85555def06bd2b668968354bdd4cca08e5a59ff0762a"
    );
    let check = |files: Vec<(String, String)>, expected: Value| {
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
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = super::check(&workspace);
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
    let expected: Vec<Value> = original
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            json!({"rule": row["code"], "message": row["message"], "hint": row["hint"],
                "path": row["file"], "severity": row["severity"],
                "location": {"line": row["line"], "column": row["column"].as_u64().unwrap() + 1,
                    "end_line": row["end_line"], "end_column": row["end_column"]}})
        })
        .collect();
    check(
        vec![
            ("main.py".into(), main.into()),
            ("control.py".into(), control.into()),
        ],
        Value::Array(expected),
    );
    // Read two existing cases. Their sources and original expectations stay unchanged.
    for name in [
        "builtin_shape1_dict_import_alias_bare_annotation",
        "dict_constructor1_shadowed_value",
    ] {
        let mut cases = catalog::CHUNKS
            .iter()
            .filter(|chunk| chunk.contains(name))
            .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
            .filter(|case| case["name"].as_str() == Some(name));
        let case = cases.next().expect("existing dict control");
        assert!(cases.next().is_none(), "ambiguous existing dict control");
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case["source_sha256"].as_str().unwrap()
        );
        check(
            vec![(case["filename"].as_str().unwrap().into(), source.into())],
            case["expected"][0].clone(),
        );
    }
}
