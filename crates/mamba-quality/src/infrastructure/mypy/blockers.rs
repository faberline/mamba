//! Blocking parse errors stop semantic analysis and keep original point positions.
use super::dependencies::Loaded;
use crate::domain::model::{QualityDiagnostic, QualityLocation, QualityTool};
use mamba_language::application::quality_syntax::{self, SyntaxDocument};
use std::collections::BTreeSet;

pub(super) fn syntax(loaded: &Loaded) -> Vec<QualityDiagnostic> {
    let mut diagnostics = Vec::new();
    for &id in &loaded.parse_order {
        let source = &loaded.workspace.sources[id];
        if source.decoding_issue.is_some() {
            continue;
        }
        let Some(document) = loaded.document(id) else {
            continue;
        };
        if document.is_valid() || document.has_only_type_inference_version_errors() {
            continue;
        }
        let mut seen = BTreeSet::new();
        let mut issues = Vec::new();
        for issue in &document.issues {
            let (line, column) = document.line_column(issue.start);
            let (message, column) = if issue.version_error {
                match version_message(&document, issue) {
                    Some(message) => (message, column),
                    None => continue,
                }
            } else {
                (capitalize(&issue.message), column + 1)
            };
            if !seen.insert((line, message.clone())) {
                continue;
            }
            issues.push((
                line,
                column,
                QualityDiagnostic {
                    tool: QualityTool::Mypy,
                    rule: "syntax".into(),
                    message,
                    hint: None,
                    path: source.path.clone(),
                    location: QualityLocation {
                        line,
                        column,
                        end_line: Some(line),
                        end_column: Some(i64::from(column)),
                        offset: None,
                        end_offset: None,
                    },
                    severity: "error".into(),
                    fix: None,
                },
            ));
        }
        issues.sort_by_key(|(line, column, _)| (*line, *column));
        diagnostics.extend(issues.into_iter().map(|(_, _, diagnostic)| diagnostic));
        if !diagnostics.is_empty() {
            return diagnostics;
        }
    }
    diagnostics
}

fn capitalize(message: &str) -> String {
    let Some((offset, first)) = message.char_indices().find(|(_, ch)| !ch.is_whitespace()) else {
        return message.into();
    };
    format!(
        "{}{}{}",
        &message[..offset],
        first.to_uppercase(),
        &message[offset + first.len_utf8()..]
    )
}

fn version_message(
    document: &SyntaxDocument<'_>,
    issue: &quality_syntax::SyntaxIssue,
) -> Option<String> {
    let kind = document
        .parsed
        .unsupported_syntax_errors()
        .iter()
        .find(|error| error.range.start().to_u32() == issue.start)?
        .kind;
    use quality_syntax::UnsupportedSyntaxErrorKind;
    let (feature, minimum) = match kind {
        UnsupportedSyntaxErrorKind::TemplateStrings => ("T-strings", "3.14"),
        UnsupportedSyntaxErrorKind::TypeParamDefault => ("Type parameter defaults", "3.13"),
        _ => return Some(capitalize(&issue.message)),
    };
    Some(format!(
        "{feature}: requires Python {minimum} or newer (current target: Python {})",
        document.target.native()
    ))
}
