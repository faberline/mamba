//! Original Black error classes, text, and source positions.
use crate::domain::{model::*, source::SourceSnapshot};
use mamba_language::application::quality_syntax::{PythonTarget, SyntaxDocument};
mod ast;
pub(super) mod formatted;
mod mixed_literals;
mod safety;
mod targets;
pub(super) use safety::error as safety;

const INVALID_INPUT: &str = "black.parsing.InvalidInput";
const SOURCE_AST: &str = "black.parsing.SourceASTParseError";

fn python_name(target: PythonTarget) -> &'static str {
    match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }
}

pub(super) fn syntax(source: &SourceSnapshot, document: &SyntaxDocument<'_>) -> QualityIssue {
    if document
        .issues
        .iter()
        .any(|issue| issue.message == "unindent does not match any outer indentation level")
    {
        return source_ast(source, document);
    }
    let issue = document
        .issues
        .iter()
        .find(|issue| {
            !issue.version_error
                && !targets::is_context_error(&issue.message)
                && !mixed_literals::is_mixed_literal_issue(&issue.message)
        })
        .or_else(|| {
            document
                .issues
                .iter()
                .find(|issue| !issue.version_error && !targets::is_context_error(&issue.message))
        })
        .or_else(|| document.issues.iter().find(|issue| !issue.version_error))
        .unwrap();
    let interpolation = document.issues.iter().any(|issue| {
        issue.message.contains("expecting `}`") || issue.message.contains("unterminated string")
    });
    let eof = issue.message.contains("EOF");
    let ordinary_string = issue.message.contains("missing closing quote");
    let triple_interpolation = interpolation
        && document.tokens.iter().any(|token| {
            matches!(token.kind.as_str(), "FStringStart" | "TStringStart")
                && (document.text(token).ends_with("\"\"\"")
                    || document.text(token).ends_with("'''"))
        });
    let empty_body = issue.message.contains("Expected an indented block")
        && source.text[issue.end as usize..].trim().is_empty();
    let continuation =
        issue.message.contains("after line continuation") && source.text.trim_end().ends_with('\\');
    let offset = if triple_interpolation || empty_body || continuation {
        source.text.len() as u32
    } else if eof {
        source.text.strip_suffix('\n').unwrap_or(&source.text).len() as u32
    } else if interpolation {
        document
            .tokens
            .iter()
            .rfind(|token| token.kind == "Lbrace" && token.start < issue.start)
            .map_or(issue.start, |token| token.end)
    } else if ordinary_string {
        issue.start.saturating_sub(1)
    } else {
        issue.start
    };
    let (mut line, mut column) = document.line_column(offset);
    column = column.saturating_sub(1);
    if continuation {
        line += if source.text.ends_with('\n') { 1 } else { 2 };
        column = 0;
    }
    let details = if triple_interpolation || empty_body || continuation {
        "ParseError: bad input"
    } else if ordinary_string
        && (source.text[issue.start as usize..].starts_with("\"\"\"")
            || source.text[issue.start as usize..].starts_with("'''"))
    {
        "TokenError: Unexpected EOF in multi-line statement"
    } else if interpolation || ordinary_string {
        "TokenError: Failed to parse: UnterminatedString"
    } else if eof {
        "TokenError: Unexpected EOF in multi-line statement"
    } else {
        "ParseError: bad input"
    };
    invalid_input(source, document.target, (line, column), details)
}

fn invalid_input(
    source: &SourceSnapshot,
    target: PythonTarget,
    (line, column): (u32, u32),
    details: &str,
) -> QualityIssue {
    let faulty = source
        .text
        .lines()
        .nth(line as usize - 1)
        .unwrap_or("<line number missing in source>");
    QualityIssue::new(
        INVALID_INPUT,
        format!(
            "cannot parse for target version Python {}: {line}:{column}\n    {faulty}\n    {}^\n{details}",
            python_name(target),
            " ".repeat(column.saturating_sub(1) as usize)
        ),
        Some(source.path.clone()),
    )
}

