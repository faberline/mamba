//! Full original diagnostic lists retain order, locations, and severity.
mod build_probes;
mod catalog;
mod crash_filename;
mod cross_file_stores;
mod dependency_io;
mod encoding_controls;
mod encoding_declaration_controls;
mod entry_inputs;
mod entry_io;
mod entry_path_errno;
mod imported_function_aliases;
mod path_prefix;
mod starred_comprehensions;
mod terminal_reference_cycles;
mod variants;
use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

#[test]
fn pinned_original_corpus() {
    compare_corpus(0, "MAMBA_QUALITY_PYLINT_EVIDENCE");
}

#[test]
fn pinned_latest_differential() {
    const { assert!(catalog::REUSED_CASE_COUNT < catalog::CASE_COUNT) };
    compare_corpus(
        catalog::REUSED_CASE_COUNT,
        "MAMBA_QUALITY_PYLINT_TAIL_EVIDENCE",
    );
}

fn compare_corpus(start: usize, evidence: &str) {
    let mut evidence = std::env::var_os(evidence).map(|destination| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .expect("create new Pylint evidence file");
        let mut writer = std::io::BufWriter::new(file);
        writer
            .write_all(b"{\"cases\":[")
            .expect("write Pylint evidence header");
        writer
    });
    let mut first_result = true;
    let mut cases_seen = 0usize;
    let mut comparisons = 0usize;
    let mut failures = 0;
    for case in catalog::GROUPS
        .iter()
        .flat_map(|group| group.iter())
        .flat_map(|chunk| serde_json::from_str::<Vec<Value>>(chunk).unwrap())
    {
        cases_seen += 1;
        if cases_seen <= start {
            continue;
        }
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let files = case["files"].as_object().unwrap();
        for (name, value) in files {
            let source = value.as_str().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(source.as_bytes())),
                case["source_sha256"][name].as_str().unwrap()
            );
            let path = root_path.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        let mut workspace = Filesystem.load(std::slice::from_ref(&root_path), true);
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
            let fatal = report.diagnostics.iter().any(|value| value.rule == "F0002");
            assert_eq!(
                report.completion,
                if fatal {
                    CompletionStatus::Error
                } else {
                    CompletionStatus::Incomplete
                }
            );
            assert_eq!(report.reference_exit_code.is_some(), fatal);
            assert_eq!(
                report.checked_files,
                files.keys().filter(|name| name.ends_with(".py")).count()
            );
            let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| json!({
                "rule": diagnostic.rule, "message": diagnostic.message, "hint": diagnostic.hint,
                "path": diagnostic.path.strip_prefix(&root_path).unwrap(), "severity": diagnostic.severity,
                "location": {"line": diagnostic.location.line, "column": diagnostic.location.column,
                    "end_line": diagnostic.location.end_line, "end_column": diagnostic.location.end_column},
            })).collect();
            let mut actual = Value::Array(actual);
            let mut expected = case["expected"][target].clone();
            path_prefix::normalize(&mut actual);
            path_prefix::normalize(&mut expected);
            crash_filename::normalize(&mut actual);
            crash_filename::normalize(&mut expected);
            let diagnostic_exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
                exit | match diagnostic.severity.as_str() {
                    "fatal" => 1,
                    "error" => 2,
                    "warning" => 4,
                    "refactor" => 8,
                    "convention" => 16,
                    "info" => 0,
                    severity => panic!("Unknown original Pylint severity: {severity}"),
                }
            });
            if fatal {
                assert_eq!(report.reference_exit_code, Some(diagnostic_exit as i32));
            }
            let canonical_equal = actual == expected
                && diagnostic_exit == case["reference_exits"][target].as_i64().unwrap();
            let observed_variant =
                !canonical_equal && variants::observed(&case, target, &actual, diagnostic_exit);
            let equal = canonical_equal || observed_variant;
            failures += usize::from(!equal);
            comparisons += 1;
            if let Some(writer) = evidence.as_mut() {
                let mut row = json!({"name":case["name"], "files":files, "python":version.name(),
                    "expected":expected, "actual":actual, "equal":equal,
                    "diagnostic_exit":diagnostic_exit,
                    "reference_exit":case["reference_exits"][target]});
                if observed_variant {
                    row["observed_original_variant"] = json!(true);
                }
                if !first_result {
                    writer
                        .write_all(b",")
                        .expect("write Pylint evidence separator");
                }
                serde_json::to_writer(&mut *writer, &row).expect("write Pylint evidence row");
                first_result = false;
            }
            for (name, value) in files {
                assert_eq!(
                    fs::read(root_path.join(name)).unwrap(),
                    value.as_str().unwrap().as_bytes(),
                    "Pylint source check changed {name}"
                );
            }
        }
    }
    assert_eq!(cases_seen, catalog::CASE_COUNT);
    assert_eq!(comparisons, (catalog::CASE_COUNT - start) * 3);
    if let Some(writer) = evidence.as_mut() {
        writer
            .write_all(b"],\"corpus_sha256\":")
            .expect("write Pylint evidence footer");
        serde_json::to_writer(&mut *writer, catalog::CORPUS_SHA256)
            .expect("write Pylint corpus hash");
        write!(
            writer,
            ",\"failed\":{failures},\"schema\":\"mamba.quality.pylint-comparison.v1\""
        )
        .expect("write Pylint evidence counts");
        if start > 0 {
            writer
                .write_all(b",\"scope\":")
                .expect("write Pylint evidence scope key");
            serde_json::to_writer(&mut *writer,
                &json!({"first_project":start,"source_projects":catalog::CASE_COUNT-start,"full_corpus":false}))
                .expect("write Pylint evidence scope");
        }
        writer
            .write_all(b",\"whole_suite_accepted\":false}")
            .expect("write Pylint evidence completion");
        writer.flush().expect("flush complete Pylint evidence");
    }
    assert_eq!(failures, 0, "Pinned Pylint default diagnostics differ");
}

mod unicode_notes;

mod length_controls;

mod width_markers;
