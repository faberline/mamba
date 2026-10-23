use crate::domain::{
    model::*,
    ports::QualityEngine,
    profile::FixedProfile,
    source::{QualityWorkspace, SourceSnapshot},
};
use mamba_language::application::quality_syntax::{self, PythonTarget};

pub struct NativeEngine;

/// One immutable log policy for one application request.
pub(crate) struct RequestEngine {
    logs: super::failure_logs::FailureLogs,
}

impl NativeEngine {
    pub(crate) fn for_request(operation: QualityOperation) -> RequestEngine {
        RequestEngine {
            logs: super::failure_logs::FailureLogs::new(operation == QualityOperation::Check),
        }
    }
}

impl QualityEngine for RequestEngine {
    fn remaining(&self, tool: QualityTool) -> Vec<String> {
        NativeEngine.remaining(tool)
    }

    fn check(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        match tool {
            QualityTool::Black => super::black::check_with_logs(workspace, &self.logs),
            QualityTool::Pylint => super::pylint::check_with_logs(workspace, &self.logs),
            _ => NativeEngine.check(workspace, profile, tool),
        }
    }

    fn transform(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult {
        NativeEngine.transform(workspace, profile, tool)
    }

    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        structure_equivalent: bool,
    ) -> Result<(), QualityIssue> {
        NativeEngine.validate(original, candidate, workspace, structure_equivalent)
    }
}

pub fn target(workspace: &QualityWorkspace, source: &SourceSnapshot) -> PythonTarget {
    match workspace.projects[source.project].python_version.minor() {
        12 => PythonTarget::Py312,
        13 => PythonTarget::Py313,
        _ => PythonTarget::Py314,
    }
}

fn incomplete(tool: QualityTool) -> Vec<String> {
    vec![format!(
        "{} native reference parity is not complete",
        tool.name()
    )]
}

impl QualityEngine for NativeEngine {
    fn remaining(&self, tool: QualityTool) -> Vec<String> {
        match tool {
            QualityTool::Ruff => Vec::new(),
            QualityTool::Isort => {
                vec!["isort complete Black-profile byte parity is incomplete".into()]
            }
            QualityTool::Black => {
                vec!["Black complete stable-style byte parity is incomplete".into()]
            }
            QualityTool::Mypy => {
                vec!["mypy full strict inference and Pydantic model parity are incomplete".into()]
            }
            QualityTool::Pylint => {
                vec!["Pylint full default-check and inference parity are incomplete".into()]
            }
        }
    }

    fn check(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        if tool == QualityTool::Ruff {
            return super::ruff::check(workspace, profile);
        }
        if tool == QualityTool::Isort {
            return super::isort::check(workspace, profile);
        }
        if tool == QualityTool::Black {
            return super::black::check(workspace);
        }
        if tool == QualityTool::Mypy {
            return super::mypy::check(workspace);
        }
        if tool == QualityTool::Pylint {
            return super::pylint::check(workspace);
        }
        ToolReport {
            tool,
            reference_version: tool.reference_version().into(),
            completion: CompletionStatus::Incomplete,
            reference_exit_code: None,
            checked_files: 0,
            diagnostics: Vec::new(),
            remaining: incomplete(tool),
        }
    }

    fn transform(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult {
        if tool == QualityTool::Ruff {
            return super::ruff::transform(workspace, profile);
        }
        if tool == QualityTool::Isort {
            return super::isort::transform(workspace, profile);
        }
        if tool == QualityTool::Black {
            return super::black::transform(workspace);
        }
        TransformResult {
            completion: CompletionStatus::Incomplete,
            candidates: Vec::new(),
            issues: Vec::new(),
            remaining: incomplete(tool),
        }
    }

    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        structure_equivalent: bool,
    ) -> Result<(), QualityIssue> {
        if let Some(issue) = &original.decoding_issue {
            return Err(issue.clone());
        }
        let version = target(workspace, original);
        let parsed =
            quality_syntax::parse(candidate, version, original.is_stub).map_err(|error| {
                QualityIssue::new("candidate-syntax", error, Some(original.path.clone()))
            })?;
        if !parsed.is_valid() {
            return Err(QualityIssue::new(
                "candidate-syntax",
                parsed
                    .issues
                    .iter()
                    .map(|issue| issue.message.clone())
                    .collect::<Vec<_>>()
                    .join("; "),
                Some(original.path.clone()),
            ));
        }
        if structure_equivalent {
            let before = quality_syntax::parse(&original.text, version, original.is_stub).map_err(
                |error| QualityIssue::new("original-syntax", error, Some(original.path.clone())),
            )?;
            if !super::black::equivalent(&before, &parsed) {
                return Err(QualityIssue::new(
                    "candidate-structure",
                    "Formatting changed the program structure",
                    Some(original.path.clone()),
                ));
            }
        }
        Ok(())
    }
}
