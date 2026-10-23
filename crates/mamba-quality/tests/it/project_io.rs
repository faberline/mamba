//! Permission errors stay explicit and never change project source files.
#![cfg(unix)]
use mamba_quality::{run, QualityOperation, QualityRequest};
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn source_permission_failure_retains_fatal_and_error_exit() {
    for version in ["3.12", "3.13", "3.14"] {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join("pyproject.toml"),
            format!("[project]\nrequires-python = '>= {version}'\n"),
        )
        .unwrap();
        let path = project.path().join("main.py");
        let original = "\"\"\"Permission fixture.\"\"\"\n";
        fs::write(&path, original).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
        let outcome = std::panic::catch_unwind(|| {
            run(&QualityRequest::new(
                QualityOperation::Check,
                vec![path.clone()],
            ))
        });
        fs::set_permissions(&path, permissions).unwrap();
        let report = outcome.unwrap();
        assert_eq!(report.exit_code, 2);
        assert!(!report.analysis_completed && !report.passed);
        assert_eq!(report.files.discovered, 1);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].code, "source-read");
        let pylint = report
            .tools
            .iter()
            .find(|tool| tool.tool.name() == "pylint")
            .unwrap();
        assert_eq!(
            pylint.completion,
            mamba_quality::application::quality::CompletionStatus::Error
        );
        assert_eq!(pylint.checked_files, 0);
        assert_eq!(pylint.reference_exit_code, None);
        assert_eq!(pylint.diagnostics.len(), 1);
        let diagnostic = &pylint.diagnostics[0];
        assert_eq!(diagnostic.rule, "F0010");
        assert_eq!(diagnostic.severity, "fatal");
        assert_eq!(diagnostic.path, path.canonicalize().unwrap());
        assert_eq!(
            (diagnostic.location.line, diagnostic.location.column),
            (1, 0)
        );
        assert!(diagnostic.location.end_line.is_none() && diagnostic.location.end_column.is_none());
        assert!(diagnostic
            .message
            .starts_with("error while code parsing: Unable to load file "));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}
