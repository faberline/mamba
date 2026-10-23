//! Syntax errors retain CPython's one-based offset and absent end position.
mod imported;
use crate::domain::{model::QualityTool, source::SourceSnapshot};
pub(super) use imported::import_failure;
use mamba_language::application::quality_syntax::{self, PythonTarget};

pub(super) fn diagnostic(
    source: &SourceSnapshot,
    target: PythonTarget,
) -> Option<crate::domain::model::QualityDiagnostic> {
    let document = quality_syntax::parse(&source.text, target, false).ok()?;
    if target == PythonTarget::Py312 {
        let mut defaults = GenericDefaults::default();
        mamba_language::application::quality_syntax::python_ast::visitor::Visitor::visit_body(
            &mut defaults,
            &document.parsed.syntax().body,
        );
        if let Some(offset) = defaults.0 {
            return Some(syntax_diagnostic(source, &document, offset, "expected '('"));
        }
    }
    let issue = document.issues.first()?;
    if let Some(message) = starred_comprehension(&document, issue) {
        return Some(syntax_diagnostic(source, &document, issue.start, message));
    }
    let line_start = source.text[..issue.start as usize]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    if issue.message == "Expected an expression"
        && source.text[issue.start as usize..]
            .chars()
            .next()
            .is_none_or(|value| matches!(value, '\n' | '\r'))
        && source.text[line_start..issue.start as usize]
            .trim_end()
            .ends_with(',')
        && document.tokens.iter().any(|token| {
            token.kind == "Assert"
                && token.start as usize >= line_start
                && token.start < issue.start
        })
    {
        return Some(syntax_diagnostic(
            source,
            &document,
            issue.start,
            "invalid syntax",
        ));
    }
    if issue.message == "Expected `,`, found newline"
        && document.tokens.iter().any(|token| {
            token.kind == "Lambda"
                && token.start as usize >= line_start
                && token.start < issue.start
        })
        && document
            .issues
            .iter()
            .any(|issue| issue.message == "Expected `:`, found end of file")
    {
        return Some(syntax_diagnostic(
            source,
            &document,
            issue.start,
            "invalid syntax",
        ));
    }
    if issue.message.starts_with("Expected `:`, found")
        && source
            .text
            .get(issue.start as usize..)
            .is_some_and(|text| text.is_empty() || text.starts_with(['\n', '\r']))
    {
        return Some(syntax_diagnostic(
            source,
            &document,
            issue.start,
            "expected ':'",
        ));
    }
    let mut offset = issue.start;
    let character = source.text[offset as usize..].chars().next()?;
    let message = if matches!(character, '\u{8}' | '\u{b}' | '\u{1a}' | '\u{1b}' | '\u{a0}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    {
        format!("invalid non-printable character U+{:04X}", character as u32)
    } else if issue.version_error && matches!(character, 't' | 'T') {
        offset += 1;
        "invalid syntax".into()
    } else if issue.message == "Lambda expression cannot be used here" {
        "invalid syntax".into()
    } else {
        return None;
    };
    if source.text.contains('\r') && !source.text.contains('\n') {
        let mut diagnostic = super::super::diagnostics::at(
            source,
            QualityTool::Pylint,
            "E0001",
            message,
            super::super::diagnostics::point(offset as usize),
        );
        diagnostic.location.line = 1;
        diagnostic.location.column = source.text[..offset as usize].chars().count() as u32;
        return Some(diagnostic);
    }
    Some(syntax_diagnostic(source, &document, offset, &message))
}

fn starred_comprehension(
    document: &quality_syntax::SyntaxDocument<'_>,
    issue: &quality_syntax::SyntaxIssue,
) -> Option<&'static str> {
    if !issue.version_error
        || !document
            .parsed
            .unsupported_syntax_errors()
            .iter()
            .any(|error| {
                error.range.start().to_u32() == issue.start
                    && matches!(
                        error.kind,
                        quality_syntax::UnsupportedSyntaxErrorKind::UnpackingInComprehension(_)
                    )
            })
    {
        return None;
    }
    match document
        .tokens
        .iter()
        .find(|token| token.start == issue.start)
        .map(|token| token.kind.as_str())
    {
        Some("Star") => Some("iterable unpacking cannot be used in comprehension"),
        Some("DoubleStar") => Some("dict unpacking cannot be used in dict comprehension"),
        _ => None,
    }
}

fn syntax_diagnostic(
    source: &SourceSnapshot,
    document: &quality_syntax::SyntaxDocument<'_>,
    offset: u32,
    message: &str,
) -> crate::domain::model::QualityDiagnostic {
    let (line, column) = document.line_column(offset);
    let name = source
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("module");
    let mut value = super::super::diagnostics::at(
        source,
        QualityTool::Pylint,
        "E0001",
        format!("Parsing failed: '{message} ({name}, line {line})'"),
        super::super::diagnostics::point(offset as usize),
    );
    value.location.column = column;
    value
}

#[derive(Default)]
struct GenericDefaults(Option<u32>);
impl<'a> mamba_language::application::quality_syntax::python_ast::visitor::Visitor<'a>
    for GenericDefaults
{
    fn visit_type_param(
        &mut self,
        parameter: &'a mamba_language::application::quality_syntax::python_ast::TypeParam,
    ) {
        use mamba_language::application::quality_syntax::python_ast::TypeParam;
        use ruff_text_size::Ranged;
        let default = match parameter {
            TypeParam::TypeVar(value) => value.default.as_ref(),
            TypeParam::TypeVarTuple(value) => value.default.as_ref(),
            TypeParam::ParamSpec(value) => value.default.as_ref(),
        };
        if default.is_some() && self.0.is_none() {
            self.0 = Some(parameter.start().to_u32().saturating_sub(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preceding_syntax_issue_keeps_priority() {
        let source = "broken =\n[*value for value in items]\n";
        let document = quality_syntax::parse(source, PythonTarget::Py312, false).unwrap();
        let issue = document.issues.first().unwrap();
        assert!(!issue.version_error);
        assert!(starred_comprehension(&document, issue).is_none());
    }
}
