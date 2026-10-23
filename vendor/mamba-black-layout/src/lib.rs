use ruff_db::PythonFile;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use thiserror::Error;
use tracing::Level;

pub use black_syntax::{is_permissive_target_error, supports_permissive_syntax};
pub use range::format_range;
use ruff_formatter::prelude::*;
use ruff_formatter::{FormatError, Formatted, PrintError, Printed, SourceCode, format, write};
use ruff_python_ast::{AnyNodeRef, Mod};
use ruff_python_parser::{ParseError, ParseOptions, Parsed, parse};
use ruff_python_trivia::TriviaRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{
    Comments, SourceComment, has_skip_comment, leading_node_comments, trailing_comments,
};
pub use crate::context::PyFormatContext;
pub use crate::db::Db;
pub use crate::options::{
    DocstringCode, DocstringCodeLineWidth, MagicTrailingComma, NestedStringQuoteStyle, PreviewMode,
    PyFormatOptions, QuoteStyle,
};
use crate::range::is_logical_line;
pub use crate::shared_traits::{AsFormat, FormattedIter, FormattedIterExt, IntoFormat};

mod black_syntax;
pub(crate) mod builders;
pub mod cli;
mod comments;
pub(crate) mod context;
mod db;
pub(crate) mod expression;
mod generated;
pub(crate) mod module;
mod options;
pub(crate) mod other;
pub(crate) mod pattern;
mod prelude;
mod preview;
mod range;
mod shared_traits;
pub(crate) mod statement;
pub(crate) mod string;
pub mod style;
pub(crate) mod type_param;
mod verbatim;

/// 'ast is the lifetime of the source code (input), 'buf is the lifetime of the buffer (output)
pub(crate) type PyFormatter<'ast, 'buf> = Formatter<'buf, PyFormatContext<'ast>>;

/// Rule for formatting a Python AST node.
pub(crate) trait FormatNodeRule<N>
where
    N: Ranged,
    for<'a> AnyNodeRef<'a>: From<&'a N>,
{
    fn fmt(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let node_ref = AnyNodeRef::from(node);
        let node_comments = comments.leading_dangling_trailing(node_ref);

        leading_node_comments(node_ref).fmt(f)?;

        // Emit source map information for nodes that are valid "narrowing" targets
        // in range formatting. Never emit source map information if they're disabled
        // for performance reasons.
        let emit_source_position = (is_logical_line(node_ref) || node_ref.is_mod_module())
            && f.options().source_map_generation().is_enabled();

        emit_source_position
            .then_some(source_position(node.start()))
            .fmt(f)?;

        if crate::black_suppression::off::format_fields(node_ref, f)?
            || crate::black_suppression::opening::format_fields(node_ref, f)?
            || crate::black_suppression::line::format_fields(node_ref, f)?
        {
            // The raw fields already contain every comment in their range.
        } else if crate::black_comments::unsplittable(node_ref, f.context())
            || node_ref.is_arguments() && f.context().flat_bracket() == Some(node.range())
        {
            let mut buffer = ruff_formatter::RemoveSoftLinesBuffer::new(f);
            write!(buffer, [format_with(|f| self.fmt_fields(node, f))])?;
        } else {
            self.fmt_fields(node, f)?;
        }
        if f.options().style_policy().is_some()
            && (node_ref.is_parameter() || node_ref.is_parameter_with_default())
            && node_comments.trailing.iter().any(|comment| {
                let source = &f.context().source()[comment.range()];
                crate::black_comments::type_payload(source).is_some()
                    && !crate::black_comments::is_ignore(source)
            })
        {
            expand_parent().fmt(f)?;
        }

        debug_assert!(
            node_comments
                .dangling
                .iter()
                .all(SourceComment::is_formatted),
            "The node has dangling comments that need to be formatted manually. Add the special dangling comments handling to `fmt_fields`."
        );

        let import_comment_gap = f.options().style_policy().is_some()
            && (node_ref.is_stmt_import() || node_ref.is_stmt_import_from());
        write!(
            f,
            [
                emit_source_position.then_some(source_position(node.end())),
                trailing_comments(node_comments.trailing)
                    .with_blank_line_before(import_comment_gap)
            ]
        )
    }

    /// Formats the node's fields.
    fn fmt_fields(&self, item: &N, f: &mut PyFormatter) -> FormatResult<()>;
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum FormatModuleError {
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    FormatError(#[from] FormatError),
    #[error(transparent)]
    PrintError(#[from] PrintError),
}

impl FormatModuleError {
    pub fn range(&self) -> Option<TextRange> {
        match self {
            FormatModuleError::ParseError(parse_error) => Some(parse_error.range()),
            FormatModuleError::FormatError(_) | FormatModuleError::PrintError(_) => None,
        }
    }
}

impl From<&FormatModuleError> for Diagnostic {
    fn from(error: &FormatModuleError) -> Self {
        match error {
            FormatModuleError::ParseError(parse_error) => Diagnostic::new(
                DiagnosticId::InternalError,
                Severity::Error,
                &parse_error.error,
            ),
            FormatModuleError::FormatError(format_error) => {
                Diagnostic::new(DiagnosticId::InternalError, Severity::Error, format_error)
            }
            FormatModuleError::PrintError(print_error) => {
                Diagnostic::new(DiagnosticId::InternalError, Severity::Error, print_error)
            }
        }
    }
}

#[tracing::instrument(name = "format", level = Level::TRACE, skip_all)]
pub fn format_module_source(
    source: &str,
    options: PyFormatOptions,
) -> Result<Printed, FormatModuleError> {
    let source_type = options.source_type();
    if options.style_policy().is_some() {
        let document = black_syntax::parse(source, source_type)?;
        let trivia = TriviaRanges::from(&document.tokens);
        let formatted = format_node_parts(
            document.parsed.syntax(),
            &document.tokens,
            &trivia,
            source,
            options,
            &document.aliases,
        )?;
        return Ok(formatted.print()?);
    }
    let parsed = parse(source, ParseOptions::from(source_type))?;
    let trivia = TriviaRanges::from(parsed.tokens());
    let formatted = format_module_ast(&parsed, &trivia, source, options)?;
    Ok(formatted.print()?)
}

pub fn format_module_ast<'a>(
    parsed: &'a Parsed<Mod>,
    trivia: &'a TriviaRanges,
    source: &'a str,
    options: PyFormatOptions,
) -> FormatResult<Formatted<PyFormatContext<'a>>> {
    format_node(parsed, trivia, source, options)
}

fn format_node<'a, N>(
    parsed: &'a Parsed<N>,
    trivia: &'a TriviaRanges,
    source: &'a str,
    options: PyFormatOptions,
) -> FormatResult<Formatted<PyFormatContext<'a>>>
where
    N: AsFormat<PyFormatContext<'a>>,
    &'a N: Into<AnyNodeRef<'a>>,
{
    format_node_parts(
        parsed.syntax(),
        parsed.tokens(),
        trivia,
        source,
        options,
        &[],
    )
}

