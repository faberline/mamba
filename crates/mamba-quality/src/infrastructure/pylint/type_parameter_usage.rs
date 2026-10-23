//! Pylint retains unused PEP 695 variables after ordinary unused arguments.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr, StmtFunctionDef, TypeParam,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
#[derive(Default)]
struct Names(BTreeSet<String>);
impl<'a> Visitor<'a> for Names {
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Name(value) = value {
            if value.ctx.is_load() {
                self.0.insert(value.id.to_string());
            }
        }
        walk_expr(self, value);
    }
}
impl Checker<'_> {
    pub(super) fn check_unused_type_parameters(&mut self, function: &StmtFunctionDef) {
        let Some(parameters) = &function.type_params else {
            return;
        };
        let mut names = Names::default();
        for parameter in function.parameters.iter() {
            if let Some(value) = parameter.annotation() {
                names.visit_expr(value);
            }
        }
        if let Some(value) = &function.returns {
            names.visit_expr(value);
        }
        names.visit_body(&function.body);
        for parameter in &parameters.type_params {
            let name = match parameter {
                TypeParam::TypeVar(value) => &value.name,
                TypeParam::TypeVarTuple(value) => &value.name,
                TypeParam::ParamSpec(value) => &value.name,
            };
            if !names.0.contains(name.as_str())
                && !super::defaults::fixed().ignored(name.as_str(), false)
            {
                self.emit(
                    "W0612",
                    "unused-variable",
                    &format!("Unused variable '{}'", name),
                    parameter.range(),
                );
            }
        }
    }
}
