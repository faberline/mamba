//! Embedded sequence equality and containment have known boolean results.
use super::*;
use mamba_language::application::quality_syntax::python_ast::CmpOp;

impl Context {
    pub(in crate::infrastructure::mypy) fn embedded_sequence(&self, ty: Type) -> bool {
        !self.is_class_value(ty)
            && self
                .class_id(ty)
                .is_some_and(|id| self.class(id).namespace == "builtins.range")
    }

    pub(in crate::infrastructure::mypy) fn sequence_comparison(
        &self,
        left: Type,
        right: Type,
        operator: CmpOp,
    ) -> Option<comparisons::Comparison> {
        if !self.embedded_sequence(left) && !self.embedded_sequence(right) {
            return None;
        }
        let symbol = match operator {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            _ => return None,
        };
        let returns = if matches!(operator, CmpOp::Eq | CmpOp::NotEq) {
            if left.is_any() || right.is_any() {
                Type::Any
            } else if !self.is_class_value(left) {
                self.class_method(
                    left,
                    if operator == CmpOp::Eq {
                        "__eq__"
                    } else {
                        "__ne__"
                    },
                )
                .and_then(|signature| signature.returns)
                .unwrap_or(Type::Bool)
            } else {
                Type::Bool
            }
        } else {
            Type::Bool
        };
        Some(comparisons::Comparison {
            returns,
            failure: None,
            symbol,
            left,
            right,
        })
    }

    pub(in crate::infrastructure::mypy) fn type_overlap(&self, left: Type, right: Type) -> bool {
        self.type_overlap_inner(left, right, &mut std::collections::BTreeSet::new())
    }

    fn type_overlap_inner(
        &self,
        left: Type,
        right: Type,
        active: &mut std::collections::BTreeSet<(Type, Type)>,
    ) -> bool {
        if !active.insert((left, right)) {
            return true;
        }
        let result = self.type_overlap_domain(left, right, active);
        active.remove(&(left, right));
        result
    }

    fn type_overlap_domain(
        &self,
        left: Type,
        right: Type,
        active: &mut std::collections::BTreeSet<(Type, Type)>,
    ) -> bool {
        if self.accepts(left, right) || self.accepts(right, left) {
            return true;
        }
        if let Some(Node::Union(items)) = self.node(left).as_deref() {
            return items
                .iter()
                .any(|&item| self.type_overlap_inner(item, right, active));
        }
        if let Some(Node::Union(items)) = self.node(right).as_deref() {
            return items
                .iter()
                .any(|&item| self.type_overlap_inner(left, item, active));
        }
        if let Some(Node::Variable(variable)) = self.node(left).as_deref() {
            return if variable.values.is_empty() {
                self.type_overlap_inner(variable.bound, right, active)
            } else {
                variable
                    .values
                    .iter()
                    .any(|&item| self.type_overlap_inner(item, right, active))
            };
        }
        if let Some(Node::Variable(variable)) = self.node(right).as_deref() {
            return if variable.values.is_empty() {
                self.type_overlap_inner(left, variable.bound, active)
            } else {
                variable
                    .values
                    .iter()
                    .any(|&item| self.type_overlap_inner(left, item, active))
            };
        }
        false
    }

