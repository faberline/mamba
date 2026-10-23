//! Any origin identity is separate from value equality and native fallback policy.
use super::super::signatures::{Parameter, Signature};
use super::*;
use std::collections::BTreeSet;

impl Context {
    // This receipt is sticky within one Context. It records an unproved result,
    // not a source location or a successful restoration of original behavior.
    pub(in crate::infrastructure::mypy) fn has_unsupported_any_origin(&self) -> bool {
        self.unsupported_any_origin.get()
    }

    pub(super) fn mark_unsupported_any_origin(&self) {
        self.unsupported_any_origin.set(true);
    }

    // Erase only Any's origin for semantic equality. Keep every other native
    // field, class/alias/variable identity and collection order unchanged.
    // No graph allocation, alias expansion, source lookup or substitution occurs.
    pub(in crate::infrastructure::mypy) fn any_origin_equal(
        &self,
        left: Type,
        right: Type,
    ) -> bool {
        self.any_origin_equal_inner(left, right, &mut BTreeSet::new())
    }

    fn any_origin_equal_inner(
        &self,
        left: Type,
        right: Type,
        active: &mut BTreeSet<(Type, Type)>,
    ) -> bool {
        if left == right || left.is_any() && right.is_any() {
            return true;
        }
        if !active.insert((left, right)) {
            // An unsupported recursive pair cannot prove equality.
            return false;
        }
        let left_node = self.node(left);
        let right_node = self.node(right);
        let result = match (left_node.as_deref(), right_node.as_deref()) {
            (Some(Node::Container(a, left)), Some(Node::Container(b, right))) => {
                a == b && self.any_origin_items_equal(left, right, active)
            }
            (Some(Node::Tuple(left)), Some(Node::Tuple(right)))
            | (Some(Node::Union(left)), Some(Node::Union(right))) => {
                self.any_origin_items_equal(left, right, active)
            }
            (Some(Node::VariadicTuple(a)), Some(Node::VariadicTuple(b)))
            | (Some(Node::Unpack(a)), Some(Node::Unpack(b)))
            | (Some(Node::ParamSpecArgs(a)), Some(Node::ParamSpecArgs(b)))
            | (Some(Node::ParamSpecKwargs(a)), Some(Node::ParamSpecKwargs(b))) => {
                self.any_origin_equal_inner(*a, *b, active)
            }
            (
                Some(Node::AppliedClass {
                    id: a,
                    arguments: left,
                    value: av,
                }),
                Some(Node::AppliedClass {
                    id: b,
                    arguments: right,
                    value: bv,
                }),
            ) => a == b && av == bv && self.any_origin_items_equal(left, right, active),
            (Some(Node::AliasInstance(a, left)), Some(Node::AliasInstance(b, right))) => {
                a == b && self.any_origin_items_equal(left, right, active)
            }
            (
                Some(Node::InferredAlias {
                    id: a,
                    arguments: left,
                    target: at,
                }),
                Some(Node::InferredAlias {
                    id: b,
                    arguments: right,
                    target: bt,
                }),
            ) => {
                a == b
                    && self.any_origin_items_equal(left, right, active)
                    && self.any_origin_equal_inner(*at, *bt, active)
            }
            (Some(Node::Alias(a, left)), Some(Node::Alias(b, right))) => {
                a == b && self.any_origin_optional_equal(*left, *right, active)
            }
            (Some(Node::Parameters(left, a)), Some(Node::Parameters(right, b))) => {
                a == b && self.any_origin_parameters_equal(left, right, active)
            }
            (Some(Node::Callable(left)), Some(Node::Callable(right))) => {
                self.any_origin_signatures_equal(left, right, active)
            }
            (Some(Node::Variable(left)), Some(Node::Variable(right))) => {
                let mut a = left.clone();
                let mut b = right.clone();
                a.bound = Type::Object;
                b.bound = Type::Object;
                a.values.clear();
                b.values.clear();
                a.default = None;
                b.default = None;
                a == b
                    && self.any_origin_equal_inner(left.bound, right.bound, active)
                    && self.any_origin_items_equal(&left.values, &right.values, active)
                    && self.any_origin_optional_equal(left.default, right.default, active)
            }
            (Some(left), Some(right)) => left == right,
            _ => false,
        };
        active.remove(&(left, right));
        result
    }

