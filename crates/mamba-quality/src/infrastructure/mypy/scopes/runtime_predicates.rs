//! Runtime predicates share resolved identity across branch and comprehension inference.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, ExprCall, Operator, UnaryOp,
};

impl Scopes {
    pub(in crate::infrastructure::mypy) fn runtime_predicate_name(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<&'static str> {
        let signature = self.callable_expression_in(expression, environment)?;
        if signature.definition_module.as_deref() != Some("builtins") {
            return None;
        }
        if let Expr::Name(name) = expression {
            if self.resolved_binding(name.id.as_str()).is_some()
                && self.callable(name.id.as_str()).is_none()
            {
                return None;
            }
        }
        match signature.name.as_deref()? {
            "isinstance" => Some("isinstance"),
            "issubclass" => Some("issubclass"),
            _ => None,
        }
    }

    fn runtime_predicate_targets(
        &self,
        expression: &Expr,
        infer: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Vec<(Type, bool)>> {
        if let Expr::Tuple(tuple) = expression {
            let mut targets = Vec::new();
            for item in &tuple.elts {
                targets.extend(self.runtime_predicate_targets(item, infer)?);
            }
            return Some(targets);
        }
        if let Expr::BinOp(binary) = expression {
            if binary.op == Operator::BitOr {
                let mut targets = self.runtime_predicate_targets(&binary.left, infer)?;
                targets.extend(self.runtime_predicate_targets(&binary.right, infer)?);
                return Some(targets);
            }
        }
        if expression.is_none_literal_expr() {
            return Some(vec![(Type::None, false)]);
        }
        self.context.runtime_type_ranges(infer(expression)?)
    }

    pub(in crate::infrastructure::mypy) fn runtime_predicate_refinement(
        &self,
        call: &ExprCall,
        positive: bool,
        environment: &BTreeMap<String, Type>,
        infer: &impl Fn(&Expr) -> Option<Type>,
    ) -> (bool, Option<(String, Type)>) {
        let Some(name) = self.runtime_predicate_name(&call.func, environment) else {
            return (false, None);
        };
        let refinement = (|| {
            let argument = call.arguments.args.first()?;
            let target = call.arguments.args.get(1).or_else(|| {
                call.arguments
                    .keywords
                    .first()
                    .map(|keyword| &keyword.value)
            })?;
            let argument = if let Expr::Named(named) = argument {
                named.target.as_ref()
            } else {
                argument
            };
            let key = Self::attribute_key(argument)?;
            let input = infer(argument)?;
            let targets = self.runtime_predicate_targets(target, infer)?;
            let module = self.qualified_name("");
            let narrowed = if name == "issubclass" {
                self.context.runtime_class_narrow(
                    input,
                    &targets,
                    positive,
                    module.trim_end_matches('.'),
                )
            } else {
                self.context.runtime_instance_narrow(
                    input,
                    &targets,
                    positive,
                    module.trim_end_matches('.'),
                )
            };
            Some((key, narrowed))
        })();
        (true, refinement)
    }

    pub(super) fn refine_comprehension_filter(
        &self,
        expression: &Expr,
        positive: bool,
        environment: &mut BTreeMap<String, Type>,
    ) -> bool {
        if let Expr::UnaryOp(unary) = expression {
            if unary.op == UnaryOp::Not {
                return self.refine_comprehension_filter(&unary.operand, !positive, environment);
            }
        }
        if let Expr::BoolOp(boolean) = expression {
            if (boolean.op == BoolOp::And) == positive {
                return boolean
                    .values
                    .iter()
                    .all(|value| self.refine_comprehension_filter(value, positive, environment));
            }
            let baseline = environment.clone();
            let mut remaining = baseline.clone();
            let mut outcomes = Vec::new();
            for value in &boolean.values {
                let mut branch = remaining.clone();
                if self.refine_comprehension_filter(value, positive, &mut branch) {
                    outcomes.push(branch);
                }
                if !self.refine_comprehension_filter(value, !positive, &mut remaining) {
                    break;
                }
            }
            if outcomes.is_empty() {
                return false;
            }
            let names: std::collections::BTreeSet<_> = outcomes
                .iter()
                .flat_map(|branch| branch.keys().cloned())
                .collect();
            *environment = baseline.clone();
            for name in names {
                let types: Option<Vec<_>> = outcomes
                    .iter()
                    .map(|branch| branch.get(&name).or_else(|| baseline.get(&name)).copied())
                    .collect();
                if let Some(types) = types {
                    environment.insert(
                        name.clone(),
                        self.context
                            .truth_merge(types, baseline.get(&name).copied()),
                    );
                }
            }
            return true;
        }
        if let Expr::Call(call) = expression {
            let (_, refinement) =
                self.runtime_predicate_refinement(call, positive, environment, &|value| {
                    self.context.infer_with_calls(value, environment, &|value| {
                        self.expression_type(value, environment)
                    })
                });
            if let Some((name, ty)) = refinement {
                environment.insert(name, ty);
                return ty != Type::Never;
            }
        }
        let input = self.context.literal_expression(expression).or_else(|| {
            self.context
                .infer_with_calls(expression, environment, &|value| {
                    self.expression_type(value, environment)
                })
        });
        let Some(input) = input else {
            return true;
        };
        let narrowed = self.context.truthiness_type(input, positive);
        if let Some(name) = Self::attribute_key(expression) {
            environment.insert(name, narrowed);
        }
        narrowed != Type::Never
    }
}