    pub(in crate::infrastructure::mypy) fn custom_equality(&self, ty: Type) -> bool {
        if self.is_class_value(ty) {
            return false;
        }
        let Some(id) = self.class_id(ty) else {
            return false;
        };
        self.class_lineage(id).into_iter().any(|id| {
            let class = self.class(id);
            class.namespace != "<builtins>"
                && !class.namespace.starts_with("builtins.")
                && class.methods.contains_key("__eq__")
        })
    }
    pub(super) fn receiver_overlap(
        &self,
        left: Type,
        right: Type,
        active: &mut std::collections::BTreeSet<(Type, Type)>,
    ) -> Option<bool> {
        let (left, right) = (
            self.expanded_alias_instance(left),
            self.expanded_alias_instance(right),
        );
        if self.is_alias_instance(left) || self.is_alias_instance(right) {
            return None;
        }
        if self.type_overlap(left, right) || !active.insert((left, right)) {
            return Some(true);
        }
        let result = (|| {
            let any = |items: &[Type], other, active: &mut std::collections::BTreeSet<_>| {
                let mut unknown = false;
                for &item in items {
                    match self.receiver_overlap(item, other, active) {
                        Some(true) => return Some(true),
                        None => unknown = true,
                        _ => {}
                    }
                }
                (!unknown).then_some(false)
            };
            let pair =
                |left: &[Type], right: &[Type], active: &mut std::collections::BTreeSet<_>| {
                    if left.len() != right.len() {
                        return Some(false);
                    }
                    let mut unknown = false;
                    for (&left, &right) in left.iter().zip(right) {
                        match self.receiver_overlap(left, right, active) {
                            Some(false) => return Some(false),
                            None => unknown = true,
                            _ => {}
                        }
                    }
                    (!unknown).then_some(true)
                };
            match (self.node(left).as_deref(), self.node(right).as_deref()) {
                (Some(Node::Union(items)), _) => return any(items, right, active),
                (_, Some(Node::Union(items))) => return any(items, left, active),
                (Some(Node::Variable(variable)), _) => {
                    return if variable.values.is_empty() {
                        self.receiver_overlap(variable.bound, right, active)
                    } else {
                        any(&variable.values, right, active)
                    };
                }
                (_, Some(Node::Variable(variable))) => {
                    return if variable.values.is_empty() {
                        self.receiver_overlap(left, variable.bound, active)
                    } else {
                        any(&variable.values, left, active)
                    };
                }
                (
                    Some(Node::Container(left_kind, left)),
                    Some(Node::Container(right_kind, right)),
                ) if left_kind == right_kind => return pair(left, right, active),
                (Some(Node::Tuple(left)), Some(Node::Tuple(right))) => {
                    return pair(left, right, active)
                }
                (Some(Node::VariadicTuple(left)), Some(Node::VariadicTuple(right))) => {
                    return self.receiver_overlap(*left, *right, active);
                }
                (Some(Node::Tuple(items)), Some(Node::VariadicTuple(item)))
                | (Some(Node::VariadicTuple(item)), Some(Node::Tuple(items))) => {
                    return pair(items, &vec![*item; items.len()], active);
                }
                (Some(Node::Callable(_) | Node::Parameters(_, _) | Node::Unpack(_)), _)
                | (_, Some(Node::Callable(_) | Node::Parameters(_, _) | Node::Unpack(_))) => {
                    return None
                }
                _ => {}
            }
            if let (Some(left_id), Some(right_id)) = (self.class_id(left), self.class_id(right)) {
                if self.is_class_value(left) != self.is_class_value(right) {
                    return None;
                }
                let owner = if self.nominal_accepts(left_id, right_id) {
                    left_id
                } else if self.nominal_accepts(right_id, left_id) {
                    right_id
                } else {
                    return Some(false);
                };
                if self.class(owner).typed_dict.is_some() || self.class(owner).protocol {
                    return None;
                }
                return pair(
                    &self.class_arguments_for(left, owner)?,
                    &self.class_arguments_for(right, owner)?,
                    active,
                );
            }
            // Different compact/nominal representations need their declared
            // fallback relationship; absence here is not disjointness evidence.
            match (self.node(left).as_deref(), self.node(right).as_deref()) {
                (
                    Some(Node::Container(_, _)),
                    Some(Node::AppliedClass { .. } | Node::Instance(_)),
                )
                | (
                    Some(Node::AppliedClass { .. } | Node::Instance(_)),
                    Some(Node::Container(_, _)),
                ) => None,
                _ => Some(false),
            }
        })();
        active.remove(&(left, right));
        result
    }
}
