//! Default token layout checks retain Pylint's parentheses exemptions.
use super::Checker;
use mamba_language::application::quality_syntax::{self, python_ast::Stmt, SyntaxToken};
use ruff_text_size::Ranged;

fn parentheses(tokens: &[&SyntaxToken], source: &str, start: usize) -> Option<usize> {
    let text = |index: usize| {
        tokens
            .get(index)
            .map(|token| &source[token.start as usize..token.end as usize])
            .unwrap_or("")
    };
    let keyword = text(start);
    if text(start + 1) != "(" || (keyword == "not" && start > 0 && text(start - 1) == "is") {
        return None;
    }
    let mut depth = 0usize;
    let mut double = 0i32;
    let mut boolean = false;
    let mut walrus = None;
    for i in start..tokens.len().saturating_sub(1) {
        if tokens[i].kind == "NonLogicalNewline" {
            return None;
        }
        let value = text(i);
        if value == ":=" {
            walrus = Some(depth);
        }
        match value {
            "(" => {
                depth += 1;
                if text(i + 1) == "(" {
                    double = 1;
                }
            }
            ")" => {
                depth = depth.checked_sub(1)?;
                if depth != 0 {
                    if double != 0 && text(i + 1) == ")" {
                        if matches!(keyword, "in" | "if" | "not") {
                            continue;
                        }
                        return None;
                    }
                    double -= 1;
                    continue;
                }
                let ending = matches!(text(i + 1), ":" | ")" | "]" | "}" | "in")
                    || matches!(
                        tokens[i + 1].kind.as_str(),
                        "Newline" | "EndOfFile" | "Comment"
                    );
                if !ending
                    || boolean
                    || walrus.is_some_and(|level| level.saturating_sub(1) == depth)
                    || i == start + 2
                {
                    return None;
                }
                if keyword == "in"
                    && !(i == start + 3
                        && matches!(
                            tokens[start + 2].kind.as_str(),
                            "String" | "Int" | "Float" | "Complex"
                        ))
                {
                    return None;
                }
                return Some(start);
            }
            "," | "yield" | "for" if depth == 1 => return None,
            "and" | "or" if depth == 1 => boolean = true,
            "else" if depth == 1 => return parentheses(tokens, source, i),
            _ => {}
        }
    }
    None
}

impl Checker<'_> {
    pub(super) fn check_keyword_parentheses(
        &mut self,
        document: &quality_syntax::SyntaxDocument<'_>,
    ) {
        let tokens: Vec<_> = document
            .tokens
            .iter()
            .filter(|token| token.kind != "Whitespace" && token.kind != "TrailingTrivia")
            .collect();
        for (i, token) in tokens.iter().enumerate() {
            let keyword = document.text(token);
            if matches!(
                keyword,
                "assert"
                    | "del"
                    | "elif"
                    | "except"
                    | "for"
                    | "if"
                    | "in"
                    | "not"
                    | "raise"
                    | "return"
                    | "while"
                    | "yield"
                    | "with"
                    | "="
                    | ":="
            ) {
                let Some(position) = parentheses(&tokens, document.source, i) else {
                    continue;
                };
                let token = tokens[position];
                let keyword = document.text(token);
                let before = &document.source[..token.start as usize];
                let offset = before.rfind('\n').map_or(0, |index| index + 1);
                self.emit(
                    "C0325",
                    "superfluous-parens",
                    &format!("Unnecessary parens after '{keyword}' keyword"),
                    super::super::diagnostics::point(offset),
                );
            }
        }
    }

    pub(super) fn check_multiple_statements(&mut self, statement: &Stmt) {
        let line = self.source.text[..statement.start().to_usize()]
            .bytes()
            .filter(|&b| b == b'\n')
            .count() as u32
            + 1;
        let context = self.statements.contexts.get(&statement.start().to_u32());
        let stub = matches!(statement, Stmt::Expr(expr) if matches!(expr.value.as_ref(), mamba_language::application::quality_syntax::python_ast::Expr::EllipsisLiteral(_)))
            && context.is_some_and(|ctx| {
                matches!(
                    ctx.parent,
                    super::statements::Kind::Function | super::statements::Kind::Class
                )
            });
        if context.is_some_and(|context| context.previous_line == line)
            && !matches!(statement, Stmt::With(_))
            && !stub
            && self.reported_statement_lines.insert(line)
        {
            self.emit(
                "C0321",
                "multiple-statements",
                "More than one statement on a single line",
                statement.range(),
            );
        }
    }
}
