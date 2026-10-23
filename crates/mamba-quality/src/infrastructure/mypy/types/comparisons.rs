//! Rich comparisons retain direct/reflected methods and specialized dataclass behavior.
use super::*;
pub(in crate::infrastructure::mypy) mod none_identity;
pub(in crate::infrastructure::mypy) mod primitive_equality;
use crate::infrastructure::mypy::signatures::{Kind, Signature};
use mamba_language::application::quality_syntax::python_ast::CmpOp;
use mamba_language::application::quality_syntax::PythonTarget;

#[derive(Clone, Copy)]
pub(in crate::infrastructure::mypy) enum Failure {
    Left,
    Right,
    Reflected,
}

pub(in crate::infrastructure::mypy) struct Comparison {
    pub returns: Type,
    pub failure: Option<Failure>,
    pub symbol: &'static str,
    pub left: Type,
    pub right: Type,
}

pub(in crate::infrastructure::mypy) fn method(
    operator: CmpOp,
) -> Option<(&'static str, &'static str, &'static str)> {
    match operator {
        CmpOp::Lt => Some(("__lt__", "__gt__", "<")),
        CmpOp::Gt => Some(("__gt__", "__lt__", ">")),
        CmpOp::LtE => Some(("__le__", "__ge__", "<=")),
        CmpOp::GtE => Some(("__ge__", "__le__", ">=")),
        _ => None,
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn dataclass_operand(&self, ty: Type) -> bool {
        self.class_id(ty).is_some_and(|id| {
            self.class_lineage(id)
                .into_iter()
                .any(|id| self.class(id).dataclass.is_some())
        })
    }

    pub(in crate::infrastructure::mypy) fn comparison_parameter(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<(Type, Type)> {
        if self.is_class_value(ty) {
            return None;
        }
        if let Some(signature) = self.class_method(ty, name) {
            return Some((signature.parameters.first()?.ty?, signature.returns?));
        }
        let expected = match ty {
            Type::Int | Type::Float | Type::Bool => Type::Float,
            Type::Str => Type::Str,
            Type::Bytes => Type::Bytes,
            _ => return None,
        };
        Some((expected, Type::Bool))
    }

    pub(in crate::infrastructure::mypy) fn dataclass_comparison(
        &self,
        left: Type,
        right: Type,
        operator: CmpOp,
    ) -> Option<Comparison> {
        if !self.dataclass_operand(left) && !self.dataclass_operand(right) {
            return None;
        }
        let (direct, reflected, symbol) = method(operator)?;
        if left.is_any() || right.is_any() {
            return Some(Comparison {
                returns: Type::Any,
                failure: None,
                symbol,
                left,
                right,
            });
        }
        let direct = self.comparison_parameter(left, direct);
        let reflected = self.comparison_parameter(right, reflected);
        let (returns, failure) = if let Some((_, returns)) =
            direct.filter(|&(expected, _)| self.accepts(expected, right))
        {
            (returns, None)
        } else if let Some((_, returns)) =
            reflected.filter(|&(expected, _)| self.accepts(expected, left))
        {
            (returns, None)
        } else if let Some((_, returns)) = direct {
            (
                if reflected.is_none() && !self.dataclass_operand(left) {
                    returns
                } else {
                    Type::Any
                },
                Some(Failure::Right),
            )
        } else if let Some((_, returns)) = reflected {
            (returns, Some(Failure::Reflected))
        } else {
            (Type::Any, Some(Failure::Left))
        };
        Some(Comparison {
            returns,
            failure,
            symbol,
            left,
            right,
        })
    }

    pub(in crate::infrastructure::mypy) fn ordering_comparison(
        &self,
        left: Type,
        right: Type,
        operator: CmpOp,
        target: PythonTarget,
    ) -> Option<Comparison> {
        let (direct, reflected, symbol) = method(operator)?;
        let left = self.expanded_alias_instance(left);
        let right = self.expanded_alias_instance(right);
        if left.is_any() || right.is_any() {
            return Some(Comparison {
                returns: Type::Any,
                failure: None,
                symbol,
                left,
                right,
            });
        }
        if [left, right].contains(&Type::Never) {
            return Some(Comparison {
                returns: Type::Never,
                failure: None,
                symbol,
                left,
                right,
            });
        }
        for (ty, name) in [(left, direct), (right, reflected)] {
            if self.is_class_value(ty) {
                return None;
            }
            if let Some(scalar) = self.builtin_scalar(ty) {
                if self.class_method_for(ty, name, target).is_none() {
                    self.runtime_builtin_class_instance(&self.name(scalar), target)?;
                }
            } else if !matches!(
                self.node(ty).as_deref(),
                Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
            ) {
                return None;
            }
        }
        let class_id = |ty| {
            self.class_id(ty).or_else(|| {
                self.builtin_scalar(ty)
                    .map(|scalar| self.builtin_class(scalar))
            })
        };
        let left_id = class_id(left)?;
        let right_id = class_id(right)?;
        for id in [left_id, right_id] {
            if self.class_lineage(id).iter().any(|&id| {
                let class = self.class(id);
                class.protocol
                    || class.base_types.iter().any(|ty| ty.is_any())
                    || class.erased_members.contains(direct)
                    || class.erased_members.contains(reflected)
            }) {
                return None;
            }
        }
        let forward = self.class_method_for(left, direct, target);
        let reverse = self.class_method_for(right, reflected, target);
        // An absent method on a nominal placeholder is not known rejection.
        if (forward.is_none() && self.builtin_scalar(left).is_none())
            || (reverse.is_none() && self.builtin_scalar(right).is_none())
        {
            return None;
        }
        let definer = |id, name: &str| {
            self.class_lineage(id)
                .into_iter()
                .find(|&id| self.class(id).methods.contains_key(name))
        };
        let reverse_first = self.class_lineage(right_id).contains(&left_id)
            && definer(left_id, direct) != definer(right_id, reflected);
        let variants = if reverse_first {
            [
                (reverse, left, Failure::Reflected),
                (forward, right, Failure::Right),
            ]
        } else {
            [
                (forward, right, Failure::Right),
                (reverse, left, Failure::Reflected),
            ]
        };
        let mut rejected = Vec::new();
        for (signature, argument, failure) in variants {
            let Some(signature) = signature else {
                continue;
            };
            let (returns, incompatible) = self.ordering_call(&signature, argument)?;
            if !incompatible {
                return Some(Comparison {
                    returns,
                    failure: None,
                    symbol,
                    left,
                    right,
                });
            }
            rejected.push((returns, failure));
        }
        let (returns, failure) = match rejected.as_slice() {
            [] => (Type::Any, Failure::Left),
            [(returns, failure)] => (*returns, *failure),
            [(_, failure), ..] => (Type::Any, *failure),
        };
        Some(Comparison {
            returns,
            failure: Some(failure),
            symbol,
            left,
            right,
        })
    }

    // Equality is reversible even for two instances of the same class.
    // Different classes need covers_at_runtime/promotion dispatch proof first.
    pub(in crate::infrastructure::mypy) fn equality_comparison(
        &self,
        left: Type,
        right: Type,
        operator: CmpOp,
        target: PythonTarget,
    ) -> Option<Comparison> {
        let (name, symbol) = match operator {
            CmpOp::Eq => ("__eq__", "=="),
            CmpOp::NotEq => ("__ne__", "!="),
            _ => return None,
        };
        let left_receiver = self.equality_receiver(left, name, target)?;
        let right_receiver = self.equality_receiver(right, name, target)?;
        if self.class_id(left_receiver)? != self.class_id(right_receiver)? {
            return None;
        }
        // Lookup both methods once, in the same order as check_op_reversible.
        // No failed binding falls through to another lookup of that method.
        let (forward, forward_error) =
            self.class_method_call_binding_for(left_receiver, name, target)?;
        let (reverse, reverse_error) =
            self.class_method_call_binding_for(right_receiver, name, target)?;
        if forward_error.is_some()
            || reverse_error.is_some()
            || !Self::flat_equality_signature(&forward)
            || !Self::flat_equality_signature(&reverse)
        {
            return None;
        }
        for (signature, argument) in [(forward, right), (reverse, left)] {
            let expected = signature.parameters[0].ty?;
            if self.accepts(expected, argument) {
                return Some(Comparison {
                    returns: signature.returns?,
                    failure: None,
                    symbol,
                    left,
                    right,
                });
            }
        }
        None
    }

    fn equality_receiver(&self, ty: Type, name: &str, target: PythonTarget) -> Option<Type> {
        let receiver = match self.node(ty).as_deref()? {
            Node::Container(Container::Dict, items) if items.len() == 2 => {
                self.nominal_dict_instance(ty, target)?
            }
            Node::Instance(id) if self.class(*id).generic.is_empty() => ty,
            Node::AppliedClass {
                id,
                arguments,
                value: false,
            } if arguments.len() == self.class(*id).generic.len() => ty,
            _ => return None,
        };
        let id = self.class_id(receiver)?;
        let class = self.class(id);
        let method = class.methods.get(name)?;
        // Positive own methods avoid inferring absence from an incomplete base.
        // Static/class/property and erased/dynamic providers stay unsupported.
        if class.plugin
            || class.protocol
            || class.typed_dict.is_some()
            || class.dataclass.is_some()
            || class.pydantic.is_some()
            || !class.any_base_origins.is_empty()
            || class.erased_members.contains(name)
            || class.fields.contains_key(name)
            || class.decorated_fields.contains(name)
            || class.cached_properties.contains(name)
            || class.methods.contains_key("__getattr__")
            || class.fields.contains_key("__getattr__")
            || method.kind != crate::infrastructure::mypy::classes::MethodKind::Instance
            || method.setter.is_some()
            || method.signature.implicit
            || method.signature.ignored
            || method.signature.constructor
            || method.signature.ellipsis
            || !method.signature.generic.is_empty()
            || !method.signature.overloads.is_empty()
            || self.classes.borrow().populated_class_bases.get(&id) == Some(&false)
        {
            return None;
        }
        self.class_mro(id).ok()?;
        Some(receiver)
    }

    fn flat_equality_signature(signature: &Signature) -> bool {
        !signature.implicit
            && !signature.ignored
            && !signature.constructor
            && !signature.ellipsis
            && signature.narrowing.is_none()
            && signature.generic.is_empty()
            && signature.overloads.is_empty()
            && signature.bound_receiver.is_some()
            && signature.return_explicit
            && signature.returns.is_some_and(Self::plain_equality_type)
            && matches!(signature.parameters.as_ref(), [parameter]
                if matches!(parameter.kind, Kind::PositionalOnly | Kind::PositionalOrKeyword)
                    && parameter.ty.is_some_and(Self::plain_equality_type))
    }

    fn plain_equality_type(ty: Type) -> bool {
        matches!(
            ty,
            Type::Bool
                | Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::None
                | Type::Object
        )
    }

    pub(in crate::infrastructure::mypy) fn ordering_call(
        &self,
        signature: &Signature,
        argument: Type,
    ) -> Option<(Type, bool)> {
        if !signature.overloads.is_empty() {
            let mut first = None;
            for variant in signature.overloads.iter() {
                let result = self.ordering_call(variant, argument)?;
                if !result.1 {
                    return Some(result);
                }
                first.get_or_insert(result);
            }
            return first;
        }
        let (index, parameter) =
            signature
                .parameters
                .iter()
                .enumerate()
                .find(|(_, parameter)| {
                    matches!(
                        parameter.kind,
                        Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::VariadicPositional
                    )
                })?;
        if signature.ellipsis
            || signature
                .parameters
                .iter()
                .enumerate()
                .any(|(other, parameter)| other != index && parameter.required)
        {
            return None;
        }
        let expected = parameter.ty?;
        let returns = signature.returns?;
        let solution = self.solve_call(&signature.generic, &[(expected, argument)], None);
        if !solution.unresolved.is_empty() || !solution.failed.is_empty() {
            return None;
        }
        let expected = self.substitute_inferred_alias(expected, &solution.types);
        let returns = self.substitute_inferred_alias(returns, &solution.types);
        Some((
            returns,
            !solution.invalid.is_empty() || !self.accepts(expected, argument),
        ))
    }
}
