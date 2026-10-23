use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtDelete;
use ruff_text_size::{Ranged, TextRange};

use crate::builders::{PyFormatterExtensions, parenthesize_if_expands};
use crate::comments::dangling_node_comments;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parenthesize, parenthesized};
use crate::other::commas::has_trailing_comma;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: &StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtDelete {
            range: _,
            node_index: _,
            targets,
        } = item;

        write!(f, [token("del"), space()])?;

        match targets.as_slice() {
            [] => {
                write!(
                    f,
                    [
                        // Handle special case of delete statements without targets.
                        // ```
                        // del (
                        //     # Dangling comment
                        // )
                        token("("),
                        block_indent(&dangling_node_comments(item)),
                        token(")"),
                    ]
                )
            }
            [single] => {
                if f.options().style_policy().is_some()
                    && has_trailing_comma(TextRange::new(single.end(), item.end()), f.context())
                {
                    return parenthesized("(", &format_args![single.format(), token(",")], ")")
                        .fmt(f);
                }
                write!(
                    f,
                    [maybe_parenthesize_expression(
                        single,
                        item,
                        Parenthesize::IfBreaks
                    )]
                )
            }
            targets => {
                let item = format_with(|f| {
                    f.join_comma_separated(item.end())
                        .nodes(targets.iter())
                        .finish()
                });
                parenthesize_if_expands(&item).fmt(f)
            }
        }
    }
}
