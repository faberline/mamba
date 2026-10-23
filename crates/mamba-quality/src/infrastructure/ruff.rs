use crate::domain::{
    model::*,
    profile::FixedProfile,
    source::{ProjectInfo, QualityWorkspace, SourceSnapshot},
};
use ruff_db::diagnostic::Diagnostic;
use ruff_diagnostics::Applicability;
use ruff_linter::{
    linter::{lint_fix, lint_only, ParseSource},
    package::PackageRoot,
    registry::Rule,
    settings::{flags::Noqa, types::UnsafeFixes, LinterSettings},
    source_kind::SourceKind,
};
use ruff_python_ast::{PySourceType, PythonVersion};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextSize};
use std::path::Path;
#[cfg(test)]
mod tests;

fn settings(project: &ProjectInfo, profile: &FixedProfile) -> LinterSettings {
    let mut settings = LinterSettings::new(&project.root);
    settings.rules = profile
        .ruff
        .rules
        .iter()
        .map(|rule| {
            Rule::from_code(&rule.code)
                .expect("the fixed rule catalog matches the pinned Rust core")
        })
        .collect();
    settings.src = vec![project.root.clone(), project.root.join("src")];
    settings.unresolved_target_version = match project.python_version.minor() {
        12 => PythonVersion::PY312,
        13 => PythonVersion::PY313,
        _ => PythonVersion::PY314,
    }
    .into();
    settings
}

fn package(path: &Path) -> Option<PackageRoot<'_>> {
    let parent = path.parent()?;
    let mut root = None;
    for ancestor in parent.ancestors() {
        if !ancestor.join("__init__.py").is_file() {
            break;
        }
        root = Some(ancestor);
    }
    root.map(PackageRoot::root)
}

fn source_kind(source: &SourceSnapshot, text: &str) -> SourceKind {
    SourceKind::Python {
        code: text.into(),
        is_stub: source.is_stub,
    }
}

fn source_type(source: &SourceSnapshot) -> PySourceType {
    if source.is_stub {
        PySourceType::Stub
    } else {
        PySourceType::Python
    }
}

fn line_column(source: &str, index: &LineIndex, offset: u32) -> (u32, u32) {
    let position = index.line_column(TextSize::from(offset), source);
    (position.line.get() as u32, position.column.get() as u32)
}

fn convert(
    diagnostic: &Diagnostic,
    source: &SourceSnapshot,
    text: &str,
    index: &LineIndex,
) -> QualityDiagnostic {
    let range = diagnostic.range();
    let start = range.map(|range| range.start().to_u32());
    let end = range.map(|range| range.end().to_u32());
    let (line, column) = line_column(text, index, start.unwrap_or(0));
    let end_location = end.map(|offset| line_column(text, index, offset));
    let fix = diagnostic.fix().map(|fix| QualityFix {
        applicability: match fix.applicability() {
            Applicability::Safe => "safe",
            Applicability::Unsafe => "unsafe",
            Applicability::DisplayOnly => "displayonly",
        }
        .into(),
        message: diagnostic.first_help_text().map(str::to_owned),
        edits: fix
            .edits()
            .iter()
            .map(|edit| QualityEdit {
                start: edit.range().start().to_u32(),
                end: edit.range().end().to_u32(),
                replacement: edit.content().unwrap_or_default().into(),
            })
            .collect(),
    });
    QualityDiagnostic {
        tool: QualityTool::Ruff,
        rule: diagnostic.secondary_code_or_id().into(),
        message: diagnostic.concise_message().to_string(),
        hint: None,
        path: source.path.clone(),
        severity: "error".into(),
        location: QualityLocation {
            line,
            column,
            end_line: end_location.map(|position| position.0),
            end_column: end_location.map(|position| i64::from(position.1)),
            offset: start,
            end_offset: end,
        },
        fix,
    }
}

