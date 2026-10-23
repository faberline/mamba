use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator, StmtExpr};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::trailing_semicolon;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        if let Expr::Generator(generator) = value.as_ref()
            && f.options().style_policy().is_some()
        {
            generator
                .format()
                .with_options(crate::expression::expr_generator::black_parentheses(
                    generator,
                    item.into(),
                    f.context().tokens(),
                ))
                .fmt(f)?;
        } else if let Expr::Tuple(tuple) = value.as_ref()
            && f.options().style_policy().is_some()
            && !tuple.parenthesized
            && tuple.elts.len() == 1
        {
            tuple
                .format()
                .with_options(crate::expression::expr_tuple::TupleParentheses::Preserve)
                .fmt(f)?;
        } else if is_arithmetic_like(value) {
            maybe_parenthesize_expression(value, item, Parenthesize::Optional).fmt(f)?;
        } else {
            value.format().fmt(f)?;
        }

        if f.options().source_type().is_ipynb()
            && f.context().node_level().is_last_top_level_statement()
            && trailing_semicolon(item.into(), f.context().source()).is_some()
        {
            token(";").fmt(f)?;
        }

        Ok(())
    }
}

const fn is_arithmetic_like(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::BinOp(ast::ExprBinOp {
            op: Operator::BitOr
                | Operator::BitXor
                | Operator::LShift
                | Operator::RShift
                | Operator::Add
                | Operator::Sub,
            ..
        })
    )
}
