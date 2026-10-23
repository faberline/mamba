//! Shared Rust document layout. Black-specific compatibility is verified separately.
use crate::domain::{model::QualityIssue, source::SourceSnapshot};
use mamba_black_layout::{
    format_module_source, DocstringCode, MagicTrailingComma, NestedStringQuoteStyle, PreviewMode,
    PyFormatOptions, QuoteStyle,
};
use mamba_language::application::quality_syntax::PythonTarget;
use ruff_formatter::{
    printer::{LineEnding, SourceMapGeneration},
    IndentStyle, IndentWidth, LineWidth,
};
use ruff_python_ast::PySourceType;
use std::{borrow::Cow, sync::Arc};

pub fn format(source: &SourceSnapshot, target: PythonTarget) -> Result<String, QualityIssue> {
    let source_type = if source.is_stub {
        PySourceType::Stub
    } else {
        PySourceType::Python
    };
    let ending = super::super::encoding::newline(&source.text);
    let normalized = if source.text.contains('\r') {
        Cow::Owned(source.text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(source.text.as_str())
    };
    let options = PyFormatOptions::from_source_type(source_type)
        .with_source_map_generation(if normalized.contains('\x0c') {
            SourceMapGeneration::Enabled
        } else {
            SourceMapGeneration::Disabled
        })
        .with_style_policy(Arc::new(super::policy::BlackPolicy))
        .with_target_version(target.native())
        .with_line_width(LineWidth::try_from(88).unwrap())
        .with_indent_style(IndentStyle::Space)
        .with_indent_width(IndentWidth::try_from(4).unwrap())
        .with_line_ending(LineEnding::LineFeed)
        .with_quote_style(QuoteStyle::Double)
        .with_nested_string_quote_style(NestedStringQuoteStyle::Preferred)
        .with_magic_trailing_comma(MagicTrailingComma::Respect)
        .with_docstring_code(DocstringCode::Disabled)
        .with_preview(PreviewMode::Disabled);
    format_module_source(&normalized, options)
        .map(|printed| {
            let output = if normalized.contains('\x0c') {
                super::formfeeds::restore(
                    &normalized,
                    printed.as_code(),
                    printed.sourcemap(),
                    target,
                    source.is_stub,
                )
            } else {
                printed.into_code()
            };
            if output == normalized {
                source.text.clone()
            } else {
                output.replace('\n', ending)
            }
        })
        .map_err(|error| {
            QualityIssue::new("black-layout", error.to_string(), Some(source.path.clone()))
        })
}
