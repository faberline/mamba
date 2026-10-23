//! Type-check statement diagnostics retain their original syntax gates.
use super::{operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtAssign, StmtFor};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_dunder_name_assignment(&mut self, statement: &StmtAssign) {
        if !matches!(statement.targets.first(), Some(Expr::Attribute(value)) if value.attr.as_str() == "__name__")
            || matches!(statement.value.as_ref(), Expr::StringLiteral(_))
            || matches!(
                self.operand(&statement.value),
                Operand::Unknown | Operand::Literal("str")
            )
        {
            return;
        }
        self.emit(
            "W1115",
            "non-str-assignment-to-dunder-name",
            "Non-string value assigned to __name__",
            statement.range(),
        );
    }

    pub(super) fn check_dictionary_missing_items(&mut self, statement: &StmtFor) {
        if statement.is_async
            || !matches!(statement.target.as_ref(), Expr::Tuple(value) if value.elts.len() == 2)
            || !matches!(statement.iter.as_ref(), Expr::Name(_))
        {
            return;
        }
        let Some(dictionary) =
            self.unpacked_value(&statement.iter, Context::Scope(self.scopes.current_index()))
        else {
            return;
        };
        if !dictionary.dictionary || dictionary.view.is_some() {
            return;
        }
        if dictionary.entries.is_some_and(|entries| {
            entries
                .iter()
                .any(|(key, _)| !matches!(key, Expr::Tuple(_)))
        }) {
            self.emit(
                "E1141",
                "dict-iter-missing-items",
                "Unpacking a dictionary in iteration without calling .items()",
                statement.range(),
            );
        }
    }
}
