use std::borrow::Cow;

use ruff_formatter::{FormatError, FormatOptions, SourceCode, format_args, write};
use ruff_python_ast::{AnyNodeRef, NodeKind, PySourceType};
use ruff_python_trivia::{
    CommentLinePosition, find_trailing_pragma_offset, is_pragma_comment, lines_after,
    lines_after_ignoring_trivia, lines_before,
};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::SourceComment;
use crate::context::NodeLevel;
use crate::prelude::*;
use crate::preview::is_trailing_pragma_in_comment_width_enabled;
use crate::statement::suite::should_insert_blank_line_after_class_in_stub_file;

/// Formats the leading comments of a node.
pub(crate) fn leading_node_comments<'a, T>(node: T) -> FormatLeadingComments<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    FormatLeadingComments::Node(node.into())
}

/// Formats the passed comments as leading comments
pub(crate) const fn leading_comments(comments: &[SourceComment]) -> FormatLeadingComments<'_> {
    FormatLeadingComments::Comments(comments)
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FormatLeadingComments<'a> {
    Node(AnyNodeRef<'a>),
    Comments(&'a [SourceComment]),
}

impl Format<PyFormatContext<'_>> for FormatLeadingComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        fn write_leading_comments(
            comments: &[SourceComment],
            definition: bool,
            f: &mut PyFormatter,
        ) -> FormatResult<()> {
            let source = f.context().source();
            let final_group_gap = if definition
                && f.options().style_policy().is_some()
                && !f.options().source_type().is_stub()
                && matches!(f.context().node_level(), NodeLevel::TopLevel(_))
                && comments
                    .last()
                    .is_some_and(|comment| lines_after(comment.end(), source) <= 1)
            {
                comments
                    .windows(2)
                    .enumerate()
                    .filter(|(_, pair)| lines_after(pair[0].end(), source) > 1)
                    .map(|(index, _)| index)
                    .next_back()
                    .filter(|index| {
                        !comments[index + 1..]
                            .iter()
                            .any(|comment| comment.is_suppression_off_comment(source))
                    })
            } else {
                None
            };
            for (index, comment) in comments
                .iter()
                .enumerate()
                .filter(|(_, comment)| comment.is_unformatted())
            {
                if f.context().compact_comprehension() && comment.line_position().is_own_line() {
                    hard_line_break().fmt(f)?;
                }
                let mut lines_after_comment = lines_after(comment.end(), source);
                if crate::black_suppression::off::comments::empty_with_gap(comment, f.context()) {
                    lines_after_comment = lines_after_comment.max(2);
                }
                if final_group_gap == Some(index) {
                    lines_after_comment = lines_after_comment.max(3);
                }
                write!(
                    f,
                    [format_comment(comment), empty_lines(lines_after_comment)]
                )?;

                comment.mark_formatted();
            }

            Ok(())
        }

        match self {
            FormatLeadingComments::Node(node) => {
                let comments = f.context().comments().clone();
                write_leading_comments(
                    comments.leading(*node),
                    node.is_stmt_class_def() || node.is_stmt_function_def(),
                    f,
                )
            }
            FormatLeadingComments::Comments(comments) => write_leading_comments(comments, false, f),
        }
    }
}

/// Formats the leading `comments` of an alternate branch and ensures that it preserves the right
/// number of empty lines before. The `last_node` is the last node of the preceding body.
///
/// For example, `last_node` is the last statement in the if body when formatting the leading
/// comments of the `else` branch.
pub(crate) fn leading_alternate_branch_comments<'a, T>(
    comments: &'a [SourceComment],
    last_node: Option<T>,
) -> FormatLeadingAlternateBranchComments<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    FormatLeadingAlternateBranchComments {
        comments,
        last_node: last_node.map(Into::into),
    }
}

pub(crate) struct FormatLeadingAlternateBranchComments<'a> {
    comments: &'a [SourceComment],
    last_node: Option<AnyNodeRef<'a>>,
}

