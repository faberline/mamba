//! Parameter bounds combine callable domains with their original join and meet rules.
use super::super::signatures::{Kind, Parameter};
use super::*;

impl Context {
    pub(super) fn join_parameter_bounds(&self, left: Type, right: Type) -> Type {
        if left == right {
            return left;
        }
        let (Some(left_node), Some(right_node)) = (self.node(left), self.node(right)) else {
            return self.join(left, right);
        };
        let (Node::Parameters(left, _), Node::Parameters(right, ellipsis)) =
            (left_node.as_ref(), right_node.as_ref())
        else {
            return self.join(left, right);
        };
        let minimum = |parameters: &[Parameter]| {
            parameters
                .iter()
                .filter(|parameter| {
                    parameter.required
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOnly | Kind::PositionalOrKeyword
                        )
                })
                .count()
        };
        let variadic = |parameters: &[Parameter]| {
            parameters
                .iter()
                .any(|parameter| parameter.kind == Kind::VariadicPositional)
        };
        if left.len() != right.len()
            || minimum(left) != minimum(right)
            || variadic(left) != variadic(right)
        {
            return Type::Any;
        }
        self.intern(Node::Parameters(
            left.iter()
                .zip(right)
                .map(|(left, right)| {
                    let mut parameter = right.clone();
                    parameter.ty = Some(self.meet_parameter_types(
                        left.ty.unwrap_or(Type::Any),
                        right.ty.unwrap_or(Type::Any),
                    ));
                    let left_name =
                        (left.kind != Kind::PositionalOnly).then_some(left.name.as_str());
                    let right_name =
                        (right.kind != Kind::PositionalOnly).then_some(right.name.as_str());
                    if left_name != right_name
                        && left.kind != Kind::KeywordOnly
                        && right.kind != Kind::KeywordOnly
                    {
                        parameter.name.clear();
                        if matches!(
                            parameter.kind,
                            Kind::PositionalOnly | Kind::PositionalOrKeyword
                        ) {
                            parameter.kind = Kind::PositionalOnly;
                        }
                    }
                    parameter
                })
                .collect(),
            *ellipsis,
        ))
    }

    pub(super) fn meet_parameter_bounds(&self, left: Type, right: Type) -> Type {
        if left == right {
            return left;
        }
        let (Some(left_node), Some(right_node)) = (self.node(left), self.node(right)) else {
            return self.meet_parameter_types(left, right);
        };
        let (Node::Parameters(left, _), Node::Parameters(right, ellipsis)) =
            (left_node.as_ref(), right_node.as_ref())
        else {
            return self.meet_parameter_types(left, right);
        };
        if left.len() != right.len() {
            return Type::Never;
        }
        self.intern(Node::Parameters(
            left.iter()
                .zip(right)
                .map(|(left, right)| Parameter {
                    ty: Some(
                        self.join(left.ty.unwrap_or(Type::Any), right.ty.unwrap_or(Type::Any)),
                    ),
                    ..right.clone()
                })
                .collect(),
            *ellipsis,
        ))
    }

    pub(super) fn meet_parameter_types(&self, left: Type, right: Type) -> Type {
        if (self.has_special_any(left) || self.has_special_any(right))
            && self.any_origin_equal(left, right)
        {
            return left;
        }
        if left == Type::AnySpecial || right == Type::AnySpecial {
            if let Some(result) = self.special_any_meet(left, right) {
                return result;
            }
            // Alias/opaque meet policy is not proved here. Keep legacy Any.
            self.mark_unsupported_any_origin();
            return Type::Any;
        }
        if left.is_any() || right.is_any() {
            return Type::Any;
        }
        if self.accepts(left, right) {
            return right;
        }
        if self.accepts(right, left) {
            return left;
        }
        if let Some(Node::Union(items)) = self.node(left).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| self.meet_parameter_types(item, right))
                    .collect(),
            );
        }
        if let Some(Node::Union(items)) = self.node(right).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| self.meet_parameter_types(left, item))
                    .collect(),
            );
        }
        Type::Never
    }
}
