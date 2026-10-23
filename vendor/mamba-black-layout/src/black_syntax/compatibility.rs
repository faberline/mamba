//! Black defers AST-only validation until the formatted text changes.
use super::*;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_parser::{InterpolatedStringErrorType, LexicalErrorType};
use ruff_text_size::Ranged;

const MIXED_TEMPLATE_STRING_ERROR: &str =
    "Cannot mix t-string literals with string or bytes literals";
const MIXED_BYTES_STRING_ERROR: &str = "Bytes literal cannot be mixed with non-bytes literals";

pub(super) fn has_mixed_string_literal_error(parsed: &Parsed<Mod>) -> bool {
    parsed.errors().iter().any(|error| {
        matches!(
            &error.error,
            ParseErrorType::OtherError(message)
                if message == MIXED_TEMPLATE_STRING_ERROR || message == MIXED_BYTES_STRING_ERROR
        )
    })
}

/// Project mixed interpolated and byte literals while preserving source offsets.
/// Black formats original tokens; this temporary AST keeps every literal part.
pub(super) fn project_mixed_string_literals(
    parsed: &Parsed<Mod>,
    tokens: &Tokens,
    projection: &mut [u8],
) {
    let errors: Vec<_> = parsed
        .errors()
        .iter()
        .filter(|error| {
            matches!(
                &error.error,
                ParseErrorType::OtherError(message)
                    if message == MIXED_TEMPLATE_STRING_ERROR || message == MIXED_BYTES_STRING_ERROR
            )
        })
        .map(|error| error.location)
        .collect();
    if errors.is_empty() {
        return;
    }

    for token in tokens.iter() {
        let start = token.start().to_usize();
        let end = token.end().to_usize();
        let overlaps_mixed_error = errors
            .iter()
            .any(|error| token.start() >= error.start() && token.start() < error.end());
        if !overlaps_mixed_error {
            continue;
        }

        let prefix_end = (start..end)
            .find(|index| matches!(projection[*index], b'\'' | b'"'))
            .unwrap_or(start);
        match token.kind() {
            TokenKind::TStringStart => {
                if let Some(index) =
                    (start..prefix_end).find(|index| matches!(projection[*index], b't' | b'T'))
                {
                    projection[index] = if projection[index] == b't' {
                        b'f'
                    } else {
                        b'F'
                    };
                }
            }
            TokenKind::String => {
                let Some(flags) = token.string_flags() else {
                    continue;
                };
                if !flags.is_byte_string() {
                    continue;
                }

                // Keep a literal node in Ruff's temporary CST. An f-string
                // prefix works with both raw and non-raw byte-prefix orders.
                // Mask braces so the literal body cannot become interpolation.
                if let Some(index) =
                    (start..prefix_end).find(|index| matches!(projection[*index], b'b' | b'B'))
                {
                    projection[index] = if projection[index] == b'b' {
                        b'f'
                    } else {
                        b'F'
                    };
                }
                for byte in &mut projection[prefix_end..end] {
                    if matches!(*byte, b'{' | b'}') {
                        *byte = b' ';
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) fn is_cst_error(error: &ParseErrorType) -> bool {
    if matches!(error, ParseErrorType::OtherError(message) if message == "Only single target (not tuple) can be annotated" || message == "Only single target (not list) can be annotated")
    {
        return true;
    }
    matches!(
        error,
        ParseErrorType::NonDefaultParamAfterDefaultParam
            | ParseErrorType::FStringError(
                InterpolatedStringErrorType::InvalidConversionFlag
                    | InterpolatedStringErrorType::SingleRbrace
            )
            | ParseErrorType::TStringError(
                InterpolatedStringErrorType::InvalidConversionFlag
                    | InterpolatedStringErrorType::SingleRbrace
            )
            | ParseErrorType::Lexical(
                LexicalErrorType::FStringError(
                    InterpolatedStringErrorType::InvalidConversionFlag
                        | InterpolatedStringErrorType::SingleRbrace
                ) | LexicalErrorType::TStringError(
                    InterpolatedStringErrorType::InvalidConversionFlag
                        | InterpolatedStringErrorType::SingleRbrace
                )
            )
    )
}

pub(super) fn has_indentation_errors(parsed: &Parsed<Mod>) -> bool {
    parsed.errors().iter().any(|error| {
        matches!(
            error.error,
            ParseErrorType::UnexpectedIndentation
                | ParseErrorType::Lexical(LexicalErrorType::IndentationError)
        )
    })
}

pub(super) fn project_indentation(parsed: &Parsed<Mod>, projection: &mut [u8]) {
    for error in parsed.errors() {
        if matches!(
            error.error,
            ParseErrorType::UnexpectedIndentation
                | ParseErrorType::Lexical(LexicalErrorType::IndentationError)
        ) {
            let start = error.location.start().to_usize();
            let end = error.location.end().to_usize();
            projection[start..end].fill(b' ');
            if start < end {
                projection[end - 1] = b'\x0c';
            }
        }
    }
}