impl Format<PyFormatContext<'_>> for FormatLeadingAlternateBranchComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if f.options().style_policy().is_some() {
            let definition = std::iter::successors(self.last_node, AnyNodeRef::last_child_in_body)
                .take_while(|node| !f.context().comments().has_trailing_own_line(*node))
                .any(|node| {
                    (node.is_stmt_function_def() || node.is_stmt_class_def())
                        && !crate::black_suppression::line::skipped_definition(node, f.context())
                });
            if definition {
                if f.options().source_type().is_stub()
                    && f.context()
                        .indent_level()
                        .to_ascii_spaces(f.options().indent_width())
                        > 0
                {
                    return if let Some(first) = self.comments.first() {
                        write!(
                            f,
                            [
                                empty_lines(lines_before(first.start(), f.context().source())),
                                leading_comments(self.comments)
                            ]
                        )
                    } else if let Some(last) = self.last_node {
                        empty_lines(lines_after_ignoring_trivia(
                            last.end(),
                            f.context().source(),
                        ))
                        .fmt(f)
                    } else {
                        Ok(())
                    };
                }
                write!(f, [empty_lines(2), leading_comments(self.comments)])?;
                return Ok(());
            }
        }
        if self.last_node.is_some_and(|preceding| {
            should_insert_blank_line_after_class_in_stub_file(preceding, None, f.context())
        }) {
            write!(f, [empty_line(), leading_comments(self.comments)])?;
        } else if let Some(first_leading) = self.comments.first() {
            // Leading comments only preserves the lines after the comment but not before.
            // Insert the necessary lines.
            write!(
                f,
                [empty_lines(lines_before(
                    first_leading.start(),
                    f.context().source()
                ))]
            )?;

            write!(f, [leading_comments(self.comments)])?;
        } else if let Some(last_preceding) = self.last_node {
            // The leading comments formatting ensures that it preserves the right amount of lines
            // after We need to take care of this ourselves, if there's no leading `else` comment.
            // Since the `last_node` could be a compound node, we need to skip _all_ trivia.
            //
            // For example, here, when formatting the `if` statement, the `last_node` (the `while`)
            // would end at the end of `pass`, but we want to skip _all_ comments:
            // ```python
            // if True:
            //     while True:
            //         pass
            //         # comment
            //
            //     # comment
            // else:
            //     ...
            // ```
            //
            // `lines_after_ignoring_trivia` is safe here, as we _know_ that the `else` doesn't
            // have any leading comments.
            write!(
                f,
                [empty_lines(lines_after_ignoring_trivia(
                    last_preceding.end(),
                    f.context().source()
                ))]
            )?;
        }

        Ok(())
    }
}

/// Formats the passed comments as trailing comments
pub(crate) fn trailing_comments(comments: &[SourceComment]) -> FormatTrailingComments<'_> {
    FormatTrailingComments {
        comments,
        blank_line_before: false,
    }
}

pub(crate) struct FormatTrailingComments<'a> {
    comments: &'a [SourceComment],
    blank_line_before: bool,
}

impl FormatTrailingComments<'_> {
    pub(crate) fn with_blank_line_before(mut self, enabled: bool) -> Self {
        self.blank_line_before = enabled;
        self
    }
}