pub(super) fn source_ast(source: &SourceSnapshot, document: &SyntaxDocument<'_>) -> QualityIssue {
    let ast_error = ast::error(document);
    if let Some(issue) = mixed_literals::source_ast(source, document, ast_error.as_ref()) {
        return issue;
    }
    if let Some((reason, offset)) = ast_error {
        return source_ast_message(source, &reason, document.line_column(offset).0);
    }
    let issue = document
        .issues
        .iter()
        .find(|issue| !issue.version_error)
        .or_else(|| document.issues.first())
        .unwrap();
    let line = document.line_column(issue.start).0;
    let reason = if document.target != PythonTarget::Py314
        && document
            .issues
            .iter()
            .any(|issue| issue.version_error && issue.message.contains("t-strings"))
    {
        "invalid syntax".into()
    } else if issue.message.contains("invalid conversion character") {
        let kind = if issue.message.starts_with("t-string:") {
            "t-string"
        } else {
            "f-string"
        };
        let flag = source.text[issue.start as usize..]
            .chars()
            .next()
            .unwrap_or('?');
        format!("{kind}: invalid conversion character '{flag}': expected 's', 'r', or 'a'")
    } else if issue.message.contains("single `}`") {
        issue.message.replace("`}`", "'}'")
    } else if issue.message == "Unexpected indentation" {
        "unexpected indent".into()
    } else if issue.message == "unindent does not match any outer indentation level" {
        issue.message.clone()
    } else {
        "invalid syntax".into()
    };
    source_ast_message(source, &reason, line)
}

fn source_ast_message(source: &SourceSnapshot, reason: &str, line: u32) -> QualityIssue {
    QualityIssue::new(
        SOURCE_AST,
        format!(
            "cannot use --safe with this file; failed to parse source file AST: {reason} (<unknown>, line {line})\nThis could be caused by running Black with an older Python version that does not support new syntax used in your source file."
        ),
        Some(source.path.clone()),
    )
}

pub(super) fn tokenization(
    source: &SourceSnapshot,
    document: &SyntaxDocument<'_>,
) -> Option<QualityIssue> {
    let position = mixed_indentation(document)?;
    Some(invalid_input(
        source,
        document.target,
        position,
        "TokenError: Failed to parse: InconsistentUseOfTabsAndSpaces",
    ))
}

fn mixed_indentation(document: &SyntaxDocument<'_>) -> Option<(u32, u32)> {
    if !document.issues.iter().any(|issue| {
        issue.message == "Unexpected indentation" || issue.message.contains("indentation level")
    }) {
        return None;
    }
    let mut previous = None;
    for (line, text) in document.source.lines().enumerate() {
        let indent = text
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .collect::<String>();
        if indent.is_empty() || text.trim_start().starts_with('#') || text.trim().is_empty() {
            continue;
        }
        let tabs = indent.contains('\t');
        if let Some((old_tabs, old_line, old_len)) = previous {
            if old_tabs != tabs {
                return Some((old_line, old_len));
            }
        }
        previous = Some((tabs, line as u32 + 1, text.chars().count() as u32));
    }
    None
}

pub(super) fn diagnostic(source: &SourceSnapshot, error: &QualityIssue) -> QualityDiagnostic {
    let location = invalid_location(error).unwrap_or((1, 1));
    QualityDiagnostic {
        tool: QualityTool::Black,
        rule: error.code.clone(),
        message: error.message.clone(),
        hint: None,
        path: source.path.clone(),
        severity: "error".into(),
        location: QualityLocation {
            line: location.0,
            column: location.1,
            end_line: None,
            end_column: None,
            offset: None,
            end_offset: None,
        },
        fix: None,
    }
}

fn invalid_location(error: &QualityIssue) -> Option<(u32, u32)> {
    if error.code != INVALID_INPUT {
        return None;
    }
    let position = error.message.lines().next()?.rsplit_once(": ")?.1;
    let (line, column) = position.split_once(':')?;
    Some((line.parse().ok()?, column.parse().ok()?))
}