    fn any_origin_items_equal(
        &self,
        left: &[Type],
        right: &[Type],
        active: &mut BTreeSet<(Type, Type)>,
    ) -> bool {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(&a, &b)| self.any_origin_equal_inner(a, b, active))
    }

    fn any_origin_optional_equal(
        &self,
        left: Option<Type>,
        right: Option<Type>,
        active: &mut BTreeSet<(Type, Type)>,
    ) -> bool {
        match (left, right) {
            (Some(a), Some(b)) => self.any_origin_equal_inner(a, b, active),
            (None, None) => true,
            _ => false,
        }
    }

    fn any_origin_parameters_equal(
        &self,
        left: &[Parameter],
        right: &[Parameter],
        active: &mut BTreeSet<(Type, Type)>,
    ) -> bool {
        left.len() == right.len()
            && left.iter().zip(right).all(|(a, b)| {
                a.name == b.name
                    && a.kind == b.kind
                    && a.required == b.required
                    && self.any_origin_optional_equal(a.ty, b.ty, active)
            })
    }

    fn any_origin_signatures_equal(
        &self,
        left: &Signature,
        right: &Signature,
        active: &mut BTreeSet<(Type, Type)>,
    ) -> bool {
        let mut a = left.clone();
        let mut b = right.clone();
        a.parameters = Arc::from([]);
        b.parameters = Arc::from([]);
        a.returns = None;
        b.returns = None;
        a.narrowing = None;
        b.narrowing = None;
        a.variables = Variables::new();
        b.variables = Variables::new();
        a.generic = Arc::from([]);
        b.generic = Arc::from([]);
        a.overloads = Arc::from([]);
        b.overloads = Arc::from([]);
        a == b
            && self.any_origin_parameters_equal(&left.parameters, &right.parameters, active)
            && self.any_origin_optional_equal(left.returns, right.returns, active)
            && match (left.narrowing, right.narrowing) {
                (Some((a, at)), Some((b, bt))) => {
                    a == b && self.any_origin_equal_inner(at, bt, active)
                }
                (None, None) => true,
                _ => false,
            }
            && left.variables.len() == right.variables.len()
            && left
                .variables
                .iter()
                .zip(right.variables.iter())
                .all(|((a, at), (b, bt))| a == b && self.any_origin_equal_inner(*at, *bt, active))
            && self.any_origin_items_equal(&left.generic, &right.generic, active)
            && left.overloads.len() == right.overloads.len()
            && left
                .overloads
                .iter()
                .zip(right.overloads.iter())
                .all(|(a, b)| self.any_origin_signatures_equal(a, b, active))
    }

    // Only a top-level Any (or a Union arm containing one) is a proper
    // supertype of an Any atom. Alias/annotation wrappers remain unproved.
    pub(super) fn special_any_meet(&self, left: Type, right: Type) -> Option<Type> {
        fn any_arm(context: &Context, ty: Type) -> Option<bool> {
            if ty.is_any() {
                return Some(true);
            }
            match context.node(ty).as_deref() {
                Some(Node::Union(items)) => {
                    let mut found = false;
                    for &item in items {
                        found |= any_arm(context, item)?;
                    }
                    Some(found)
                }
                Some(
                    Node::Variable(_)
                    | Node::Alias(_, _)
                    | Node::AliasInstance(_, _)
                    | Node::InferredAlias { .. }
                    | Node::TypingForm(_)
                    | Node::LiteralForm
                    | Node::Deleted(_)
                    | Node::Unpack(_)
                    | Node::ParamSpecArgs(_)
                    | Node::ParamSpecKwargs(_),
                ) => None,
                _ => Some(false),
            }
        }
        fn only_any_bottom(context: &Context, ty: Type) -> Option<bool> {
            if ty.is_any() || ty == Type::Never {
                return Some(true);
            }
            match context.node(ty).as_deref() {
                Some(Node::Union(items)) => {
                    let mut all = true;
                    for &item in items {
                        all &= only_any_bottom(context, item)?;
                    }
                    Some(all)
                }
                Some(
                    Node::Variable(_)
                    | Node::Alias(_, _)
                    | Node::AliasInstance(_, _)
                    | Node::InferredAlias { .. },
                ) => None,
                _ => Some(false),
            }
        }
        if left == Type::AnySpecial {
            return Some(if any_arm(self, right)? { left } else { right });
        }
        if right == Type::AnySpecial {
            return Some(if only_any_bottom(self, left)? {
                left
            } else if any_arm(self, left)? {
                right
            } else {
                left
            });
        }
        None
    }

    pub(super) fn has_special_any(&self, ty: Type) -> bool {
        self.has_special_any_inner(ty, &mut BTreeSet::new())
    }

    fn has_special_any_inner(&self, ty: Type, seen: &mut BTreeSet<Type>) -> bool {
        if ty == Type::AnySpecial {
            return true;
        }
        if !seen.insert(ty) {
            return false;
        }
        let result = match self.node(ty).as_deref() {
            Some(
                Node::Container(_, items)
                | Node::Tuple(items)
                | Node::Union(items)
                | Node::AppliedClass {
                    arguments: items, ..
                }
                | Node::AliasInstance(_, items),
            ) => items
                .iter()
                .any(|&item| self.has_special_any_inner(item, seen)),
            Some(
                Node::VariadicTuple(item)
                | Node::Unpack(item)
                | Node::ParamSpecArgs(item)
                | Node::ParamSpecKwargs(item),
            ) => self.has_special_any_inner(*item, seen),
            Some(Node::InferredAlias {
                arguments, target, ..
            }) => {
                arguments
                    .iter()
                    .any(|&item| self.has_special_any_inner(item, seen))
                    || self.has_special_any_inner(*target, seen)
            }
            Some(Node::Parameters(parameters, _)) => parameters
                .iter()
                .any(|p| p.ty.is_some_and(|ty| self.has_special_any_inner(ty, seen))),
            Some(Node::Callable(signature)) => self.signature_has_special_any(signature, seen),
            Some(Node::Alias(_, Some(target))) => self.has_special_any_inner(*target, seen),
            // A declared Variable is an identity, not a computed substitution.
            // Its bound/default stay with the existing declaration.
            _ => false,
        };
        seen.remove(&ty);
        result
    }

    fn signature_has_special_any(&self, signature: &Signature, seen: &mut BTreeSet<Type>) -> bool {
        signature
            .parameters
            .iter()
            .any(|p| p.ty.is_some_and(|ty| self.has_special_any_inner(ty, seen)))
            || signature
                .returns
                .is_some_and(|ty| self.has_special_any_inner(ty, seen))
            || signature
                .narrowing
                .is_some_and(|(_, ty)| self.has_special_any_inner(ty, seen))
            || signature
                .overloads
                .iter()
                .any(|sig| self.signature_has_special_any(sig, seen))
    }

    // Unmodelled join operations retain their legacy value shape, but cannot
    // manufacture positive erased-origin proof for their computed result.
    pub(super) fn ordinary_any_origin(&self, ty: Type) -> Type {
        if self.has_special_any(ty) {
            self.mark_unsupported_any_origin();
            self.substitute(ty, &BTreeMap::from([(Type::AnySpecial, Type::Any)]))
        } else {
            ty
        }
    }
}
