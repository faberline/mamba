//! Black style on shared Rust layout machinery. Reference deviations stay visible.
mod comments;
mod equivalence;
mod errors;
mod formfeeds;
mod layout;
mod literals;
mod policy;
mod type_comments;
pub(super) use equivalence::equivalent;
#[cfg(test)]
mod tests;
use crate::domain::{
    model::*,
    source::{QualityWorkspace, SourceSnapshot},
};
use mamba_language::application::quality_syntax;

fn format(source: &SourceSnapshot, workspace: &QualityWorkspace) -> Result<String, QualityIssue> {
    format_with_logs(
        source,
        workspace,
        &super::failure_logs::FailureLogs::new(false),
    )
}

fn format_with_logs(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    logs: &super::failure_logs::FailureLogs,
) -> Result<String, QualityIssue> {
    if let Some(issue) = &source.decoding_issue {
        return Err(issue.clone());
    }
    let target = super::native::target(workspace, source);
    let document = quality_syntax::parse(&source.text, target, source.is_stub)
        .map_err(|message| QualityIssue::new("black-syntax", message, Some(source.path.clone())))?;
    if let Some(error) = errors::tokenization(source, &document) {
        return Err(error);
    }
    // Black's 3.10+ grammar is shared across the three fixed targets. Its AST
    // safety check runs only when formatting changes the source.
    if document
        .parsed
        .errors()
        .iter()
        .any(|error| !mamba_black_layout::is_permissive_target_error(&error.error))
        && !mamba_black_layout::supports_permissive_syntax(
            &source.text,
            if source.is_stub {
                ruff_python_ast::PySourceType::Stub
            } else {
                ruff_python_ast::PySourceType::Python
            },
        )
    {
        return Err(errors::syntax(source, &document));
    }
    let mut candidate = layout::format(source, target)?;
    if candidate != source.text {
        let mut after = quality_syntax::parse(&candidate, document.target, source.is_stub)
            .map_err(|message| {
                QualityIssue::new("black-syntax", message, Some(source.path.clone()))
            })?;
        if !after.is_valid() {
            let line_feed_candidate = candidate.replace("\r\n", "\n");
            if line_feed_candidate != candidate {
                candidate = line_feed_candidate;
                after = quality_syntax::parse(&candidate, document.target, source.is_stub)
                    .map_err(|message| {
                        QualityIssue::new("black-syntax", message, Some(source.path.clone()))
                    })?;
            }
        }
        if !after.is_valid() {
            let source_type = if source.is_stub {
                ruff_python_ast::PySourceType::Stub
            } else {
                ruff_python_ast::PySourceType::Python
            };
            if !mamba_black_layout::supports_permissive_syntax(&candidate, source_type) {
                return Err(errors::formatted::error(
                    source, &candidate, target, &after, &document, workspace, logs,
                ));
            }
            return Err(errors::source_ast(source, &document));
        }
        if !document.is_valid() {
            return Err(errors::source_ast(source, &document));
        }
        if !equivalent(&document, &after) {
            return Err(errors::safety(source, &candidate, target, workspace, logs));
        }
    }
    Ok(candidate)
}

fn remaining() -> Vec<String> {
    vec!["Black full stable-style parity is incomplete: exact layout, literal normalization, comments, directives, and interpolation behavior require original-corpus acceptance".into()]
}

pub fn check(workspace: &QualityWorkspace) -> ToolReport {
    check_with_logs(workspace, &super::failure_logs::FailureLogs::new(false))
}

pub(super) fn check_with_logs(
    workspace: &QualityWorkspace,
    logs: &super::failure_logs::FailureLogs,
) -> ToolReport {
    let mut report = ToolReport {
        tool: QualityTool::Black,
        reference_version: "26.10.0".into(),
        completion: CompletionStatus::Incomplete,
        reference_exit_code: None,
        checked_files: 0,
        diagnostics: Vec::new(),
        remaining: remaining(),
    };
    for source in &workspace.sources {
        match format_with_logs(source, workspace, logs) {
            Ok(candidate) => {
                report.checked_files += 1;
                if candidate != source.text {
                    report.diagnostics.push(QualityDiagnostic {
                        tool: QualityTool::Black,
                        rule: "format".into(),
                        message: format!("would reformat {}", source.path.display()),
                        hint: None,
                        path: source.path.clone(),
                        severity: "error".into(),
                        location: QualityLocation {
                            line: 1,
                            column: 1,
                            end_line: None,
                            end_column: None,
                            offset: Some(0),
                            end_offset: None,
                        },
                        fix: None,
                    });
                }
            }
            Err(error) => {
                report.completion = CompletionStatus::Error;
                report.diagnostics.push(errors::diagnostic(source, &error));
            }
        }
    }
    report
}

pub fn transform(workspace: &QualityWorkspace) -> TransformResult {
    let mut result = TransformResult {
        completion: CompletionStatus::Incomplete,
        candidates: Vec::new(),
        issues: Vec::new(),
        remaining: remaining(),
    };
    for (index, source) in workspace.sources.iter().enumerate() {
        match format(source, workspace) {
            Ok(candidate) if candidate != source.text => result.candidates.push((index, candidate)),
            Ok(_) => {}
            Err(error) => {
                result.completion = CompletionStatus::Error;
                result.issues.push(error);
            }
        }
    }
    result
}
