//! Call results share overload binding and preserve builtin result rules.
use super::*;
mod casts;

impl Scopes {
    pub(crate) fn call_return(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        self.call_return_context(call, environment, None)
    }

    pub(crate) fn call_return_context(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> Option<Type> {
        if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "reveal_type") {
            if let [value] = call.arguments.args.as_ref() {
                return self.context.infer_with_calls(value, environment, &|value| {
                    self.expression_type(value, environment)
                });
            }
        }
        if let Some(result) = self.typing_cast_result(call, environment) {
            return Some(result);
        }
        if let Some(returns) = self.dataclass_field_return(call, environment, expected_return) {
            return Some(returns);
        }
        if let Some((signature, _)) = self.dataclass_replace_signature(call, environment) {
            return signature.returns;
        }
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            if attribute.attr.as_str() == "__init__"
                && self
                    .context
                    .infer_with_calls(&attribute.value, environment, &|expression| {
                        self.expression_type(expression, environment)
                    })
                    .is_some_and(|ty| {
                        self.context.class_id(ty).is_some() && !self.context.is_class_value(ty)
                    })
            {
                return Some(Type::Any);
            }
        }
        if let Some(returns) = self
            .context
            .typed_dict_call(call, environment, &|expression| {
                self.expression_type(expression, environment)
            })
        {
            return Some(returns);
        }
        if let Some(signatures) = self
            .context
            .infer_with_calls(&call.func, environment, &|value| {
                self.expression_type(value, environment)
            })
            .and_then(|ty| {
                self.context
                    .type_object_constructors_for(ty, self.context.module_target(&self.namespace))
            })
        {
            let returns = signatures
                .iter()
                .map(|signature| {
                    if signature.overloads.is_empty() {
                        signature.returns.unwrap_or(Type::Any)
                    } else {
                        self.resolve_overload(call, signature, environment, expected_return)
                            .returns
                    }
                })
                .collect();
            return Some(self.context.union(returns));
        }
        if let Some(callee) =
            self.context
                .infer_with_calls(&call.func, environment, &|expression| {
                    self.expression_type(expression, environment)
                })
        {
            if matches!(
                self.context.node(callee).as_deref(),
                Some(types::Node::Union(_))
            ) {
                return self.union_call_return(call, callee, environment, expected_return);
            }
        }
        let Some(signature) = self.callable_expression_in(&call.func, environment) else {
            return self
                .context
                .infer_with_calls(&call.func, environment, &|expression| {
                    self.expression_type(expression, environment)
                })
                .map(|_| Type::Any);
        };
        self.signature_call_return(call, signature, environment, expected_return)
    }

    fn union_call_return(
        &self,
        call: &ExprCall,
        callee: Type,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> Option<Type> {
        if callee.is_any() || callee == Type::Never {
            return Some(callee.derived_any());
        }
        if let Some(node) = self.context.node(callee) {
            if let types::Node::Union(items) = node.as_ref() {
                let returns = items
                    .iter()
                    .map(|&item| self.union_call_return(call, item, environment, expected_return))
                    .collect::<Option<Vec<_>>>()?;
                return Some(self.context.union(returns));
            }
        }
        let target = self.context.module_target(&self.namespace);
        if let Some(signature) = self.context.union_callable_signature_for(callee, target) {
            self.signature_call_return(call, signature, environment, expected_return)
        } else {
            self.context
                .non_callable_rule_for(callee, target)
                .map(|_| Type::Any)
        }
    }

    fn signature_call_return(
        &self,
        call: &ExprCall,
        signature: std::sync::Arc<Signature>,
        environment: &BTreeMap<String, Type>,
        expected_return: Option<Type>,
    ) -> Option<Type> {
        let expanded = self.collection_expanded_call(call, &signature);
        if let Some(result) = self.builtin_value_result(call, &signature, environment) {
            return Some(result);
        }
        let call = expanded.as_deref().unwrap_or(call);
        if !signature.overloads.is_empty() {
            if let Some(expression) = self.dictionary_expression(call, &signature) {
                return self.dictionary_expression_return(
                    &expression,
                    environment,
                    expected_return,
                );
            }
            let returned = self
                .resolve_overload_with_lambda_shape(
                    call,
                    &signature,
                    environment,
                    expected_return,
                    expanded.is_none(),
                )
                .returns;
            return Some(self.context.iteration_return(&signature, returned));
        }
        let returns = signature.returns?;
        if signature.generic.is_empty() {
            return Some(
                self.context
                    .substitute_inferred_alias(returns, &BTreeMap::new()),
            );
        }
        let solution = self.call_solution(call, &signature, environment, expected_return);
        let result = self
            .context
            .substitute_inferred_alias(returns, &solution.types);
        if self.context.is_class_value(result)
            && self
                .context
                .node(returns)
                .is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(_)))
        {
            return self
                .context
                .class_constructor(result)
                .map(|signature| self.context.callable_type(signature));
        }
        Some(result)
    }
}
