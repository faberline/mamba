//! Conflicting ordering declarations still retain their own signature errors.
use super::super::*;
use quality_syntax::python_ast::StmtFunctionDef;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_order_declaration(
        &mut self,
        function: &StmtFunctionDef,
    ) {
        if !self.in_class
            || super::super::super::classes::dataclass::order_symbol(function.name.as_str())
                .is_none()
        {
            return;
        }
        let Some(instance) = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.instance_type(ty))
        else {
            return;
        };
        if !self
            .context
            .class(self.context.class_id(instance).unwrap())
            .dataclass
            .as_ref()
            .is_some_and(|definition| definition.order)
        {
            return;
        }
        if function.parameters.len() < 2 && function.parameters.vararg.is_none() {
            let mut signature = signatures::Signature::from_function(
                function,
                &self.context,
                &self.scopes.variables(),
                &self.scopes.function_binder_input(
                    function,
                    &self
                        .context
                        .class(self.context.class_id(instance).unwrap())
                        .generic,
                ),
            );
            let mut parameters = signature.parameters.to_vec();
            if let Some(first) = parameters.first_mut() {
                first.ty = first.ty.or(Some(instance));
            }
            signature.parameters = parameters.into();
            signature.name = None;
            self.emit(
                "misc",
                format!(
                    "Invalid signature \"{}\"",
                    self.context
                        .name(self.context.callable_type(std::sync::Arc::new(signature)))
                ),
                &self.function_range(function),
            );
        }
    }
}
