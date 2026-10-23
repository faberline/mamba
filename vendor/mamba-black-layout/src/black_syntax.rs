//! Black's CST grammar admits invalid semantic targets without dropping their AST nodes.
use ruff_python_ast::{
    Expr, Mod, PySourceType,
    token::{TokenKind, Tokens},
};
use ruff_python_parser::ParseErrorType;
use ruff_python_parser::{
    Mode, ParseError, ParseOptions, Parsed, parse_expression_range, parse_unchecked,
};
use ruff_text_size::{Ranged, TextRange};
mod aliases;
mod compatibility;

pub(crate) struct Alias {
    pub(crate) range: TextRange,
    expression: Expr,
}

impl Alias {
    pub(crate) fn expression(&self) -> &Expr {
        &self.expression
    }
}

pub(crate) struct Document {
    pub(crate) parsed: Parsed<Mod>,
    pub(crate) tokens: Tokens,
    pub(crate) aliases: Vec<Alias>,
}

/// Validate the complete Black grammar projection, including exception aliases.
pub fn supports_permissive_syntax(source: &str, source_type: PySourceType) -> bool {
    parse(source, source_type).is_ok()
}

pub(crate) fn parse(source: &str, source_type: PySourceType) -> Result<Document, ParseError> {
    let original = parse_unchecked(source, ParseOptions::from(source_type));
    let aliases = aliases::collect(source, original.tokens());
    let tokens = original.tokens().clone();
    let mixed_string_literals = compatibility::has_mixed_string_literal_error(&original);
    let parsed = if aliases.is_empty()
        && !compatibility::has_indentation_errors(&original)
        && !mixed_string_literals
    {
        original
    } else {
        // Identifier placeholders preserve every byte offset. Their values are
        // never rendered: alias expressions are retained and formatted instead.
        let mut projection = source.as_bytes().to_vec();
        for alias in &aliases {
            projection[alias.range.start().to_usize()..alias.range.end().to_usize()].fill(b'_');
        }
        if aliases.is_empty() {
            compatibility::project_indentation(&original, &mut projection);
        }
        compatibility::project_mixed_string_literals(&original, &tokens, &mut projection);
        parse_unchecked(
            std::str::from_utf8(&projection).unwrap(),
            ParseOptions::from(source_type),
        )
    };
    if let Some(error) = parsed.errors().iter().find(|error| {
        !is_permissive_target_error(&error.error) && !compatibility::is_cst_error(&error.error)
    }) {
        return Err(error.clone());
    }
    Ok(Document {
        parsed,
        tokens,
        aliases,
    })
}

/// These errors validate target context after the complete expression was parsed.
/// They do not indicate missing tokens or a recovered, incomplete syntax tree.
/// The caller still needs the source AST check whenever formatting changes it.
pub fn is_permissive_target_error(error: &ParseErrorType) -> bool {
    matches!(
        error,
        ParseErrorType::InvalidAssignmentTarget
            | ParseErrorType::InvalidNamedAssignmentTarget
            | ParseErrorType::InvalidAnnotatedAssignmentTarget
            | ParseErrorType::InvalidAugmentedAssignmentTarget
            | ParseErrorType::InvalidDeleteTarget
    )
}
