//! String token warnings run before AST checks and preserve their column convention.
use super::Checker;
use mamba_language::application::quality_syntax::{self, python_ast::Expr};
use ruff_text_size::Ranged;

fn literal(text: &str) -> Option<(&str, &str, usize)> {
    let quote = text.find(['\'', '"'])?;
    let prefix = &text[..quote];
    let width = if text[quote..].starts_with(&text[quote..quote + 1].repeat(3)) {
        3
    } else {
        1
    };
    let body = text.get(quote + width..text.len().checked_sub(width)?)?;
    Some((prefix, body, quote + width))
}

impl Checker<'_> {
    pub(super) fn check_string_tokens(&mut self, document: &quality_syntax::SyntaxDocument<'_>) {
        for token in document
            .tokens
            .iter()
            .filter(|token| token.kind == "String")
        {
            let Some((prefix, body, start)) = literal(document.text(token)) else {
                continue;
            };
            if prefix.contains(['r', 'R']) {
                continue;
            }
            let mut characters = body.char_indices();
            while let Some((index, character)) = characters.next() {
                if character != '\\' {
                    continue;
                }
                let Some((_, next)) = characters.next() else {
                    break;
                };
                let offset = token.start as usize + start + index;
                let matched = format!("\\{next}");
                if "uUN".contains(next) && prefix.contains(['b', 'B']) {
                    self.emit("W1402", "anomalous-unicode-escape-in-string", &format!("Anomalous Unicode escape in byte string: '{matched}'. String constant might be missing an r or u prefix."), super::super::diagnostics::point(offset));
                } else if !"uUNabfnrtvx\n\r\t\\'\"01234567".contains(next) {
                    self.emit("W1401", "anomalous-backslash-in-string", &format!("Anomalous backslash in string: '{matched}'. String constant might be missing an r prefix."), super::super::diagnostics::point(offset));
                }
            }
        }
    }

    pub(super) fn check_string_assignment(&mut self, expression: &Expr) {
        self.check_concatenation(expression, "assignment");
    }

    pub(super) fn check_string_constants(&mut self, expression: &Expr) {
        let elements: Option<(&[Expr], &str)> = match expression {
            Expr::Call(call) => Some((call.arguments.args.as_ref(), "call")),
            Expr::List(list) => Some((list.elts.as_ref(), "list")),
            Expr::Set(set) => Some((set.elts.as_ref(), "set")),
            Expr::Tuple(tuple) => Some((tuple.elts.as_ref(), "tuple")),
            _ => None,
        };
        if let Some((elements, label)) = elements {
            for element in elements {
                self.check_concatenation(element, label);
            }
        }
        if let Expr::StringLiteral(string) = expression {
            if self.source.text[string.range()].starts_with('u') {
                self.emit(
                    "W1406",
                    "redundant-u-string-prefix",
                    "The u prefix for strings is no longer necessary in Python >=3.0",
                    super::super::diagnostics::point(string.start().to_usize()),
                );
            }
        }
    }

    fn check_concatenation(&mut self, expression: &Expr, label: &str) {
        let Expr::StringLiteral(string) = expression else {
            return;
        };
        let [first, second, ..] = string.value.as_slice() else {
            return;
        };
        let Some((prefix, body, _)) = literal(&self.source.text[first.range()]) else {
            return;
        };
        let Some((next_prefix, _, _)) = literal(&self.source.text[second.range()]) else {
            return;
        };
        if prefix.contains(['r', 'R']) != next_prefix.contains(['r', 'R'])
            || body == string.value.to_str()
        {
            return;
        }
        if self.source.text[first.start().to_usize()..second.start().to_usize()].contains('\n') {
            return;
        }
        let before = &self.source.text[..first.start().to_usize()];
        let offset = before.rfind('\n').map_or(0, |index| index + 1);
        self.emit(
            "W1404",
            "implicit-str-concat",
            &format!("Implicit string concatenation found in {label}"),
            super::super::diagnostics::point(offset),
        );
    }
}
