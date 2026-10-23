//! Black's call split and end-of-line comment placement.
use crate::comments::{checkpoint, trailing_comments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use ruff_formatter::{format_args, write};
use ruff_python_ast::{Expr, token::TokenKind};
use ruff_text_size::Ranged;

pub(super) fn format_indexed_value(
    before: &super::AnyBeforeOperator,
    operator: &super::AnyAssignmentOperator,
    value: &Expr,
    statement: ruff_python_ast::AnyNodeRef,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    let target = match before {
        super::AnyBeforeOperator::AssignmentTarget(target)
        | super::AnyBeforeOperator::ChainTarget(target)
            if target.is_subscript_expr() =>
        {
            *target
        }
        super::AnyBeforeOperator::Expression(target)
            if target.is_subscript_expr() && statement.is_stmt_aug_assign() =>
        {
            *target
        }
        _ => return Ok(false),
    };
    if f.options().style_policy().is_none()
        || !(statement.is_stmt_assign() || statement.is_stmt_aug_assign())
        || !matches!(value, Expr::BinOp(_) | Expr::BoolOp(_) | Expr::If(_))
        || crate::expression::can_omit_optional_parentheses(value, f.context())
        || f.context().comments().has_trailing(statement)
        || f.context()
            .tokens()
            .in_range(statement.range())
            .iter()
            .any(|token| token.kind() == TokenKind::Comment)
    {
        return Ok(false);
    }
    let id = f.group_id("black_indexed_assignment");
    let before = before.memoized();
    let content = format_with(|f| {
        let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);
        value.format().with_options(Parentheses::Never).fmt(&mut f)
    })
    .memoized();
    let flat = format_with(|f| write!(f, [before, space(), operator, space(), content])).memoized();
    let expand = !crate::black_measurement::flat_line_fits(flat.inspect(f)?, f.context())?;
    let parenthesized =
        crate::expression::parentheses::parenthesized("(", &content, ")").with_force_expansion(
            crate::expression::black_trailers::has_magic(target, f.context()),
        );
    group(&format_args![
        before,
        space(),
        operator,
        space(),
        if_group_fits_on_line(&content).with_group_id(Some(id)),
        if_group_breaks(&parenthesized).with_group_id(Some(id)),
    ])
    .with_id(Some(id))
    .should_expand(expand)
    .fmt(f)?;
    Ok(true)
}

pub(super) fn format_call(expression: &Expr, f: &mut PyFormatter) -> FormatResult<bool> {
    let Some(call) = expression.as_call_expr() else {
        return Ok(false);
    };
    if f.options().style_policy().is_none()
        || call.arguments.is_empty()
        || f.context().is_expression_parenthesized(expression.into())
    {
        return Ok(false);
    }
    let comments = f.context().comments().clone();
    let root = comments.leading_dangling_trailing(expression);
    let opening = comments.dangling(&call.arguments);
    if root.has_leading()
        || root.has_trailing_own_line()
        || opening
            .iter()
            .any(|comment| comment.line_position().is_own_line())
    {
        return Ok(false);
    }
    let mut moved: Vec<_> = opening.iter().chain(root.trailing).collect();
    moved.sort_by_key(|comment| comment.start());
    if moved.is_empty()
        || moved.iter().any(|comment| {
            let source = &f.context().source()[comment.range()];
            crate::black_comments::type_payload(source).is_some()
                && !crate::black_comments::is_ignore(source)
                || crate::black_comments::has_later_type_comment(comment, f.context())
        })
    {
        return Ok(false);
    }
    let checkpoint = checkpoint::Checkpoint::new(&comments, expression.into());
    for comment in &moved {
        comment.mark_formatted();
    }
    let value = expression
        .format()
        .with_options(Parentheses::Never)
        .memoized();
    value.inspect(f)?;
    let suffix = format_with(|f: &mut PyFormatter| {
        for comment in &moved {
            comment.mark_unformatted();
            trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
        }
        Ok(())
    });
    let flat = format_with(|f| write!(f, [value, suffix]));
    let split = format_with(|f: &mut PyFormatter| {
        checkpoint.restore();
        for comment in opening {
            if crate::black_comments::is_ignore(&f.context().source()[comment.range()]) {
                comment.mark_unformatted();
            } else {
                comment.mark_formatted();
            }
        }
        call.func
            .format()
            .with_options(Parentheses::Preserve)
            .fmt(f)?;
        call.arguments.format().with_options(true).fmt(f)?;
        for comment in &moved {
            if !opening
                .iter()
                .any(|candidate| std::ptr::eq(candidate, *comment))
                || !crate::black_comments::is_ignore(&f.context().source()[comment.range()])
            {
                comment.mark_unformatted();
                trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
            }
        }
        Ok(())
    });
    let wrapped = format_with(|f| {
        write!(
            f,
            [
                token("("),
                block_indent(&format_args![value, suffix]),
                token(")")
            ]
        )
    });
    if opening.is_empty() {
        best_fitting![flat, split, wrapped]
            .with_mode(BestFittingMode::AllLines)
            .fmt(f)?;
    } else {
        // When even the call header exceeds the width, Black retains its
        // opening comment rather than adding an outer pair of parentheses.
        let original_opening = format_with(|f| {
            checkpoint.restore();
            for comment in opening {
                comment.mark_unformatted();
            }
            call.func
                .format()
                .with_options(Parentheses::Preserve)
                .fmt(f)?;
            call.arguments.format().with_options(true).fmt(f)?;
            for comment in root.trailing {
                comment.mark_unformatted();
                trailing_comments(std::slice::from_ref(comment)).fmt(f)?;
            }
            Ok(())
        });
        if checkpoint.has_inner_comment(call.arguments.range(), opening) {
            original_opening.fmt(f)?;
        } else {
            best_fitting![flat, split, original_opening]
                .with_mode(BestFittingMode::AllLines)
                .fmt(f)?;
        }
    }
    Ok(true)
}
