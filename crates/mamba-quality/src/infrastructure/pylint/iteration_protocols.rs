//! Async iteration checks use the inferred asynchronous protocol.
use super::{display, operand_inference::Operand, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFor};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_sync_yield_iteration(&mut self, value: &Expr) {
        if matches!(
            value,
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_)
        ) {
            return;
        }
        let invalid = match self.operand(value) {
            Operand::Unknown | Operand::Generator => false,
            Operand::Literal(name) => !matches!(
                name,
                "list" | "tuple" | "set" | "frozenset" | "dict" | "str" | "bytes"
            ),
            Operand::Function(_) => true,
            Operand::Instance(class, origin) | Operand::Class(class, origin) => {
                self.operand_known_bases(class.clone(), origin.clone(), &mut BTreeSet::new())
                    && !self.dynamic_member(class.clone(), origin.clone())
                    && self
                        .class_member(class.clone(), origin.clone(), "__iter__", true)
                        .is_none()
                    && self
                        .class_member(class, origin, "__getitem__", true)
                        .is_none()
            }
        };
        if invalid {
            let text = display::expression(value, &self.source.text);
            self.emit(
                "E1133",
                "not-an-iterable",
                &format!("Non-iterable value {text} is used in an iterating context"),
                value.range(),
            );
        }
    }
    pub(super) fn check_async_iteration(&mut self, statement: &StmtFor) {
        if !statement.is_async
            || matches!(
                statement.iter.as_ref(),
                Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_)
            )
        {
            return;
        }
        let invalid = match self.operand(&statement.iter) {
            Operand::Unknown => self
                .unpacked_value(
                    &statement.iter,
                    super::resolution::Context::Scope(self.scopes.current_index()),
                )
                .is_some_and(|value| value.view.is_some()),
            Operand::Generator => false,
            Operand::Literal(_) | Operand::Function(_) => true,
            Operand::Instance(class, origin) | Operand::Class(class, origin) => {
                self.operand_known_bases(class.clone(), origin.clone(), &mut BTreeSet::new())
                    && !self.dynamic_member(class.clone(), origin.clone())
                    && self
                        .class_member(class, origin, "__aiter__", true)
                        .is_none()
            }
        };
        if invalid {
            let value = display::expression(&statement.iter, &self.source.text);
            self.emit(
                "E1133",
                "not-an-iterable",
                &format!("Non-iterable value {value} is used in an iterating context"),
                statement.iter.range(),
            );
        }
    }
}
