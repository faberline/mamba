use crate::prelude::*;
use ruff_python_ast::Parameter;

#[derive(Default)]
pub struct FormatParameter;

impl FormatNodeRule<Parameter> for FormatParameter {
    fn fmt_fields(&self, item: &Parameter, f: &mut PyFormatter) -> FormatResult<()> {
        let Parameter {
            range: _,
            node_index: _,
            name,
            annotation,
        } = item;

        name.format().fmt(f)?;

        if let Some(annotation) = annotation.as_deref() {
            token(":").fmt(f)?;

            if f.context().comments().has_leading(annotation)
                && !f.context().is_expression_parenthesized(annotation.into())
            {
                hard_line_break().fmt(f)?;
            } else {
                space().fmt(f)?;
            }

            if f.options().style_policy().is_some() {
                let parentheses = if annotation.is_bin_op_expr()
                    || annotation
                        .as_string_literal_expr()
                        .is_some_and(|string| string.value.is_implicit_concatenated())
                {
                    crate::expression::parentheses::Parenthesize::IfBreaksParenthesizedNested
                } else {
                    crate::expression::parentheses::Parenthesize::IfRequired
                };
                crate::expression::maybe_parenthesize_expression(annotation, item, parentheses)
                    .fmt(f)?;
            } else {
                annotation.format().fmt(f)?;
            }
        }

        Ok(())
    }
}
