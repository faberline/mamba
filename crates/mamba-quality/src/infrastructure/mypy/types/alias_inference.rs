//! Inferred alias values retain their transparent target and application metadata.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn simplify_inference_union(&self, ty: Type) -> Type {
        let expanded = self.expanded_alias_instance(ty);
        let Some(node) = self.node(expanded) else {
            return expanded;
        };
        let Node::Union(items) = node.as_ref() else {
            return expanded;
        };
        let retained = items
            .iter()
            .copied()
            .enumerate()
            .filter(|&(index, item)| {
                item.is_any()
                    || !items
                        .iter()
                        .copied()
                        .enumerate()
                        .any(|(other_index, other)| {
                            other_index != index
                                && !other.is_any()
                                && self.accepts(other, item)
                                && (other_index < index || !self.accepts(item, other))
                        })
            })
            .map(|(_, item)| item)
            .collect();
        self.union(retained)
    }

    pub(in crate::infrastructure::mypy) fn substitute_inferred_alias(
        &self,
        ty: Type,
        solutions: &BTreeMap<Type, Type>,
    ) -> Type {
        let target = self.substitute(ty, solutions);
        let Some((id, arguments)) = self.alias_application(ty) else {
            return target;
        };
        // A nongeneric inferred alias reveals its structural target first.
        // Keep recursive children as alias instances under the existing guard.
        let target = if self.alias(id).parameters.is_empty() {
            self.expanded_alias_instance(target)
        } else {
            target
        };
        let arguments = arguments
            .into_iter()
            .map(|ty| self.substitute(ty, solutions))
            .collect();
        self.intern(Node::InferredAlias {
            id,
            arguments,
            target,
        })
    }

    pub(in crate::infrastructure::mypy) fn unsolved_recursive_alias(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(|node| matches!(node.as_ref(), Node::InferredAlias { id, arguments, .. } if self.alias(*id).modern && arguments.contains(&Type::Never)))
    }

    pub(in crate::infrastructure::mypy) fn recover_unsolved_alias(&self, ty: Type) -> Type {
        if self.unsolved_recursive_alias(ty) {
            self.substitute(ty, &BTreeMap::from([(Type::Never, Type::Any)]))
        } else {
            ty
        }
    }

    pub(in crate::infrastructure::mypy) fn recursive_alias_without_material_parameter(
        &self,
        ty: Type,
    ) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        let Node::InferredAlias { id, .. } = node.as_ref() else {
            return false;
        };
        let alias = self.alias(*id);
        alias.modern
            && !alias.parameters.is_empty()
            && !alias
                .parameters
                .iter()
                .any(|&parameter| self.alias_material_parameter(*id, parameter))
    }

    pub(super) fn alias_material_parameter(&self, id: u32, parameter: Type) -> bool {
        self.alias(id)
            .target
            .is_some_and(|target| self.material_variable(target, parameter))
    }

    fn material_variable(&self, ty: Type, parameter: Type) -> bool {
        if ty == parameter {
            return true;
        }
        let Some(node) = self.node(ty) else {
            return false;
        };
        match node.as_ref() {
            Node::Container(_, arguments)
            | Node::Tuple(arguments)
            | Node::Union(arguments)
            | Node::AppliedClass { arguments, .. } => arguments
                .iter()
                .any(|&ty| self.material_variable(ty, parameter)),
            Node::VariadicTuple(item) => self.material_variable(*item, parameter),
            _ => false,
        }
    }

    pub(in crate::infrastructure::mypy) fn recursive_input_literal(
        &self,
        expression: &Expr,
        actual: Type,
        expected: Type,
    ) -> Type {
        if self.alias_application(expected).is_none() {
            return actual;
        }
        match expression {
            Expr::Tuple(tuple) => {
                if let Some(node) = self.node(actual) {
                    if let Node::Tuple(items) = node.as_ref() {
                        let items = tuple
                            .elts
                            .iter()
                            .zip(items)
                            .map(|(expression, &ty)| {
                                if let Some(node) = self
                                    .literal_expression(expression)
                                    .and_then(|ty| self.node(ty))
                                {
                                    if let Node::Literal(literal) = node.as_ref() {
                                        return self.intern(Node::LiteralValue(literal.clone()));
                                    }
                                }
                                ty
                            })
                            .collect();
                        return self.intern(Node::Tuple(items));
                    }
                }
            }
            Expr::List(list) if list.elts.is_empty() => {
                return self.intern(Node::Container(Container::List, vec![Type::Never]))
            }
            _ => {}
        }
        actual
    }
}
