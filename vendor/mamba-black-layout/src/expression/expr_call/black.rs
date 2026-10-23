//! Split implicit literal heads before call arguments inside commented parentheses.
use crate::comments::checkpoint::Checkpoint;
use crate::prelude::*;
use ruff_python_ast::{ExprCall, StringLike, token::parentheses_iterator};
use ruff_text_size::Ranged;

pub(super) fn literal_head<'ast>(
    call: &ExprCall,
    formatted: &impl Format<PyFormatContext<'ast>>,
    f: &mut PyFormatter<'ast, '_>,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none() || f.context().forced_bracket().is_some() {
        return Ok(false);
    }
    let Some(attribute) = call.func.as_attribute_expr() else {
        return Ok(false);
    };
    let Ok(string) = StringLike::try_from(attribute.value.as_ref()) else {
        return Ok(false);
    };
    if !string.is_implicit_concatenated() {
        return Ok(false);
    }
    let Some(range) = parentheses_iterator(call.into(), None, f.context().tokens()).last() else {
        return Ok(false);
    };
    let comments = f.context().comments().clone();
    if !comments.leading(call).iter().any(|comment| {
        comment.line_position().is_end_of_line()
            && range.contains_range(comment.range())
            && comment.end() < call.start()
    }) {
        return Ok(false);
    }
    let checkpoint = Checkpoint::new(&comments, call.into());
    let default = formatted.memoized();
    if crate::black_measurement::flat_line_fits(default.inspect(f)?, f.context())? {
        default.fmt(f)?;
    } else {
        checkpoint.restore();
        crate::context::with_forced_bracket(string.range(), f, |f| {
            FormatNodeRule::fmt_fields(call.format().rule(), call, f)
        })?;
    }
    Ok(true)
}