impl Format<PyFormatContext<'_>> for FormatTrailingComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let mut has_trailing_own_line_comment = false;
        let mut blank_line_before = self.blank_line_before;

        for trailing in self
            .comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            has_trailing_own_line_comment |= trailing.line_position().is_own_line();

            if has_trailing_own_line_comment {
                let mut lines_before_comment = lines_before(trailing.start(), f.context().source());
                if blank_line_before {
                    lines_before_comment = lines_before_comment.max(2);
                    blank_line_before = false;
                }

                // A trailing comment at the end of a body or list
                // ```python
                // def test():
                //      pass
                //
                //      # Some comment
                // ```
                write!(
                    f,
                    [
                        line_suffix(
                            &format_args![
                                empty_lines(lines_before_comment),
                                format_comment(trailing),
                            ],
                            // Reserving width isn't necessary because we don't split
                            // comments and the empty lines expand any enclosing group.
                            0
                        ),
                        expand_parent()
                    ]
                )?;
            } else {
                // A trailing comment at the end of a line has a reserved width to
                // consider during line measurement.
                // ```python
                // tup = (
                //     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                // )  # Some comment
                // ```
                trailing_end_of_line_comment(trailing).fmt(f)?;
            }

            trailing.mark_formatted();
        }

        Ok(())
    }
}

/// Formats the dangling comments of `node`.
pub(crate) fn dangling_node_comments<'a, T>(node: T) -> FormatDanglingComments<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    FormatDanglingComments::Node(node.into())
}

pub(crate) fn dangling_comments(comments: &[SourceComment]) -> FormatDanglingComments<'_> {
    FormatDanglingComments::Comments(comments)
}

pub(crate) enum FormatDanglingComments<'a> {
    Node(AnyNodeRef<'a>),
    Comments(&'a [SourceComment]),
}

impl Format<PyFormatContext<'_>> for FormatDanglingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let dangling_comments = match self {
            Self::Comments(comments) => comments,
            Self::Node(node) => comments.dangling(*node),
        };

        let mut first = true;
        for comment in dangling_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            if first {
                match comment.line_position {
                    CommentLinePosition::OwnLine => {
                        write!(f, [hard_line_break()])?;
                    }
                    CommentLinePosition::EndOfLine => {
                        write!(f, [space(), space()])?;
                    }
                }
            }

            write!(
                f,
                [
                    format_comment(comment),
                    empty_lines(lines_after(comment.end(), f.context().source()))
                ]
            )?;

            comment.mark_formatted();

            first = false;
        }

        Ok(())
    }
}

/// Formats the dangling comments within a parenthesized expression, for example:
/// ```python
/// [  # comment
///     1,
///     2,
///     3,
/// ]
/// ```
pub(crate) fn dangling_open_parenthesis_comments(
    comments: &[SourceComment],
) -> FormatDanglingOpenParenthesisComments<'_> {
    FormatDanglingOpenParenthesisComments { comments }
}

pub(crate) struct FormatDanglingOpenParenthesisComments<'a> {
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatDanglingOpenParenthesisComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        for comment in self
            .comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            debug_assert!(
                comment.line_position().is_end_of_line(),
                "Expected dangling comment to be at the end of the line"
            );

            trailing_end_of_line_comment(comment)
                .with_original_header()
                .fmt(f)?;
            comment.mark_formatted();
        }

        Ok(())
    }
}

/// Formats the content of the passed comment.
///
/// * Adds a whitespace between `#` and the comment text except if the first character is a `#`, `:`, `'`, `!`, or `|`
/// * Replaces non breaking whitespaces with regular whitespaces except if in front of a `types:` comment
pub(crate) const fn format_comment(comment: &SourceComment) -> FormatComment<'_> {
    FormatComment { comment }
}

pub(crate) struct FormatComment<'a> {
    comment: &'a SourceComment,
}

impl Format<PyFormatContext<'_>> for FormatComment<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let slice = self.comment.slice();
        let source = SourceCode::new(f.context().source());

        if let Some(policy) = f.options().style_policy() {
            let normalized = policy.comment(slice.text(source));
            let mark = f.options().source_map_generation()
                == ruff_formatter::printer::SourceMapGeneration::Enabled;
            return write!(
                f,
                [
                    mark.then_some(source_position(slice.start())),
                    text(&normalized),
                    mark.then_some(source_position(slice.end())),
                ]
            );
        }

        let normalized_comment = normalize_comment(self.comment, source)?;

        format_normalized_comment(normalized_comment, slice.range()).fmt(f)
    }
}

