//! An unparenthesized match subject keeps its tuple comma without adding tuple brackets.
use crate::other::commas::has_trailing_comma;
use crate::prelude::*;
use ruff_formatter::write;
use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextRange};

pub(super) fn subject(subject: &Expr, f: &mut PyFormatter) -> FormatResult<bool> {
    let Some(tuple) = subject.as_tuple_expr() else {
        return Ok(false);
    };
    if f.options().style_policy().is_none()
        || tuple.parenthesized
        || f.context().comments().has(tuple)
    {
        return Ok(false);
    }
    let separator = format_with(|f| write!(f, [token(","), space()]));
    f.join_with(separator)
        .entries(tuple.elts.iter().formatted())
        .finish()?;
    if let Some(last) = tuple.elts.last()
        && has_trailing_comma(TextRange::new(last.end(), tuple.end()), f.context())
    {
        write!(f, [token(",")])?;
    }
    Ok(true)
}
