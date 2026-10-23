//! Fault injection checks the write gate independently of incomplete engines.
mod candidates;
use super::*;
use crate::domain::source::SourceSnapshot;
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};
use std::{cell::RefCell, fs};

struct Engine {
    transforms: RefCell<Vec<QualityTool>>,
    failed_check: Option<CompletionStatus>,
    diagnostic: bool,
}

impl Engine {
    fn new(failed_check: Option<CompletionStatus>, diagnostic: bool) -> Self {
        Self {
            transforms: RefCell::new(Vec::new()),
            failed_check,
            diagnostic,
        }
    }
}

impl QualityEngine for Engine {
    fn check(
        &self,
        workspace: &QualityWorkspace,
        _: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        ToolReport {
            tool,
            reference_version: tool.reference_version().into(),
            completion: if tool == QualityTool::Black {
                self.failed_check.unwrap_or(CompletionStatus::Complete)
            } else {
                CompletionStatus::Complete
            },
            reference_exit_code: Some(0),
            checked_files: workspace.sources.len(),
            diagnostics: if self.diagnostic && tool == QualityTool::Mypy {
                vec![QualityDiagnostic {
                    tool,
                    rule: "assignment".into(),
                    message: "Unrelated type error remains".into(),
                    hint: None,
                    path: workspace.sources[0].path.clone(),
                    location: QualityLocation {
                        line: 1,
                        column: 0,
                        end_line: None,
                        end_column: None,
                        offset: None,
                        end_offset: None,
                    },
                    severity: "error".into(),
                    fix: None,
                }]
            } else {
                Vec::new()
            },
            remaining: Vec::new(),
        }
    }

    fn transform(
        &self,
        workspace: &QualityWorkspace,
        _: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult {
        self.transforms.borrow_mut().push(tool);
        TransformResult {
            completion: CompletionStatus::Complete,
            candidates: if tool == QualityTool::Black {
                workspace
                    .sources
                    .iter()
                    .enumerate()
                    .map(|(index, source)| (index, source.text.replace("VALUE=1", "VALUE = 1")))
                    .collect()
            } else {
                Vec::new()
            },
            issues: Vec::new(),
            remaining: Vec::new(),
        }
    }

    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        equivalent: bool,
    ) -> Result<(), QualityIssue> {
        NativeEngine.validate(original, candidate, workspace, equivalent)
    }
}

#[test]
fn final_execution_failure_keeps_valid_candidate_unpublished() {
    for status in [
        CompletionStatus::Error,
        CompletionStatus::Unsupported,
        CompletionStatus::Incomplete,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.py");
        fs::write(&path, "VALUE=1\n").unwrap();
        let engine = Engine::new(Some(status), false);
        let report = execute(
            &QualityRequest::new(QualityOperation::Fix { diff: false }, vec![path.clone()]),
            &Filesystem,
            &engine,
        );
        assert_eq!(report.exit_code, 2);
        assert_eq!(report.files.changed, 1);
        assert_eq!(report.files.written, 0);
        assert!(!report.changes[0].applied);
        assert_eq!(fs::read_to_string(path).unwrap(), "VALUE=1\n");
    }
}

#[test]
fn quality_diagnostics_allow_safe_fix_and_second_run_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, "VALUE=1\n").unwrap();
    let engine = Engine::new(None, true);
    let request = QualityRequest::new(QualityOperation::Fix { diff: false }, vec![path.clone()]);
    let first = execute(&request, &Filesystem, &engine);
    assert_eq!(first.exit_code, 1);
    assert!(first.analysis_completed && !first.passed);
    assert_eq!(first.files.written, 1);
    assert!(first.changes[0].applied);
    assert_eq!(fs::read_to_string(path).unwrap(), "VALUE = 1\n");
    let second = execute(&request, &Filesystem, &engine);
    assert_eq!(second.exit_code, 1);
    assert_eq!(second.files.changed, 0);
    assert_eq!(second.files.written, 0);
    let tools = [QualityTool::Ruff, QualityTool::Isort, QualityTool::Black];
    assert_eq!(*engine.transforms.borrow(), tools.repeat(3));
}

#[cfg(unix)]
mod publication;
