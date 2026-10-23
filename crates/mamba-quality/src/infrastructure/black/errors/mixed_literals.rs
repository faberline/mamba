//! Original Python AST errors for adjacent mixed string literal groups.
use crate::domain::{model::QualityIssue, source::SourceSnapshot};
use mamba_language::application::quality_syntax::{PythonTarget, SyntaxDocument};

const TEMPLATE_MIX: &str = "Cannot mix t-string literals with string or bytes literals";
const BYTES_MIX: &str = "Bytes literal cannot be mixed with non-bytes literals";

pub(super) fn is_mixed_literal_issue(message: &str) -> bool {
    message == TEMPLATE_MIX || message == BYTES_MIX
}

pub(super) fn source_ast(
    source: &SourceSnapshot,
    document: &SyntaxDocument<'_>,
    earlier_ast_error: Option<&(String, u32)>,
) -> Option<QualityIssue> {
    let template_issue = document
        .issues
        .iter()
        .find(|issue| !issue.version_error && issue.message == TEMPLATE_MIX);
    let byte_issue = document
        .issues
        .iter()
        .find(|issue| !issue.version_error && issue.message == BYTES_MIX);
    let mut candidates = Vec::new();
    if let Some(template) = template_issue {
        if document.target == PythonTarget::Py314 {
            candidates.push((
                "cannot mix t-string literals with string or bytes literals",
                document.line_column(template.start).0,
                template.start,
            ));
        } else if let Some(first) = first_literal(document, template.start, template.end) {
            let byte = byte_literal(document, template.start, template.end);
            if let Some(byte) = byte {
                if first.start == byte {
                    let reason = if enclosed_by_parentheses(document, template.start, template.end)
                    {
                        "invalid syntax. Perhaps you forgot a comma?"
                    } else {
                        "invalid syntax"
                    };
                    candidates.push((
                        reason,
                        document.line_column(template.start).0,
                        template.start,
                    ));
                } else if let Some(end) = last_literal_end(document, template.start, template.end) {
                    candidates.push((
                        "cannot mix bytes and nonbytes literals",
                        document.line_column(end).0,
                        template.start,
                    ));
                }
            } else if first.kind == "String"
                && !is_byte_string(document.text(first))
                && document.tokens.iter().any(|token| {
                    token.start > first.start
                        && token.start < template.end
                        && token.kind == "TStringStart"
                })
            {
                let reason = if enclosed_by_parentheses(document, template.start, template.end) {
                    "invalid syntax. Perhaps you forgot a comma?"
                } else {
                    "invalid syntax"
                };
                candidates.push((
                    reason,
                    document.line_column(template.start).0,
                    template.start,
                ));
            }
        }
    }

    let byte_overlaps_template = template_issue.is_some_and(|template| {
        byte_issue.is_some_and(|byte| byte.start < template.end && byte.end > template.start)
    });
    if let Some(issue) = byte_issue.filter(|_| !byte_overlaps_template) {
        if let Some(end) = last_literal_end(document, issue.start, issue.end) {
            candidates.push((
                "cannot mix bytes and nonbytes literals",
                document.line_column(end).0,
                issue.start,
            ));
        }
    }

    let (reason, line, selected_offset) = candidates
        .into_iter()
        .min_by_key(|(_, _, offset)| *offset)?;

    let earlier_syntax_issue = document.issues.iter().any(|issue| {
        !issue.version_error
            && !is_mixed_literal_issue(&issue.message)
            && issue.start < selected_offset
    });
    if earlier_syntax_issue
        || earlier_ast_error.is_some_and(|(_, offset)| *offset < selected_offset)
    {
        return None;
    }
    Some(super::source_ast_message(source, reason, line))
}

fn byte_literal(document: &SyntaxDocument<'_>, start: u32, end: u32) -> Option<u32> {
    document.tokens.iter().find_map(|token| {
        (token.kind == "String"
            && token.start < end
            && token.end > start
            && is_byte_string(document.text(token)))
        .then_some(token.start)
    })
}

fn first_literal<'a>(
    document: &'a SyntaxDocument<'_>,
    start: u32,
    end: u32,
) -> Option<&'a mamba_language::application::quality_syntax::SyntaxToken> {
    document.tokens.iter().find(|token| {
        token.start >= start
            && token.start < end
            && matches!(
                token.kind.as_str(),
                "String" | "FStringStart" | "TStringStart"
            )
    })
}

fn is_byte_string(text: &str) -> bool {
    text.find(['\'', '"']).is_some_and(|quote| {
        text[..quote]
            .bytes()
            .any(|byte| matches!(byte, b'b' | b'B'))
    })
}

fn last_literal_end(document: &SyntaxDocument<'_>, start: u32, end: u32) -> Option<u32> {
    document
        .tokens
        .iter()
        .filter(|token| {
            token.start >= start
                && token.end <= end
                && matches!(token.kind.as_str(), "String" | "FStringEnd" | "TStringEnd")
        })
        .map(|token| token.end)
        .next_back()
}

fn enclosed_by_parentheses(document: &SyntaxDocument<'_>, start: u32, end: u32) -> bool {
    let mut openings = Vec::new();
    let mut pairs = Vec::new();
    for (index, token) in document.tokens.iter().enumerate() {
        match token.kind.as_str() {
            "Lpar" => openings.push((index, "Rpar")),
            "Lsqb" => openings.push((index, "Rsqb")),
            "Lbrace" => openings.push((index, "Rbrace")),
            "Rpar" | "Rsqb" | "Rbrace"
                if openings
                    .last()
                    .is_some_and(|(_, closing)| *closing == token.kind) =>
            {
                let (opening, _) = openings.pop().unwrap();
                pairs.push((opening, index));
            }
            _ => {}
        }
    }
    pairs.into_iter().any(|(opening, closing)| {
        document.tokens[opening].kind == "Lpar"
            && document.tokens[opening].end <= start
            && document.tokens[closing].start >= end
    })
}
