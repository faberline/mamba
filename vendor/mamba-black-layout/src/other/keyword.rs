use ruff_formatter::write;
use ruff_python_ast::Keyword;

use crate::prelude::*;
use crate::{builders::parenthesize_if_expands, expression::parentheses::Parentheses};

#[derive(Default)]
pub struct FormatKeyword;

impl FormatNodeRule<Keyword> for FormatKeyword {
    fn fmt_fields(&self, item: &Keyword, f: &mut PyFormatter) -> FormatResult<()> {
        let Keyword {
            range: _,
            node_index: _,
            arg,
            value,
        } = item;
        let formatted = format_with(|f: &mut PyFormatter| {
            if f.options().style_policy().is_some() && value.is_if_expr() {
                parenthesize_if_expands(&value.format().with_options(Parentheses::Never)).fmt(f)
            } else {
                value.format().fmt(f)
            }
        });
        // Comments after the `=` or `**` are reassigned as leading comments on the value.
        if let Some(arg) = arg {
            write!(f, [arg.format(), token("="), formatted])
        } else {
            write!(f, [token("**"), formatted])
        }
    }
}
