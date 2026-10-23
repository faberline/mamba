use mamba_quality::{run, QualityOperation, QualityRequest};
use std::fs;

#[test]
fn missing_inputs_keep_fatal_order_and_execution_error_exit() {
    for version in ["3.12", "3.13", "3.14"] {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join("pyproject.toml"),
            format!("[project]\nrequires-python = '>={version}'\n"),
        )
        .unwrap();
        let valid = project.path().join("main.py");
        fs::write(&valid, "\"\"\"Existing input fixture.\"\"\"\n").unwrap();
        let missing = project.path().join("missing.py");
        let second = project.path().join("nested/second.py");
        let report = run(&QualityRequest::new(
            QualityOperation::Check,
            vec![valid.clone(), missing.clone(), second.clone()],
        ));
        assert_eq!(report.exit_code, 2);
        assert!(!report.analysis_completed && !report.passed);
        assert_eq!(report.issues.len(), 2);
        assert!(report.issues.iter().all(|issue| issue.code == "path-read"));
        let pylint = report
            .tools
            .iter()
            .find(|tool| tool.tool.name() == "pylint")
            .unwrap();
        assert_eq!(
            pylint.completion,
            mamba_quality::application::quality::CompletionStatus::Error
        );
        assert_eq!(pylint.checked_files, 1);
        assert_eq!(pylint.diagnostics.len(), 2);
        for (diagnostic, path, name) in [
            (&pylint.diagnostics[0], missing, "missing.py"),
            (&pylint.diagnostics[1], second, "second.py"),
        ] {
            assert_eq!(diagnostic.rule, "F0001");
            assert_eq!(diagnostic.path, path);
            assert_eq!(diagnostic.message, format!("No module named {name}"));
            assert_eq!(diagnostic.severity, "fatal");
            assert_eq!(
                (diagnostic.location.line, diagnostic.location.column),
                (1, 0)
            );
            assert!(
                diagnostic.location.end_line.is_none() && diagnostic.location.end_column.is_none()
            );
        }
        assert_eq!(
            fs::read_to_string(valid).unwrap(),
            "\"\"\"Existing input fixture.\"\"\"\n"
        );
    }
}

#[test]
fn reports_fixed_profile_without_loading_python_or_tool_settings() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.13.1'\n[tool.ruff]\nselect = ['ALL']\n[tool.black]\nline-length = 120\n").unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Info,
        vec![project.path().into()],
    ));
    assert_eq!(report.exit_code, 0);
    assert_eq!(report.schema, "mamba.quality.v2");
    assert_eq!(report.projects[0].python_version.name(), "3.13");
    assert_eq!(report.profile.line_width, 88);
    assert_eq!(report.profile.ruff.rules.len(), 174);
    assert_eq!(report.profile.mypy.strict_flags.len(), 13);
    assert_eq!(report.profile.pylint.rules.len(), 383);
    assert_eq!(report.files.discovered, 0);
}

#[test]
fn discovery_respects_gitignore_but_explicit_files_remain_visible() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join(".gitignore"), "ignored.py\n").unwrap();
    fs::write(project.path().join("ignored.py"), "value = 1\n").unwrap();
    fs::write(project.path().join("visible.pyi"), "value: int\n").unwrap();
    fs::create_dir(project.path().join(".venv")).unwrap();
    fs::write(project.path().join(".venv/vendor.py"), "value = 2\n").unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Check,
        vec![project.path().into()],
    ));
    assert_eq!(report.files.discovered, 1);
    let explicit = run(&QualityRequest::new(
        QualityOperation::Check,
        vec![
            project.path().join("ignored.py"),
            project.path().join(".venv/vendor.py"),
        ],
    ));
    assert_eq!(explicit.files.discovered, 2);
}

#[test]
fn rejects_unsupported_minimum_and_never_claims_partial_checks_passed() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.8'\n",
    )
    .unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Info,
        vec![project.path().into()],
    ));
    assert_eq!(report.exit_code, 2);
    assert!(!report.passed);
    assert_eq!(report.issues[0].code, "unsupported-python-version");
    fs::write(
        project.path().join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.12'\n",
    )
    .unwrap();
    fs::write(project.path().join("main.py"), "value: int = 1\n").unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Check,
        vec![project.path().into()],
    ));
    assert_eq!(report.exit_code, 2);
    assert!(!report.passed);
    assert!(!report.analysis_completed);
    assert_eq!(
        fs::read_to_string(project.path().join("main.py")).unwrap(),
        "value: int = 1\n"
    );
}

#[test]
fn invalid_encoding_is_counted_and_ruff_retains_its_original_io_diagnostic() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("bad.py"), b"label = '\xff'\n").unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Check,
        vec![project.path().into()],
    ));
    assert_eq!(report.files.discovered, 1);
    assert_eq!(report.exit_code, 2);
    assert_eq!(report.issues[0].code, "source-encoding");
    let ruff = report
        .tools
        .iter()
        .find(|tool| tool.tool.name() == "ruff")
        .unwrap();
    assert_eq!(ruff.reference_exit_code, Some(1));
    assert_eq!(ruff.diagnostics[0].rule, "E902");
    assert_eq!(
        ruff.diagnostics[0].message,
        "stream did not contain valid UTF-8"
    );
}

#[test]
fn never_publishes_a_candidate_from_an_incomplete_formatter() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("main.py");
    let original = "VALUE='one'\n";
    for operation in [
        QualityOperation::Format {
            check: false,
            diff: false,
        },
        QualityOperation::Fix { diff: false },
    ] {
        fs::write(&path, original).unwrap();
        let report = run(&QualityRequest::new(operation, vec![path.clone()]));
        assert_eq!(report.exit_code, 2);
        assert_eq!(report.files.changed, 1);
        assert_eq!(report.files.written, 0);
        assert!(!report.changes[0].applied);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}
