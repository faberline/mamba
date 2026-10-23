//! True and false partitions keep scalar false literals and declared method bounds.
use super::*;
use std::collections::BTreeSet;

impl Context {
    pub(in crate::infrastructure::mypy) fn truth_merge(
        &self,
        inputs: Vec<Type>,
        baseline: Option<Type>,
    ) -> Type {
        let output = self.truth_union(inputs);
        let Some(baseline) = baseline else {
            return output;
        };
        let members = |ty| match self.node(ty).as_deref() {
            Some(Node::Union(items)) => items.iter().copied().collect::<BTreeSet<_>>(),
            _ => BTreeSet::from([ty]),
        };
        let output_items = members(output);
        let baseline_items = members(self.truth_union(vec![baseline]));
        if output_items.iter().all(|&item| {
            baseline_items
                .iter()
                .any(|&baseline| self.any_origin_equal(item, baseline))
        }) && baseline_items.iter().all(|&baseline| {
            output_items
                .iter()
                .any(|&item| self.any_origin_equal(item, baseline))
        }) {
            baseline
        } else {
            output
        }
    }
    pub(in crate::infrastructure::mypy) fn truthiness_type(
        &self,
        input: Type,
        positive: bool,
    ) -> Type {
        self.truthiness_inner(input, positive, &mut BTreeSet::new())
    }

    fn truthiness_inner(&self, input: Type, positive: bool, seen: &mut BTreeSet<Type>) -> Type {
        if !seen.insert(input) {
            return input;
        }
        let result = match input {
            Type::Never => input,
            Type::None => {
                if positive {
                    Type::Never
                } else {
                    input
                }
            }
            Type::Bool => self.intern(Node::Literal(Literal::Bool(positive))),
            Type::Int if !positive => self.intern(Node::Literal(Literal::Int(0))),
            Type::Str if !positive => self.intern(Node::Literal(Literal::Str(String::new()))),
            Type::Bytes if !positive => self.intern(Node::Literal(Literal::Bytes(Vec::new()))),
            _ => match self.node(input).as_deref() {
                Some(Node::Union(items)) => self.truth_union(
                    items
                        .iter()
                        .map(|&item| self.truthiness_inner(item, positive, seen))
                        .collect(),
                ),
                Some(Node::Literal(value) | Node::LiteralValue(value)) => {
                    let truth = match value {
                        Literal::Bool(value) => *value,
                        Literal::Int(value) => *value != 0,
                        Literal::Str(value) => !value.is_empty(),
                        Literal::Bytes(value) => !value.is_empty(),
                    };
                    if truth == positive {
                        input
                    } else {
                        Type::Never
                    }
                }
                Some(Node::Tuple(items)) => {
                    if items.is_empty() != positive {
                        input
                    } else {
                        Type::Never
                    }
                }
                Some(
                    Node::Callable(_) | Node::Class(_) | Node::AppliedClass { value: true, .. },
                ) => {
                    if positive {
                        input
                    } else {
                        Type::Never
                    }
                }
                Some(Node::AliasInstance(_, _) | Node::InferredAlias { .. }) => {
                    let expanded = self.expanded_alias_instance(input);
                    if expanded == input {
                        input
                    } else {
                        self.truthiness_inner(expanded, positive, seen)
                    }
                }
                Some(Node::Instance(_) | Node::AppliedClass { value: false, .. }) => {
                    let method = self
                        .class_method(input, "__bool__")
                        .or_else(|| self.class_method(input, "__len__"));
                    if let Some(returns) = method.and_then(|method| method.returns) {
                        if self.truthiness_inner(returns, positive, seen) == Type::Never {
                            Type::Never
                        } else {
                            input
                        }
                    } else if !positive
                        && self
                            .class_id(input)
                            .is_some_and(|id| self.class(id).final_class)
                    {
                        Type::Never
                    } else {
                        input
                    }
                }
                _ => input,
            },
        };
        seen.remove(&input);
        result
    }

    pub(in crate::infrastructure::mypy) fn truth_union(&self, inputs: Vec<Type>) -> Type {
        let result = self.union(inputs);
        let node = self.node(result);
        let Some(Node::Union(items)) = node.as_deref() else {
            return result;
        };
        let mut items = items.clone();
        let literal_bool = |item| match self.node(item).as_deref() {
            Some(
                Node::Literal(Literal::Bool(value)) | Node::LiteralValue(Literal::Bool(value)),
            ) => Some(*value),
            _ => None,
        };
        if items.iter().any(|&item| literal_bool(item) == Some(true))
            && items.iter().any(|&item| literal_bool(item) == Some(false))
        {
            let first = items
                .iter()
                .position(|&item| literal_bool(item).is_some())
                .unwrap();
            items.retain(|&item| literal_bool(item).is_none());
            items.insert(first.min(items.len()), Type::Bool);
        }
        let reduced = items
            .iter()
            .enumerate()
            .filter_map(|(index, &item)| {
                (!items
                    .iter()
                    .enumerate()
                    .any(|(other, &wider)| index != other && self.truth_union_covers(wider, item)))
                .then_some(item)
            })
            .collect();
        self.union(reduced)
    }

    fn truth_union_covers(&self, wider: Type, item: Type) -> bool {
        if let Some(Node::Literal(value) | Node::LiteralValue(value)) = self.node(item).as_deref() {
            return wider == value.base() || wider == Type::Int && value.base() == Type::Bool;
        }
        if wider == Type::Int && item == Type::Bool {
            return true;
        }
        if matches!(
            (self.node(wider).as_deref(), self.node(item).as_deref()),
            (
                Some(Node::Instance(_) | Node::AppliedClass { value: false, .. }),
                Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
            )
        ) {
            return self.accepts(wider, item) && (!self.accepts(item, wider) || wider < item);
        }
        false
    }
}
