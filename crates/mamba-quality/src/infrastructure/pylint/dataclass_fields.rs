//! Field calls preserve the original raw names and parent-specific validity.
use super::{
    callables::Target,
    dataclass_sites::{self, Parent},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

fn dataclass_module(name: &str) -> bool {
    matches!(
        name,
        "dataclasses" | "marshmallow_dataclass" | "pydantic.dataclasses"
    )
}
impl Checker<'_> {
    fn dataclass_function(&self, expression: &Expr) -> Option<String> {
        if let Some(resolved) = self.expression_target(expression) {
            if let Target::Function(function) = resolved.target {
                let module = match &resolved.context {
                    Context::Module(module) => module.as_str(),
                    Context::Scope(_) => self.module_name,
                };
                return dataclass_module(module).then(|| function.name.clone());
            }
            return None;
        }
        let qualified = self.stdlib_name(expression)?;
        let (module, name) = qualified.rsplit_once('.')?;
        dataclass_module(module).then(|| name.to_string())
    }
    fn field_dataclass_decorator(&self, expression: &Expr) -> bool {
        let expression = match expression {
            Expr::Call(value) => value.func.as_ref(),
            value => value,
        };
        if let Some(name) = self.dataclass_function(expression) {
            return name == "dataclass";
        }
        self.expression_target(expression).is_none()
            && self.stdlib_name(expression).is_none()
            && dataclass_sites::raw_name(expression) == Some("dataclass")
    }
    pub(super) fn check_dataclass_field(&mut self, call: &ExprCall) {
        if dataclass_sites::raw_name(&call.func) != Some("field")
            || self.dataclass_function(&call.func).is_none()
        {
            return;
        }
        let Some(site) = self.dataclass_sites.0.get(&call.start().to_u32()).cloned() else {
            return;
        };
        let message = match site.parent {
            Parent::Call(function)
                if matches!(&function, Expr::Name(value) if value.id.as_str() == "make_dataclass")
                    && self.dataclass_function(&function).is_some() =>
            {
                return
            }
            Parent::Class(decorators)
                if decorators
                    .iter()
                    .any(|value| self.field_dataclass_decorator(value)) =>
            {
                if site.direct_annotation {
                    return;
                }
                "it should be the value of an assignment within a dataclass."
            }
            _ => "it should be used within a dataclass or the make_dataclass() function.",
        };
        self.emit(
            "E3701",
            "invalid-field-call",
            &format!("Invalid usage of field(), {message}"),
            call.range(),
        );
    }
}
