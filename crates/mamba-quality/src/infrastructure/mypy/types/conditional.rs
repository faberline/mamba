//! Short-circuit values and conditional arms use their own input refinements.
use super::*;
use crate::infrastructure::mypy::scopes::Scopes;
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, CmpOp, ExprCall, ExprIf, UnaryOp,
};

// Unproven does not mean that refinement succeeded or that the branch is reachable.
// Only this refinement's resolved predicate Never receipt proves it unreachable.
pub(in crate::infrastructure::mypy) enum ConditionalBranch {
    Unproven,
    Unreachable,
}

impl Context {
    pub(super) fn infer_conditional(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        calls: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Named(named) => self.infer_with_calls(&named.value, environment, calls),
            Expr::BoolOp(boolean) => {
                let positive = boolean.op == BoolOp::And;
                let mut branch = environment.clone();
                let mut results = Vec::new();
                for (index, value) in boolean.values.iter().enumerate() {
                    let ty = self.infer_with_calls(value, &branch, calls)?;
                    if index + 1 == boolean.values.len() {
                        results.push(self.conditional_literal(value, ty));
                    } else {
                        results.push(self.truthiness_type(ty, !positive));
                        if self.truthiness_type(ty, positive) == Type::Never {
                            break;
                        }
                        self.refine_condition_environment(value, positive, &mut branch, calls);
                    }
                }
                Some(self.truth_union(results))
            }
            Expr::If(conditional) => self.infer_conditional_arms(
                conditional,
                environment,
                &|value, branch| self.infer_with_calls(value, branch, calls),
                &|test, positive, branch| {
                    self.refine_condition_environment(test, positive, branch, calls);
                    ConditionalBranch::Unproven
                },
            ),
            _ => None,
        }
    }

    // Only the caller that supplies Scope's If resolver uses this entry.
    // Once If is selected, an unknown result must not replay the condition.
    pub(in crate::infrastructure::mypy) fn infer_with_scoped_conditional(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        calls: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        if matches!(expression, Expr::If(_)) {
            if let Some(observed) = self.observed_expression_type(expression) {
                return observed;
            }
            return calls(expression);
        }
        self.infer_with_calls(expression, environment, calls)
    }

    pub(in crate::infrastructure::mypy) fn infer_conditional_arms(
        &self,
        conditional: &ExprIf,
        environment: &BTreeMap<String, Type>,
        infer: &impl Fn(&Expr, &BTreeMap<String, Type>) -> Option<Type>,
        refine: &impl Fn(&Expr, bool, &mut BTreeMap<String, Type>) -> ConditionalBranch,
    ) -> Option<Type> {
        let condition = infer(&conditional.test, environment);
        let mut results = Vec::new();
        let mut dictionary_shapes = [(false, false); 2];
        let mut list_shapes = [(false, false); 2];
        for (positive, value) in [(true, &conditional.body), (false, &conditional.orelse)] {
            if condition
                .is_some_and(|condition| self.truthiness_type(condition, positive) == Type::Never)
            {
                continue;
            }
            let mut branch = environment.clone();
            if matches!(
                refine(&conditional.test, positive, &mut branch),
                ConditionalBranch::Unreachable
            ) {
                continue;
            }
            let input = infer(value, &branch)?;
            let literal = self.conditional_literal(value, input);
            let node = self.node(literal);
            let dictionary = match node.as_deref() {
                Some(Node::Container(Container::Dict, arguments))
                    if literal == input && arguments.len() == 2 =>
                {
                    Some(arguments.as_slice())
                }
                _ => None,
            };
            dictionary_shapes[results.len()] = (
                dictionary.is_some_and(|arguments| {
                    arguments.len() == 2 && arguments.iter().all(|ty| ty.is_any())
                }),
                dictionary.is_some_and(|arguments| {
                    arguments
                        .iter()
                        .all(|&ty| !matches!(ty, Type::Any | Type::AnySpecial | Type::Never))
                }),
            );
            let list = match node.as_deref() {
                Some(Node::Container(Container::List, arguments))
                    if literal == input && arguments.len() == 1 =>
                {
                    Some(arguments.as_slice())
                }
                _ => None,
            };
            list_shapes[results.len()] = (
                list.is_some_and(|arguments| arguments.len() == 1 && arguments[0].is_any()),
                list.is_some_and(|arguments| {
                    !matches!(arguments[0], Type::Any | Type::AnySpecial | Type::Never)
                }),
            );
            let arm = match node.as_deref() {
                Some(Node::LiteralValue(value)) if value.base() == input => literal,
                _ => input,
            };
            results.push(arm);
        }
        if results.len() == 2 {
            let empty_dict = |value: &Expr| matches!(value, Expr::Dict(dictionary) if dictionary.items.is_empty());
            let body_empty = empty_dict(&conditional.body);
            let else_empty = empty_dict(&conditional.orelse);
            if body_empty != else_empty {
                let empty = usize::from(else_empty);
                let other = 1 - empty;
                // A fresh empty literal has no entries to recheck. Reuse only
                // the already inferred compact dict, never an arbitrary Any arm.
                if dictionary_shapes[empty].0 && dictionary_shapes[other].1 {
                    let empty_expression = if body_empty {
                        &conditional.body
                    } else {
                        &conditional.orelse
                    };
                    // A checked value, including a checked unknown, is authoritative.
                    if self.observed_expression_type(empty_expression).is_none() {
                        results[empty] = results[other];
                    }
                }
            }
            let empty_list =
                |value: &Expr| matches!(value, Expr::List(list) if list.elts.is_empty());
            let body_empty = empty_list(&conditional.body);
            let else_empty = empty_list(&conditional.orelse);
            if body_empty != else_empty {
                let empty = usize::from(else_empty);
                let other = 1 - empty;
                // Only an unobserved empty list can reuse its sibling's item type.
                if list_shapes[empty].0 && list_shapes[other].1 {
                    let empty_expression = if body_empty {
                        &conditional.body
                    } else {
                        &conditional.orelse
                    };
                    if self.observed_expression_type(empty_expression).is_none() {
                        results[empty] = results[other];
                    }
                }
            }
        }
        Some(self.truth_union(results))
    }

    fn conditional_literal(&self, expression: &Expr, input: Type) -> Type {
        let literal = self
            .literal_expression(expression)
            .and_then(|literal| self.node(literal));
        if let Some(Node::Literal(value)) = literal.as_deref() {
            self.intern(Node::LiteralValue(value.clone()))
        } else {
            input
        }
    }

    fn refine_condition_environment(
        &self,
        expression: &Expr,
        positive: bool,
        environment: &mut BTreeMap<String, Type>,
        calls: &impl Fn(&Expr) -> Option<Type>,
    ) {
        self.refine_condition_environment_with(
            expression,
            positive,
            environment,
            &|value, branch| self.infer_with_calls(value, branch, calls),
            &|_, _, _| (false, None),
        );
    }

    pub(in crate::infrastructure::mypy) fn refine_condition_environment_with(
        &self,
        expression: &Expr,
        positive: bool,
        environment: &mut BTreeMap<String, Type>,
        infer: &impl Fn(&Expr, &BTreeMap<String, Type>) -> Option<Type>,
        predicate: &impl Fn(&ExprCall, bool, &BTreeMap<String, Type>) -> (bool, Option<(String, Type)>),
    ) -> ConditionalBranch {
        if let Expr::UnaryOp(unary) = expression {
            if unary.op == UnaryOp::Not {
                return self.refine_condition_environment_with(
                    &unary.operand,
                    !positive,
                    environment,
                    infer,
                    predicate,
                );
            }
        }
        if let Expr::BoolOp(boolean) = expression {
            if (boolean.op == BoolOp::And) == positive {
                for value in &boolean.values {
                    if matches!(
                        self.refine_condition_environment_with(
                            value,
                            positive,
                            environment,
                            infer,
                            predicate,
                        ),
                        ConditionalBranch::Unreachable
                    ) {
                        return ConditionalBranch::Unreachable;
                    }
                }
            }
            return ConditionalBranch::Unproven;
        }
        if let Expr::Call(call) = expression {
            let (recognized, refinement) = predicate(call, positive, environment);
            if recognized {
                if let Some((key, narrowed)) = refinement {
                    environment.insert(key, narrowed);
                    if narrowed == Type::Never {
                        return ConditionalBranch::Unreachable;
                    }
                }
                return ConditionalBranch::Unproven;
            }
        }
        if let Expr::Named(named) = expression {
            if let Some(input) = infer(&named.value, environment) {
                let narrowed = self.truthiness_type(input, positive);
                if let Some(key) = Scopes::attribute_key(&named.target) {
                    environment.insert(key, narrowed);
                }
                if let Some(key) = Scopes::attribute_key(&named.value) {
                    environment.insert(key, narrowed);
                }
            }
            return ConditionalBranch::Unproven;
        }
        if let Expr::Compare(compare) = expression {
            if compare.operands.len() == 2 {
                let pair = match (&compare.operands[0], &compare.operands[1]) {
                    (value, Expr::NoneLiteral(_)) | (Expr::NoneLiteral(_), value) => Some(value),
                    _ => None,
                };
                if let Some(value) = pair {
                    if let (Some(key), Some(input), Some(op)) = (
                        Scopes::attribute_key(value),
                        infer(value, environment),
                        compare.ops.first(),
                    ) {
                        if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                            environment.insert(
                                key,
                                self.narrow(input, Type::None, positive == (*op == CmpOp::Is)),
                            );
                            return ConditionalBranch::Unproven;
                        }
                    }
                }
            }
        }
        if let (Some(key), Some(input)) = (
            Scopes::attribute_key(expression),
            infer(expression, environment),
        ) {
            environment.insert(key, self.truthiness_type(input, positive));
        }
        ConditionalBranch::Unproven
    }
}
