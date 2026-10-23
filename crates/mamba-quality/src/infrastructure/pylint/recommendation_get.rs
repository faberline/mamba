//! Dictionary-get candidates retain raw target shapes and actual elif spans.
use super::{comparison_constants, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    ElifElseClause, Expr, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

fn same(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
        _ => {
            comparison_constants::constant(left)
                && comparison_constants::constant(right)
                && comparison_constants::same(left, right)
        }
    }
}
fn candidate(test: &Expr, body: &[Stmt], clauses: &[ElifElseClause]) -> Option<Expr> {
    let [Stmt::Assign(assignment)] = body else {
        return None;
    };
    let [Expr::Name(target)] = assignment.targets.as_slice() else {
        return None;
    };
    let Expr::Subscript(value) = assignment.value.as_ref() else {
        return None;
    };
    let Expr::Compare(test) = test else {
        return None;
    };
    if !same(&value.value, test.operands.get(1)?) || !same(&value.slice, test.operands.first()?) {
        return None;
    }
    if !clauses.is_empty() {
        let [clause] = clauses else {
            return None;
        };
        if clause.test.is_some() {
            return None;
        }
        let [Stmt::Assign(other)] = clause.body.as_slice() else {
            return None;
        };
        let [Expr::Name(other)] = other.targets.as_slice() else {
            return None;
        };
        if other.id != target.id {
            return None;
        }
    }
    Some(test.operands[1].clone())
}
#[derive(Default)]
pub(super) struct Blocks(BTreeMap<u32, (Expr, TextRange)>);
impl<'a> Visitor<'a> for Blocks {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::If(value) = statement {
            if let Some(dictionary) = candidate(&value.test, &value.body, &value.elif_else_clauses)
            {
                self.0
                    .insert(value.start().to_u32(), (dictionary, value.range()));
            }
            for (index, clause) in value.elif_else_clauses.iter().enumerate() {
                if let Some(test) = &clause.test {
                    if let Some(dictionary) =
                        candidate(test, &clause.body, &value.elif_else_clauses[index + 1..])
                    {
                        self.0.insert(
                            clause.start().to_u32(),
                            (dictionary, TextRange::new(clause.start(), value.end())),
                        );
                    }
                }
            }
        }
        walk_stmt(self, statement);
    }
}
impl Blocks {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut result = Self::default();
        result.visit_body(body);
        result
    }
}
impl Checker<'_> {
    pub(super) fn check_dictionary_get(&mut self, offset: u32) {
        let Some((dictionary, range)) = self.get_blocks.0.get(&offset) else {
            return;
        };
        let range = *range;
        if self
            .unpacked_value(dictionary, Context::Scope(self.scopes.current_index()))
            .is_some_and(|value| {
                value.dictionary && value.view.is_none() && value.entries.is_some()
            })
        {
            self.emit("R1715", "consider-using-get", "Consider using dict.get for getting values from a dict if a key is present or a default if not", range);
        }
    }
}
