//! Imported syntax failures retain the original dependency error detail.
use crate::domain::source::SourceSnapshot;
use mamba_language::application::quality_syntax::{self, PythonTarget};

pub(in super::super) fn import_failure(
    source: &SourceSnapshot,
    module: &str,
    target: PythonTarget,
) -> Option<String> {
    if let Some(issue) = &source.decoding_issue {
        if let Some(label) = issue.message.strip_prefix("Unsupported source encoding: ") {
            return Some(format!(
                "unknown encoding for '{}': {label}",
                source.path.display()
            ));
        }
        return None;
    }
    if source.text.contains('\0') {
        return Some("source code string cannot contain null bytes".into());
    }
    let document = quality_syntax::parse(&source.text, target, false).ok()?;
    if document.is_valid() {
        return None;
    }
    let mut opening = Vec::new();
    for token in &document.tokens {
        match document.text(token) {
            "(" | "[" | "{" => opening.push(token),
            ")" | "]" | "}" => {
                let closing = document.text(token);
                if let Some(open) = opening.pop() {
                    let pair = match document.text(open) {
                        "(" => ")",
                        "[" => "]",
                        _ => "}",
                    };
                    if closing != pair {
                        let (line, _) = document.line_column(token.start);
                        return Some(format!("closing parenthesis '{closing}' does not match opening parenthesis '{}' ({module}, line {line})", document.text(open)));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(token) = opening.last() {
        let (line, _) = document.line_column(token.start);
        return Some(format!(
            "'{}' was never closed ({module}, line {line})",
            document.text(token)
        ));
    }
    if document.tokens.iter().any(|token| token.kind == "Lambda")
        && document
            .issues
            .iter()
            .any(|issue| issue.message.starts_with("Expected `:`, found"))
    {
        let token = document
            .tokens
            .iter()
            .find(|token| token.kind == "Lambda")?;
        let (line, _) = document.line_column(token.start);
        return Some(format!("invalid syntax ({module}, line {line})"));
    }
    let diagnostic = super::diagnostic(source, target)?;
    diagnostic
        .message
        .strip_prefix("Parsing failed: '")
        .and_then(|text| text.strip_suffix('\''))
        .map(|text| {
            let name = source
                .path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("module");
            text.replace(&format!("({name}, line "), &format!("({module}, line "))
        })
}