fn format_node_parts<'a, N>(
    syntax: &'a N,
    tokens: &'a ruff_python_ast::token::Tokens,
    trivia: &'a TriviaRanges,
    source: &'a str,
    options: PyFormatOptions,
    aliases: &'a [black_syntax::Alias],
) -> FormatResult<Formatted<PyFormatContext<'a>>>
where
    N: AsFormat<PyFormatContext<'a>>,
    &'a N: Into<AnyNodeRef<'a>>,
{
    let source_code = SourceCode::new(source);
    let comments = Comments::from_ast_with_aliases(
        syntax,
        source_code,
        trivia,
        options.style_policy().is_some(),
        aliases,
    );

    let formatted = format!(
        PyFormatContext::new(options, source, comments, trivia, tokens).with_aliases(aliases),
        [syntax.format()]
    )?;
    formatted
        .context()
        .comments()
        .assert_all_formatted(source_code);
    Ok(formatted)
}

pub fn formatted_file(db: &dyn Db, file: File) -> Result<Option<String>, FormatModuleError> {
    let options = db.format_options(file);

    let parsed = parsed_module(db, PythonFile::new(db, file, options.target_version())).load(db);

    if let Some(first) = parsed.errors().first() {
        return Err(FormatModuleError::ParseError(first.clone()));
    }

    let trivia = TriviaRanges::from(parsed.tokens());
    let source = source_text(db, file);

    let formatted = format_node(&parsed, &trivia, &source, options)?;
    let printed = formatted.print()?;

    if printed.as_code() == &*source {
        Ok(None)
    } else {
        Ok(Some(printed.into_code()))
    }
}