pub fn check(workspace: &QualityWorkspace, profile: &FixedProfile) -> ToolReport {
    let mut report = ToolReport {
        tool: QualityTool::Ruff,
        reference_version: QualityTool::Ruff.reference_version().into(),
        completion: CompletionStatus::Complete,
        reference_exit_code: Some(0),
        checked_files: workspace.sources.len(),
        diagnostics: Vec::new(),
        remaining: Vec::new(),
    };
    let mut pending = Vec::new();
    // Ruff reads UTF-8 bytes independently of a Python coding declaration.
    // Other engines use the Python-decoded snapshot instead.
    let texts: Vec<_> = workspace
        .sources
        .iter()
        .map(|source| {
            std::str::from_utf8(&source.bytes)
                .ok()
                .map(|text| text.strip_prefix('\u{feff}').unwrap_or(text))
        })
        .collect();
    let indexes: Vec<_> = texts
        .iter()
        .map(|text| text.map(LineIndex::from_source_text))
        .collect();
    for (source_index, source) in workspace.sources.iter().enumerate() {
        let Some(text) = texts[source_index] else {
            pending.push((source_index, None));
            continue;
        };
        let result = lint_only(
            &source.path,
            package(&source.path),
            &settings(&workspace.projects[source.project], profile),
            Noqa::Enabled,
            &source_kind(source, text),
            source_type(source),
            ParseSource::None,
        );
        pending.extend(
            result
                .diagnostics
                .into_iter()
                .map(|diagnostic| (source_index, Some(diagnostic))),
        );
    }
    // Match Ruff's stable command output. Equal starts keep the checker's
    // insertion order; rule-code or name sorting changes observable output.
    pending.sort_by(
        |(left_index, left), (right_index, right)| match (left, right) {
            (Some(left), Some(right)) => left.ruff_start_ordering(right),
            _ => {
                let left_fatal = left
                    .as_ref()
                    .is_some_and(|diagnostic| diagnostic.severity().is_fatal());
                let right_fatal = right
                    .as_ref()
                    .is_some_and(|diagnostic| diagnostic.severity().is_fatal());
                left_fatal.cmp(&right_fatal).then(
                    workspace.sources[*left_index]
                        .path
                        .cmp(&workspace.sources[*right_index].path),
                )
            }
        },
    );
    for (source_index, diagnostic) in pending {
        let source = &workspace.sources[source_index];
        report.diagnostics.push(match diagnostic {
            Some(diagnostic) => convert(
                &diagnostic,
                source,
                texts[source_index].unwrap(),
                indexes[source_index].as_ref().unwrap(),
            ),
            None => QualityDiagnostic {
                tool: QualityTool::Ruff,
                rule: "E902".into(),
                message: "stream did not contain valid UTF-8".into(),
                hint: None,
                path: source.path.clone(),
                severity: "error".into(),
                location: QualityLocation {
                    line: 1,
                    column: 1,
                    end_line: Some(1),
                    end_column: Some(1),
                    offset: None,
                    end_offset: None,
                },
                fix: None,
            },
        });
    }
    if !report.diagnostics.is_empty() {
        report.reference_exit_code = Some(1);
    }
    for issue in workspace
        .issues
        .iter()
        .filter(|issue| issue.code != "source-encoding")
    {
        report.completion = CompletionStatus::Error;
        report.reference_exit_code = None;
        report
            .remaining
            .push(format!("{}: {}", issue.code, issue.message));
    }
    report
}

pub fn transform(workspace: &QualityWorkspace, profile: &FixedProfile) -> TransformResult {
    let mut result = TransformResult {
        completion: CompletionStatus::Complete,
        candidates: Vec::new(),
        issues: Vec::new(),
        remaining: Vec::new(),
    };
    for (index, source) in workspace.sources.iter().enumerate() {
        let text = match std::str::from_utf8(&source.bytes) {
            Ok(text) => text.strip_prefix('\u{feff}').unwrap_or(text),
            Err(_) => {
                result.completion = CompletionStatus::Error;
                result.issues.push(QualityIssue::new(
                    "ruff-encoding",
                    "Ruff requires valid UTF-8 source bytes",
                    Some(source.path.clone()),
                ));
                continue;
            }
        };
        let source_kind = source_kind(source, text);
        match lint_fix(
            &source.path,
            package(&source.path),
            Noqa::Enabled,
            UnsafeFixes::Disabled,
            &settings(&workspace.projects[source.project], profile),
            &source_kind,
            source_type(source),
        ) {
            Ok(fixed) => {
                let candidate = fixed.transformed.source_code();
                let mut bytes = Vec::new();
                if source.bytes.starts_with(b"\xef\xbb\xbf") {
                    bytes.extend_from_slice(b"\xef\xbb\xbf");
                }
                bytes.extend_from_slice(candidate.as_bytes());
                // A successful byte-identical Ruff result needs no decoded candidate.
                if bytes == source.bytes {
                    continue;
                }
                match super::encoding::decode(&source.path, &bytes) {
                    Ok((candidate, _)) if candidate != source.text => {
                        result.candidates.push((index, candidate))
                    }
                    Ok(_) => {}
                    Err(error) => {
                        result.completion = CompletionStatus::Error;
                        result.issues.push(error);
                    }
                }
            }
            Err(error) => {
                result.completion = CompletionStatus::Error;
                result.issues.push(QualityIssue::new(
                    "ruff-fix",
                    error.to_string(),
                    Some(source.path.clone()),
                ));
            }
        }
    }
    result
}
