//! Descriptor spelling and ownership retain original assignment diagnostics.
use super::{callables::Target, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtAssign};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_method_wrapper(&mut self, assignment: &StmtAssign) {
        if !self.scopes.in_class() {
            return;
        }
        let Expr::Call(call) = assignment.value.as_ref() else {
            return;
        };
        let Expr::Name(wrapper) = call.func.as_ref() else {
            return;
        };
        let (rule, symbol) = match wrapper.id.as_str() {
            "classmethod" => ("R0202", "no-classmethod-decorator"),
            "staticmethod" => ("R0203", "no-staticmethod-decorator"),
            _ => return,
        };
        let Some(Expr::Name(method)) = call.arguments.args.first() else {
            return;
        };
        if !self
            .class_stack
            .last()
            .is_some_and(|(class, _)| class.methods.contains_key(method.id.as_str()))
        {
            return;
        }
        if let Some(target) = assignment.targets.first() {
            self.emit(
                rule,
                symbol,
                &format!(
                    "Consider using a decorator instead of calling {}",
                    wrapper.id
                ),
                target.range(),
            );
        }
    }
    pub(super) fn is_property_assignment(&self, assignment: &StmtAssign) -> bool {
        self.scopes.in_class()
            && assignment.targets.len() == 1
            && matches!(assignment.targets.first(), Some(Expr::Name(_)))
            && self.expression_target(&assignment.value).is_some_and(|value| matches!(value.target, Target::Instance(class, _) if class.identity.0 == "builtins" && class.name == "property"))
    }
}
