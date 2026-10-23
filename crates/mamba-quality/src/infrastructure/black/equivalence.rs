//! Black 26.10.0's permitted AST normalization. See assets/black/LICENSE.
use mamba_language::application::quality_syntax::SyntaxDocument;
use ruff_python_ast::{
    comparable::ComparableStmt,
    visitor::transformer::{self, Transformer},
    Expr, Stmt, StringLiteralValue,
};

fn whitespace(character: char) -> bool {
    matches!(character, '\t'..='\r' | '\x1c'..='\x20' | '\u{85}' | '\u{a0}'
        | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}'
        | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn normalized_string(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .split([
            '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .map(|line| line.trim_matches(whitespace))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches(whitespace)
        .into()
}

fn unwrap_tuple(expression: Expr, targets: &mut Vec<Expr>) {
    match expression {
        Expr::Tuple(tuple) => {
            for item in tuple.elts {
                unwrap_tuple(item, targets);
            }
        }
        expression => targets.push(expression),
    }
}

struct BlackNormalization;

impl Transformer for BlackNormalization {
    fn visit_stmt(&self, statement: &mut Stmt) {
        if let Stmt::Expr(expression) = statement {
            if let Expr::StringLiteral(string) = expression.value.as_mut() {
                let mut literal = string.value.as_slice()[0].clone();
                literal.value = normalized_string(string.value.to_str()).into_boxed_str();
                string.value = StringLiteralValue::single(literal);
            }
        }
        if let Stmt::Delete(delete) = statement {
            let mut targets = Vec::new();
            for target in std::mem::take(&mut delete.targets) {
                unwrap_tuple(target, &mut targets);
            }
            delete.targets = targets;
        }
        transformer::walk_stmt(self, statement);
    }
}

pub fn equivalent(before: &SyntaxDocument<'_>, after: &SyntaxDocument<'_>) -> bool {
    if super::type_comments::fingerprint(before) != super::type_comments::fingerprint(after) {
        return false;
    }
    let mut before = before.parsed.syntax().body.clone();
    let mut after = after.parsed.syntax().body.clone();
    BlackNormalization.visit_body(&mut before);
    BlackNormalization.visit_body(&mut after);
    before.len() == after.len()
        && before
            .iter()
            .zip(&after)
            .all(|(before, after)| ComparableStmt::from(before) == ComparableStmt::from(after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamba_language::application::quality_syntax::{parse, PythonTarget};

    fn equal(before: &str, after: &str) -> bool {
        equivalent(
            &parse(before, PythonTarget::Py314, false).unwrap(),
            &parse(after, PythonTarget::Py314, false).unwrap(),
        )
    }

    #[test]
    fn accepts_black_docstring_and_delete_normalization() {
        assert!(equal(
            "def f():\n    ''' hello\n      world '''\n",
            "def f():\n    \"\"\"hello\n    world\"\"\"\n"
        ));
        assert!(equal("del ((x, y),)\n", "del x, y\n"));
    }

    #[test]
    fn rejects_changes_to_program_values_and_target_order() {
        assert!(!equal("value = ' hello '\n", "value = 'hello'\n"));
        assert!(!equal("del x, y\n", "del y, x\n"));
        assert!(!equal("result = function(1)\n", "result = other(1)\n"));
        assert!(!equal(
            "value = 1  # type: int\n",
            "value = 1  # type: str\n"
        ));
        assert!(!equal(
            "x = 1  # type: ignore\ny = 2  # type: ignore\n",
            "x = 1; y = 2  # type: ignore\n"
        ));
        assert!(!equal(
            "def f(a,  # type: int\n      b):\n    pass\n",
            "def f(a,\n      b  # type: int\n):\n    pass\n"
        ));
        assert!(equal(
            "value = 1  # type: int   \n",
            "value = 1  # type: int\n"
        ));
        assert!(equal(
            "def f():  # type: () -> int\n    pass\n",
            "def f():\n    # type: () -> int\n    pass\n"
        ));
        assert!(equal(
            "x = (1  # type: int\n)\n",
            "x = (1  # type: str\n)\n"
        ));
        assert!(!equal(
            "def f():  # type: () -> int\n    pass\n",
            "def f():  # type: () -> str\n    pass\n"
        ));
        assert!(!equal(
            "for x in xs:  # type: int\n    pass\n",
            "for x in xs:  # type: str\n    pass\n"
        ));
        assert!(!equal(
            "with manager as x:  # type: int\n    pass\n",
            "with manager as x:  # type: str\n    pass\n"
        ));
    }
}
