//! Embedded collection classes bind annotations separately from runtime values.
use super::*;

impl Scopes {
    pub(super) fn collection_alias_class(
        &self,
        expression: &Expr,
        variables: &types::Variables,
    ) -> Option<u32> {
        let ty = self.context.annotation_binding(expression, variables)?;
        let id = self.context.class_id(ty)?;
        (self.context.is_class_value(ty)
            && matches!(
                self.context.class(id).namespace.as_str(),
                "builtins.reversed" | "builtins.enumerate" | "builtins.range" | "builtins.zip"
            ))
        .then_some(id)
    }

    pub(super) fn bind_collection_annotations(&self, variables: &mut types::Variables) {
        let target = self.context.module_target(&self.namespace);
        self.context.collection_constructor("reversed", target);
        self.context.aggregation_function("zip", target);
        for name in ["reversed", "enumerate", "range", "zip"] {
            let id = self
                .context
                .declare_class(&format!("builtins.{name}"), name, 0);
            variables
                .entry(name.into())
                .or_insert(self.context.class(id).object);
        }
        for name in types::Context::builtin_value_class_names(target) {
            if types::Context::builtin_name(&name).is_some()
                || self
                    .context
                    .builtin_constructor_alias(&name, target)
                    .is_some()
            {
                continue;
            }
            if let Some(ty) = self.context.runtime_builtin_value(&name, target) {
                variables.entry(name).or_insert(ty);
            }
        }
    }

    pub(in crate::infrastructure::mypy) fn reversed_tuple_bottom(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        let Expr::Call(call) = expression else {
            return false;
        };
        let Some(signature) = self.callable_expression_in(&call.func, environment) else {
            return false;
        };
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("reversed")
        {
            return false;
        }
        let Some(value) = call.arguments.args.first() else {
            return false;
        };
        self.context.infer_with_calls(value, environment, &|value| self.expression_type(value, environment))
            .is_some_and(|ty| matches!(self.context.node(ty).as_deref(), Some(types::Node::Tuple(items)) if items.is_empty()))
    }
}
