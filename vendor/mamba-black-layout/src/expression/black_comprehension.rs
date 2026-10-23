//! Black's last-resort standalone-comment split inside an enclosing bracket.
use crate::context::{NodeLevel, with_compact_comprehension};
use crate::prelude::*;
use ruff_formatter::{RemoveSoftLinesBuffer, write};
use ruff_python_ast::Comprehension;
use ruff_text_size::Ranged;

pub(super) fn format(
    generators: &[Comprehension],
    f: &mut PyFormatter,
    fields: impl Fn(&mut PyFormatter) -> FormatResult<()>,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none()
        || f.context().compact_comprehension()
        || !matches!(f.context().node_level(), NodeLevel::ParenthesizedExpression)
    {
        return Ok(false);
    }
    let comments = f.context().comments();
    if !generators.iter().any(|generator| {
        comments
            .leading_dangling_trailing(generator)
            .into_iter()
            .filter(|comment| comment.end() <= generator.iter.start())
            .chain(comments.leading(&generator.target))
            .chain(comments.leading(&generator.iter))
            .any(|comment| comment.line_position().is_own_line())
    }) {
        return Ok(false);
    }
    with_compact_comprehension(f, |f| {
        let mut buffer = RemoveSoftLinesBuffer::new(f);
        write!(buffer, [format_with(fields)])
    })?;
    Ok(true)
}

pub(super) fn generators(items: &[Comprehension], f: &mut PyFormatter) -> FormatResult<()> {
    if !f.context().compact_comprehension() {
        return f
            .join_with(soft_line_break_or_space())
            .entries(items.iter().formatted())
            .finish();
    }
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            if f.context()
                .comments()
                .leading(item)
                .iter()
                .any(|comment| comment.line_position().is_own_line())
            {
                hard_line_break().fmt(f)?;
            } else {
                space().fmt(f)?;
            }
        }
        item.format().fmt(f)?;
    }
    Ok(())
}
