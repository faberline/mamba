//! Select a right-hand bracket after accounting for a type-ignore line suffix.
use super::Snapshot;
use crate::comments::trailing_comments;
use crate::context::{with_extra_brackets, with_flat_bracket, with_forced_bracket};
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use ruff_formatter::{RemoveSoftLinesBuffer, write};
use ruff_python_ast::{Expr, StmtAssign};
use ruff_text_size::{Ranged, TextRange};

fn prefix(expr: &Expr, selected: TextRange) -> Option<(&Expr, &'static str)> {
    match expr {
        Expr::Call(call) if call.arguments.range() == selected => Some((&call.func, "(")),
        Expr::Subscript(index)
            if index.end() == selected.end() && index.value.end() <= selected.start() =>
        {
            Some((&index.value, "["))
        }
        Expr::Call(call) => prefix(&call.func, selected),
        Expr::Subscript(index) => prefix(&index.value, selected),
        Expr::Attribute(attribute) => prefix(&attribute.value, selected),
        _ => None,
    }
}

fn bracket_expr(expr: &Expr, selected: TextRange) -> Option<&Expr> {
    match expr {
        Expr::Call(call) if call.arguments.range() == selected => Some(expr),
        Expr::Subscript(index)
            if index.end() == selected.end() && index.value.end() <= selected.start() =>
        {
            Some(expr)
        }
        Expr::Call(call) => bracket_expr(&call.func, selected),
        Expr::Subscript(index) => bracket_expr(&index.value, selected),
        Expr::Attribute(attribute) => bracket_expr(&attribute.value, selected),
        _ => None,
    }
}

fn head(
    item: &StmtAssign,
    opening: TextRange,
    selected: TextRange,
    snapshot: &Snapshot,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let Some((prefix, delimiter)) = prefix(&item.value, selected) else {
        return hard_line_break().fmt(f);
    };
    with_flat_bracket(opening, f, |f| {
        with_forced_bracket(TextRange::empty(0.into()), f, |f| {
            for target in &item.targets {
                let mut buffer = RemoveSoftLinesBuffer::new(f);
                write!(
                    buffer,
                    [super::super::FormatTargetWithEqualOperator {
                        target,
                        preserve_parentheses: true
                    }]
                )?;
            }
            prefix.format().with_options(Parentheses::Never).fmt(f)?;
            token(delimiter).fmt(f)?;
            if selected == opening
                && let Some((comment, _)) = snapshot
                    .states
                    .iter()
                    .filter(|(comment, _)| opening.contains_range(comment.range()))
                    .min_by_key(|(comment, _)| comment.start())
            {
                trailing_comments(std::slice::from_ref(*comment)).fmt(f)?;
            }
            Ok(())
        })
    })
}

fn render(
    item: &StmtAssign,
    opening: TextRange,
    selected: TextRange,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let fields = |f: &mut PyFormatter| {
        with_forced_bracket(selected, f, |f| {
            for target in &item.targets {
                let mut buffer = RemoveSoftLinesBuffer::new(f);
                write!(
                    buffer,
                    [super::super::FormatTargetWithEqualOperator {
                        target,
                        preserve_parentheses: true,
                    }]
                )?;
            }
            item.value
                .format()
                .with_options(Parentheses::Never)
                .fmt(f)?;
            let comments = f.context().comments().clone();
            trailing_comments(comments.trailing(item)).fmt(f)
        })
    };
    if selected != opening {
        with_flat_bracket(opening, f, fields)
    } else {
        fields(f)
    }
}

pub(super) fn format(
    item: &StmtAssign,
    opening: TextRange,
    ranges: &[TextRange],
    snapshot: &Snapshot,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    if crate::expression::black_trailers::is_fluent(&item.value, f.context()) {
        let first = format_with(|f| {
            token("    ").fmt(f)?;
            with_flat_bracket(opening, f, |f| {
                with_forced_bracket(TextRange::empty(0.into()), f, |f| {
                    if let Some(expr) = bracket_expr(&item.value, opening) {
                        expr.format().with_options(Parentheses::Never).fmt(f)
                    } else {
                        hard_line_break().fmt(f)
                    }
                })
            })
        })
        .memoized();
        let fits = crate::black_measurement::flat_line_fits(first.inspect(f)?, f.context())?;
        snapshot.restore();
        let selected = if fits {
            TextRange::empty(0.into())
        } else {
            opening
        };
        let fields = |f: &mut PyFormatter| {
            with_forced_bracket(selected, f, |f| {
                super::super::FormatStmtAssign.fmt_fields(item, f)
            })
        };
        if fits {
            with_flat_bracket(opening, f, fields)?;
        } else {
            fields(f)?;
        }
        return Ok(true);
    }
    let flat = format_with(|f| render(item, opening, TextRange::empty(0.into()), f)).memoized();
    if crate::black_measurement::flat_line_fits(flat.inspect(f)?, f.context())? {
        flat.fmt(f)?;
        return Ok(true);
    }
    for selected in ranges
        .iter()
        .rev()
        .filter(|range| range.start() >= opening.start())
    {
        if prefix(&item.value, *selected).is_none() {
            continue;
        }
        snapshot.restore();
        let prefix = format_with(|f| head(item, opening, *selected, snapshot, f)).memoized();
        if crate::black_measurement::flat_line_fits(prefix.inspect(f)?, f.context())? {
            snapshot.restore();
            render(item, opening, *selected, f)?;
            return Ok(true);
        }
    }
    snapshot.restore();
    let tail: Vec<_> = ranges
        .iter()
        .copied()
        .filter(|range| range.start() > opening.start())
        .collect();
    with_extra_brackets(&tail, f, |f| render(item, opening, opening, f))?;
    Ok(true)
}