/// Helper that inserts the appropriate number of empty lines before a comment, depending on the node level:
/// - Top-level: Up to two empty lines.
/// - Parenthesized: A single empty line.
/// - Otherwise: Up to a single empty line.
pub(crate) const fn empty_lines(lines: u32) -> FormatEmptyLines {
    FormatEmptyLines { lines }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLines {
    lines: u32,
}

impl Format<PyFormatContext<'_>> for FormatEmptyLines {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        match f.context().node_level() {
            NodeLevel::TopLevel(_) => match self.lines {
                0 | 1 => write!(f, [hard_line_break()]),
                2 => write!(f, [empty_line()]),
                _ => match f.options().source_type() {
                    PySourceType::Stub => {
                        write!(f, [empty_line()])
                    }
                    PySourceType::Python | PySourceType::Ipynb => {
                        write!(f, [empty_line(), empty_line()])
                    }
                },
            },

            NodeLevel::CompoundStatement => match self.lines {
                0 | 1 => write!(f, [hard_line_break()]),
                _ => write!(f, [empty_line()]),
            },

            // Remove all whitespace in parenthesized expressions
            NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression => {
                write!(f, [hard_line_break()])
            }
        }
    }
}

/// A helper that constructs a formattable element using a reserved-width line-suffix
/// for normalized comments.
///
/// * Black normalization of `SourceComment`.
/// * Line suffix with reserved width for the final, normalized content.
/// * Expands parent node.
const fn trailing_end_of_line_comment(
    comment: &SourceComment,
) -> FormatTrailingEndOfLineComment<'_> {
    FormatTrailingEndOfLineComment {
        comment,
        original_header: false,
    }
}

pub(crate) struct FormatTrailingEndOfLineComment<'a> {
    comment: &'a SourceComment,
    original_header: bool,
}

impl FormatTrailingEndOfLineComment<'_> {
    const fn with_original_header(mut self) -> Self {
        self.original_header = true;
        self
    }
}

impl Format<PyFormatContext<'_>> for FormatTrailingEndOfLineComment<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let slice = self.comment.slice();
        let source = SourceCode::new(f.context().source());
        let keep_type_boundary = f.options().style_policy().is_some()
            && crate::black_comments::has_later_type_comment(self.comment, f.context());
        let keep_skip_boundary = f.options().style_policy().is_some()
            && super::suppression::controls(slice.text(source))
                .contains(&ruff_python_trivia::SuppressionKind::Skip);
        let spacing = format_with(|f: &mut PyFormatter| {
            if keep_skip_boundary {
                let prefix = &f.context().source()[..usize::from(slice.start())];
                let length = prefix
                    .chars()
                    .rev()
                    .take_while(|character| matches!(character, ' ' | '\t'))
                    .map(char::len_utf8)
                    .sum::<usize>();
                text(&prefix[prefix.len() - length..]).fmt(f)
            } else {
                write!(f, [space(), space()])
            }
        });

        let normalized_comment = if let Some(policy) = f.options().style_policy() {
            Cow::Owned(policy.comment(slice.text(source)))
        } else {
            normalize_comment(self.comment, source)?
        };

        // Don't reserve width for pragma comments. In preview, comments
        // containing a trailing pragma (e.g., `# comment # noqa: F401`) only
        // reserve width for the non-pragma prefix.
        let non_pragma_comment_part = if f.options().style_policy().is_some() {
            &normalized_comment
        } else if is_trailing_pragma_in_comment_width_enabled(f.context()) {
            match find_trailing_pragma_offset(&normalized_comment) {
                Some(offset) => normalized_comment[..offset].trim_end(),
                None => &normalized_comment,
            }
        } else if is_pragma_comment(&normalized_comment) {
            ""
        } else {
            &normalized_comment
        };

        let reserved_width = if non_pragma_comment_part.is_empty()
            || f.options().style_policy().is_some()
                && self.original_header
                && f.context().flat_bracket().is_none()
                && crate::black_comments::unsplittable_header_ignore(self.comment, f.context())
        {
            0
        } else {
            // Start with 2 because of the two leading spaces.
            2u32.saturating_add(
                TextWidth::from_text(non_pragma_comment_part, f.options().indent_width())
                    .width()
                    .expect("Expected comment not to contain any newlines")
                    .value(),
            )
        };

        write!(
            f,
            [
                line_suffix(
                    &format_args![
                        spacing,
                        format_normalized_comment(normalized_comment, slice.range())
                    ],
                    reserved_width
                )
                .with_source_order(
                    f.options()
                        .style_policy()
                        .is_some()
                        .then_some(slice.start())
                ),
                f.options()
                    .style_policy()
                    .is_none()
                    .then_some(expand_parent()),
                (keep_type_boundary || keep_skip_boundary).then_some(expand_parent())
            ]
        )
    }
}

