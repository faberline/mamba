//! Argument-order warnings compare raw formal names and named call-site values.
use super::{argument_order_names, callables::Signature, Checker};
use mamba_language::application::quality_syntax::python_ast::ExprCall;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_argument_order(&mut self, call: &ExprCall, signature: &Signature) {
        let Some(parameters) = &signature.parameter_ast else {
            return;
        };
        let mut formal: Vec<_> = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .map(|value| value.parameter.name.to_string())
            .collect();
        if signature.owner.is_some() && formal.first().is_some_and(|name| name == "self") {
            formal.remove(0);
        }
        let formal_set: BTreeSet<_> = formal.iter().collect();
        if formal_set.len() != formal.len() {
            return;
        }
        let Some(actual) = argument_order_names::collect(self, call) else {
            return;
        };
        let actual_set: BTreeSet<_> = actual.positional.iter().chain(&actual.keywords).collect();
        if actual_set != formal_set {
            return;
        }
        if !actual
            .positional
            .iter()
            .eq(formal.iter().take(actual.positional.len()))
        {
            self.emit(
                "W1114",
                "arguments-out-of-order",
                "Positional arguments appear to be out of order",
                call.range(),
            );
        }
    }
}
