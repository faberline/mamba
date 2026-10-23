//! The built-in method hook follows original direct member call semantics.
use super::*;
use quality_syntax::python_ast::ExprCall;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_from_attributes_call(
        &mut self,
        call: &ExprCall,
    ) {
        let Expr::Attribute(member) = call.func.as_ref() else {
            return;
        };
        if member.attr.as_str() != "from_orm" {
            return;
        }
        let Some(model) = self
            .infer(&member.value)
            .and_then(|ty| self.context.class_id(ty))
            .and_then(|id| {
                self.context.class(id).pydantic.clone().or_else(|| {
                    self.context
                        .classes
                        .borrow()
                        .embedded_pydantic_models
                        .get(&id)
                        .cloned()
                })
            })
        else {
            return;
        };
        if model.from_attributes != Some(true) {
            self.emit(
                "pydantic-orm",
                format!("\"{}\" does not have from_attributes=True", model.name),
                call,
            );
        }
    }
}