/// Public function for generating a printable string of the debug comments.
pub fn pretty_comments(module: &Mod, trivia: &TriviaRanges, source: &str) -> String {
    let source_code = SourceCode::new(source);
    let comments = Comments::from_ast(module, source_code, trivia);

    std::format!("{comments:#?}", comments = comments.debug(source_code))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_snapshot;

    use ruff_python_ast::PySourceType;
    use ruff_python_parser::{ParseOptions, parse};
    use ruff_python_trivia::TriviaRanges;
    use ruff_text_size::{TextRange, TextSize};

    use crate::{PyFormatOptions, format_module_ast, format_module_source, format_range};

    /// Very basic test intentionally kept very similar to the CLI
    #[test]
    fn basic() -> Result<()> {
        let input = r"
# preceding
if    True:
    pass
# trailing
";
        let expected = r"# preceding
if True:
    pass
# trailing
";
        let actual = format_module_source(input, PyFormatOptions::default())?
            .as_code()
            .to_string();
        assert_eq!(expected, actual);
        Ok(())
    }

    /// Use this test to debug the formatting of some snipped
    #[ignore]
    #[test]
    fn quick_test() {
        let source = r#"
def hello(): ...

@lambda _, /: _
class A: ...
"#;
        let source_type = PySourceType::Python;

        // Parse the AST.
        let source_path = "code_inline.py";
        let parsed = parse(source, ParseOptions::from(source_type)).unwrap();
        let trivia = TriviaRanges::from(parsed.tokens());
        let options = PyFormatOptions::from_extension(Path::new(source_path));
        let formatted = format_module_ast(&parsed, &trivia, source, options).unwrap();

        // Uncomment the `dbg` to print the IR.
        // Use `dbg_write!(f, []) instead of `write!(f, [])` in your formatting code to print some IR
        // inside of a `Format` implementation
        // use ruff_formatter::FormatContext;
        // dbg!(formatted
        //     .document()
        //     .display(formatted.context().source_code()));
        //
        // dbg!(formatted
        //     .context()
        //     .comments()
        //     .debug(formatted.context().source_code()));

        let printed = formatted.print().unwrap();

        assert_eq!(
            printed.as_code(),
            r"for converter in connection.ops.get_db_converters(
    expression
) + expression.get_db_converters(connection):
    ...
"
        );
    }

    /// Use this test to quickly debug some formatting issue.
    #[ignore]
    #[test]
    fn range_formatting_quick_test() {
        let source = r#"def convert_str(value: str) -> str:  # Trailing comment
    """Return a string as-is."""

<RANGE_START>

    return value  # Trailing comment
<RANGE_END>"#;

        let mut source = source.to_string();

        let start = TextSize::try_from(
            source
                .find("<RANGE_START>")
                .expect("Start marker not found"),
        )
        .unwrap();

        source.replace_range(
            start.to_usize()..start.to_usize() + "<RANGE_START>".len(),
            "",
        );

        let end =
            TextSize::try_from(source.find("<RANGE_END>").expect("End marker not found")).unwrap();

        source.replace_range(end.to_usize()..end.to_usize() + "<RANGE_END>".len(), "");

        let source_type = PySourceType::Python;
        let options = PyFormatOptions::from_source_type(source_type);
        let printed = format_range(&source, TextRange::new(start, end), options).unwrap();

        let mut formatted = source.clone();
        formatted.replace_range(
            std::ops::Range::<usize>::from(printed.source_range()),
            printed.as_code(),
        );

        assert_eq!(
            formatted,
            r#"print ( "format me" )
print("format me")
print("format me")
print ( "format me" )
print ( "format me" )"#
        );
    }

    #[test]
    fn string_processing() {
        use crate::prelude::*;
        use ruff_formatter::{format, format_args, write};

        struct FormatString<'a>(&'a str);

        impl Format<SimpleFormatContext> for FormatString<'_> {
            fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
                let format_str = format_with(|f| {
                    write!(f, [token("\"")])?;

                    let mut words = self.0.split_whitespace().peekable();
                    let mut fill = f.fill();

                    let separator = format_with(|f| {
                        group(&format_args![
                            if_group_breaks(&token("\"")),
                            soft_line_break_or_space(),
                            if_group_breaks(&token("\" "))
                        ])
                        .fmt(f)
                    });

                    while let Some(word) = words.next() {
                        let is_last = words.peek().is_none();
                        let format_word = format_with(|f| {
                            write!(f, [text(word)])?;

                            if is_last {
                                write!(f, [token("\"")])?;
                            }

                            Ok(())
                        });

                        fill.entry(&separator, &format_word);
                    }

                    fill.finish()
                });

                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&token("(")),
                        soft_block_indent(&format_str),
                        if_group_breaks(&token(")"))
                    ])]
                )
            }
        }

        // 77 after g group (leading quote)
        let fits =
            r"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg h";
        let breaks =
            r"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg hh";

        let output = format!(
            SimpleFormatContext::default(),
            [FormatString(fits), hard_line_break(), FormatString(breaks)]
        )
        .expect("Formatting to succeed");

        assert_snapshot!(output.print().expect("Printing to succeed").as_code());
    }
}
mod black_comments;
mod black_measurement;
mod black_multiline;
mod black_suppression;