/// A helper that constructs formattable normalized comment text as efficiently as
/// possible.
///
/// * If the content is unaltered then format with source text slice strategy and no
///   unnecessary allocations.
/// * If the content is modified then make as few allocations as possible and use
///   a dynamic text element at the original slice's start position.
const fn format_normalized_comment(
    comment: Cow<'_, str>,
    range: TextRange,
) -> FormatNormalizedComment<'_> {
    FormatNormalizedComment { comment, range }
}

pub(crate) struct FormatNormalizedComment<'a> {
    comment: Cow<'a, str>,
    range: TextRange,
}

impl Format<PyFormatContext<'_>> for FormatNormalizedComment<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        let write_sourcemap = f.options().source_map_generation().is_enabled();

        write_sourcemap
            .then_some(source_position(self.range.start()))
            .fmt(f)?;

        match self.comment {
            Cow::Borrowed(borrowed) => {
                source_text_slice(TextRange::at(self.range.start(), borrowed.text_len())).fmt(f)?;
            }

            Cow::Owned(ref owned) => {
                text(owned).fmt(f)?;
            }
        }

        write_sourcemap
            .then_some(source_position(self.range.end()))
            .fmt(f)
    }
}

/// A helper for normalizing comments by:
/// * Trimming any trailing whitespace.
/// * Adding a leading space after the `#`, if necessary.
///
/// For example:
/// * `#comment` is normalized to `# comment`.
/// * `# comment ` is normalized to `# comment`.
/// * `#  comment` is left as-is.
/// * `#!comment` is left as-is.
fn normalize_comment<'a>(
    comment: &'a SourceComment,
    source: SourceCode<'a>,
) -> FormatResult<Cow<'a, str>> {
    let slice = comment.slice();
    let comment_text = slice.text(source);

    let trimmed = comment_text.trim_end();

    let content = strip_comment_prefix(trimmed)?;

    if content.is_empty() {
        return Ok(Cow::Borrowed("#"));
    }

    // Fast path for correctly formatted comments: if the comment starts with a space, or any
    // of the allowed characters, then it's included verbatim (apart for trimming any trailing
    // whitespace).
    if content.starts_with([' ', '!', ':', '#', '\'', '|']) {
        return Ok(Cow::Borrowed(trimmed));
    }

    // Otherwise, we need to normalize the comment by adding a space after the `#`.
    if content.starts_with('\u{A0}') {
        let trimmed = content.trim_start_matches('\u{A0}');
        if trimmed.trim_start().starts_with("type:") {
            // Black adds a space before the non-breaking space if part of a type pragma.
            Ok(Cow::Owned(std::format!("# {content}")))
        } else if trimmed.starts_with(' ') {
            // Black replaces the non-breaking space with a space if followed by a space.
            Ok(Cow::Owned(std::format!("# {trimmed}")))
        } else {
            // Otherwise we replace the first non-breaking space with a regular space.
            Ok(Cow::Owned(std::format!("# {}", &content["\u{A0}".len()..])))
        }
    } else {
        Ok(Cow::Owned(std::format!("# {content}")))
    }
}

