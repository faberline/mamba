//! Lambda and comprehension bindings never leak into the enclosing scope.
use super::*;
use quality_syntax::python_ast::Comprehension;
use types::Type;

impl Checker<'_> {
    pub(super) fn visit_name_scope<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        if let Expr::Lambda(lambda) = expression {
            let signature = self
                .infer(expression)
                .and_then(|ty| self.context.callable_signature(ty));
            if let Some(parameters) = &lambda.parameters {
                for parameter in parameters.iter_non_variadic_params() {
                    if let Some(default) = parameter.default.as_deref() {
                        self.visit_expr(default);
                    }
                }
            }
            let environment = if let Some(signature) = signature {
                signature
                    .parameters
                    .iter()
                    .map(|parameter| (parameter.name.clone(), parameter.ty))
                    .collect()
            } else {
                // Lexical bindings do not require inferred parameter types.
                lambda
                    .parameters
                    .as_ref()
                    .map_or_else(BTreeMap::new, |parameters| {
                        parameters
                            .iter()
                            .map(|parameter| (parameter.name().to_string(), None))
                            .collect()
                    })
            };
            let previous_checked_owner = self.checked_expression_owner.take();
            let previous_checked_types = self.checked_expression_types.take();
            let previous_checked_contexts = std::mem::take(&mut self.checked_expression_contexts);
            self.scopes.enter(
                scopes::Kind::Function,
                &[],
                environment,
                self.scopes.variables(),
            );
            let previous_inputs = std::mem::take(&mut self.try_inputs);
            let previous_flow = std::mem::take(&mut self.flow);
            let previous_expected = self.expected_value.take();
            let previous_none = self.none_allowed.replace(lambda.body.range());
            self.visit_expr(&lambda.body);
            self.none_allowed = previous_none;
            self.flow = previous_flow;
            self.try_inputs = previous_inputs;
            self.expected_value = previous_expected;
            self.scopes.leave();
            self.checked_expression_owner = previous_checked_owner;
            self.checked_expression_types = previous_checked_types;
            self.checked_expression_contexts = previous_checked_contexts;
            return true;
        }
        let (generators, values): (&[Comprehension], Vec<&Expr>) = match expression {
            Expr::Generator(generator) => (&generator.generators, vec![&generator.elt]),
            Expr::ListComp(list) => (&list.generators, vec![&list.elt]),
            Expr::SetComp(set) => (&set.generators, vec![&set.elt]),
            Expr::DictComp(dictionary) => {
                let mut values = vec![dictionary.value.as_ref()];
                if let Some(key) = dictionary.key.as_deref() {
                    values.insert(0, key);
                }
                (&dictionary.generators, values)
            }
            _ => return false,
        };
        let Some(first) = generators.first() else {
            return false;
        };
        self.visit_expr(&first.iter);
        let first_item = self
            .infer(&first.iter)
            .and_then(|ty| self.context.iterable_item(ty))
            .unwrap_or(Type::Any);
        let outer_bindings: BTreeMap<_, _> = self
            .flow
            .narrowed
            .keys()
            .filter_map(|key| {
                let name = key.split(['.', '[']).next()?;
                if self.flow.deleted.contains(name) {
                    return None;
                }
                self.scopes
                    .comprehension_binding_identity(key)
                    .map(|identity| (key.clone(), identity))
            })
            .collect();
        // Walrus redirects need an outer declaration-view adapter. Do not send
        // an inner comprehension Flow to an enclosing try input.
        self.refuse_try_inputs();
        let previous_inputs = std::mem::take(&mut self.try_inputs);
        let previous_checked_owner = self.checked_expression_owner.take();
        let previous_checked_types = self.checked_expression_types.take();
        let previous_checked_contexts = std::mem::take(&mut self.checked_expression_contexts);
        self.scopes.enter_comprehension(generators);
        let previous_flow = std::mem::take(&mut self.flow);
        self.flow.narrowed.extend(
            previous_flow
                .narrowed
                .iter()
                .filter(|(key, _)| {
                    outer_bindings.get(*key).is_some_and(|&identity| {
                        self.scopes.comprehension_binding_identity(key) == Some(identity)
                    })
                })
                .map(|(key, &ty)| (key.clone(), ty)),
        );
        let previous_expected = self.expected_value.take();
        for (index, generator) in generators.iter().enumerate() {
            let item = if index == 0 {
                first_item
            } else {
                self.visit_expr(&generator.iter);
                self.infer(&generator.iter)
                    .and_then(|ty| self.context.iterable_item(ty))
                    .unwrap_or(Type::Any)
            };
            self.check_fixed_tuple_unpack_count(
                &generator.target,
                Some(item),
                &ruff_text_size::TextRange::at(
                    expression.start(),
                    ruff_text_size::TextSize::new(1),
                ),
            );
            let bindings = self
                .scopes
                .bind_comprehension_target(&generator.target, item);
            for name in bindings.keys() {
                scopes::Scopes::clear_comprehension_binding(&mut self.flow.narrowed, name);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
                let previous = self.flow.clone();
                self.flow_condition(condition, true);
                if !self.flow.reachable {
                    // An impossible filter does not suppress expression checks.
                    self.flow = previous;
                }
            }
        }
        for value in values {
            self.visit_expr(value);
        }
        self.flow = previous_flow;
        self.try_inputs = previous_inputs;
        self.expected_value = previous_expected;
        self.scopes.leave();
        self.checked_expression_owner = previous_checked_owner;
        self.checked_expression_types = previous_checked_types;
        self.checked_expression_contexts = previous_checked_contexts;
        true
    }
}