/// A helper for stripping '#' from comments.
fn strip_comment_prefix(comment_text: &str) -> FormatResult<&str> {
    let Some(content) = comment_text.strip_prefix('#') else {
        return Err(FormatError::syntax_error(
            "Didn't find expected comment token `#`",
        ));
    };

    Ok(content)
}

/// Format the empty lines between a node and its trailing comments.
///
/// For example, given:
/// ```python
/// class Class:
///     ...
/// # comment
/// ```
///
/// This builder will insert two empty lines before the comment.
///
/// # Preview
///
/// For preview style, this builder will insert a single empty line after a
/// class definition in a stub file.
///
/// For example, given:
/// ```python
/// class Foo:
///     pass
/// # comment
/// ```
///
/// This builder will insert a single empty line before the comment.
pub(crate) fn empty_lines_before_trailing_comments(
    comments: &[SourceComment],
    node_kind: NodeKind,
) -> FormatEmptyLinesBeforeTrailingComments<'_> {
    FormatEmptyLinesBeforeTrailingComments {
        comments,
        node_kind,
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLinesBeforeTrailingComments<'a> {
    /// The trailing comments of the node.
    comments: &'a [SourceComment],
    node_kind: NodeKind,
}

impl Format<PyFormatContext<'_>> for FormatEmptyLinesBeforeTrailingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        if let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.line_position().is_own_line())
        {
            // Black has different rules for stub vs. non-stub and top level vs. indented
            let empty_lines = match (f.options().source_type(), f.context().node_level()) {
                (PySourceType::Stub, NodeLevel::TopLevel(_)) => 1,
                (PySourceType::Stub, _) => u32::from(self.node_kind == NodeKind::StmtClassDef),
                (_, NodeLevel::TopLevel(_)) => 2,
                (_, _) => 1,
            };

            let actual = lines_before(comment.start(), f.context().source()).saturating_sub(1);
            for _ in actual..empty_lines {
                empty_line().fmt(f)?;
            }
        }
        Ok(())
    }
}

/// Format the empty lines between a node and its leading comments.
///
/// For example, given:
/// ```python
/// # comment
///
/// class Class:
///     ...
/// ```
///
/// While `leading_comments` will preserve the existing empty line, this builder will insert an
/// additional empty line before the comment.
pub(crate) fn empty_lines_after_leading_comments(
    comments: &[SourceComment],
) -> FormatEmptyLinesAfterLeadingComments<'_> {
    FormatEmptyLinesAfterLeadingComments { comments }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLinesAfterLeadingComments<'a> {
    /// The leading comments of the node.
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatEmptyLinesAfterLeadingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        if let Some(comment) = self
            .comments
            .iter()
            .rev()
            .find(|comment| comment.line_position().is_own_line())
        {
            // Black has different rules for stub vs. non-stub and top level vs. indented
            let empty_lines = match (f.options().source_type(), f.context().node_level()) {
                (PySourceType::Stub, NodeLevel::TopLevel(_)) => 1,
                (PySourceType::Stub, _) => 0,
                (_, NodeLevel::TopLevel(_)) => 2,
                (_, _) => 1,
            };

            let actual = lines_after(comment.end(), f.context().source()).saturating_sub(1);
            // If there are no empty lines, keep the comment tight to the node.
            if actual == 0 {
                return Ok(());
            }

            // If there are more than enough empty lines already, `leading_comments` will
            // trim them as necessary.
            if actual >= empty_lines {
                return Ok(());
            }

            for _ in actual..empty_lines {
                empty_line().fmt(f)?;
            }
        }
        Ok(())
    }
}
