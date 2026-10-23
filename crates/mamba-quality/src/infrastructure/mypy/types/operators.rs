//! Binary method results retain their receiver substitutions and call bounds.
use super::super::signatures::{Kind, Signature};
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    Arguments, ExprBinOp, ExprNoneLiteral,
};
use mamba_language::application::quality_syntax::PythonTarget;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(in crate::infrastructure::mypy) fn symbol(name: &str) -> Option<&'static str> {
    match name {
        "__or__" => Some("|"),
        "__and__" => Some("&"),
        "__xor__" => Some("^"),
        "__sub__" => Some("-"),
        "__add__" => Some("+"),
        _ => None,
    }
}

impl Context {
    // Shared with normal BinOp inference at its original dispatch position.
    pub(in crate::infrastructure::mypy) fn primitive_binary_result(
        left: Type,
        right: Type,
        operator: Operator,
    ) -> Option<Type> {
        match (left, right, operator) {
            (Type::Int | Type::Bool, Type::Int | Type::Bool, Operator::Div) => Some(Type::Float),
            (
                Type::Int | Type::Bool,
                Type::Int | Type::Bool,
                Operator::Add | Operator::Sub | Operator::Mult | Operator::FloorDiv | Operator::Mod,
            ) => Some(Type::Int),
            (Type::Str, Type::Str, Operator::Add) => Some(Type::Str),
            (Type::Bytes, Type::Bytes, Operator::Add) => Some(Type::Bytes),
            (
                Type::Float,
                Type::Int | Type::Float | Type::Bool,
                Operator::Add | Operator::Sub | Operator::Mult | Operator::Div,
            )
            | (
                Type::Int | Type::Bool,
                Type::Float,
                Operator::Add | Operator::Sub | Operator::Mult | Operator::Div,
            ) => Some(Type::Float),
            _ => None,
        }
    }

    // Compact set receivers cannot introduce a subclass/reflected override.
    // The return and receiver error come from one real method binding.
    pub(in crate::infrastructure::mypy) fn candidate_binary_result(
        &self,
        left: Type,
        right: Type,
        operator: Operator,
        target: PythonTarget,
    ) -> Option<Type> {
        if left == Type::Any || right == Type::Any {
            return Some(Type::Any);
        }
        if let Some(actual) = Self::primitive_binary_result(left, right, operator) {
            return Some(actual);
        }
        if operator != Operator::BitAnd {
            return None;
        }
        if ![left, right].iter().all(|&ty| {
            matches!(self.node(ty).as_deref(),
            Some(Node::Container(Container::Set, arguments)) if arguments.len() == 1)
        }) {
            return None;
        }
        let (signature, error) = self.class_method_call_binding_for(left, "__and__", target)?;
        if error.is_some() {
            return None;
        }
        let [parameter] = signature.parameters.as_ref() else {
            return None;
        };
        if !signature.overloads.is_empty()
            || signature.ellipsis
            || signature.ignored
            || !matches!(
                parameter.kind,
                Kind::PositionalOnly | Kind::PositionalOrKeyword
            )
        {
            return None;
        }
        let expected = parameter.ty?;
        let solution = self.solve_call(&signature.generic, &[(expected, right)], None);
        if !solution.unresolved.is_empty() {
            return None;
        }
        if !solution.failed.is_empty() || !solution.invalid.is_empty() {
            return None;
        }
        let expected = self.substitute(expected, &solution.types);
        if !self.candidate_value_role(expected, 0) {
            return None;
        }
        if !self.accepts(expected, right) {
            return None;
        }
        let returned = self.substitute(signature.returns?, &solution.types);
        self.candidate_value_role(returned, 0).then_some(returned)
    }

    // Only the locked plain-dict method has this contextual literal contract.
    pub(in crate::infrastructure::mypy::types) fn dictionary_or_signature(
        &self,
        signature: &Signature,
    ) -> bool {
        signature.definition_module.as_deref() == Some("builtins")
            && signature.owner.as_deref() == Some("dict")
            && signature.name.as_deref() == Some("__or__")
            && signature.bound_receiver.as_deref() == Some("self")
            && !signature.constructor
            && !signature.ellipsis
            && signature.overloads.len() == 2
            && signature.overloads.iter().all(|variant| {
                variant.overloads.is_empty()
                    && !variant.ellipsis
                    && matches!(variant.parameters.as_ref(), [parameter]
                        if parameter.kind == Kind::PositionalOnly
                            && parameter.required && parameter.ty.is_some())
                    && variant.returns.is_some()
            })
            && signature.overloads[0].parameters[0]
                .ty
                .is_some_and(|ty| self.dictionary_or_arguments(ty).is_some())
    }

    pub(in crate::infrastructure::mypy::types) fn dictionary_or_flat(&self, ty: Type) -> bool {
        matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::None
                | Type::Object
        ) || matches!(self.node(ty).as_deref(), Some(Node::Union(items))
            if !items.is_empty() && items.iter().all(|&item| self.dictionary_or_flat(item)))
    }

    pub(in crate::infrastructure::mypy::types) fn dictionary_or_arguments(
        &self,
        ty: Type,
    ) -> Option<Vec<Type>> {
        match self.node(ty)?.as_ref() {
            Node::Container(Container::Dict, arguments)
                if arguments.len() == 2
                    && arguments
                        .iter()
                        .all(|&argument| self.dictionary_or_flat(argument)) =>
            {
                Some(arguments.clone())
            }
            _ => None,
        }
    }

    pub(super) fn dictionary_or_expression(
        &self,
        binary: &ExprBinOp,
        left: Type,
        right: Type,
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        self.dictionary_or_arguments(left)?;
        self.dictionary_or_arguments(right)?;
        // Source literals with context-sensitive children are not in this proof.
        match binary.right.as_ref() {
            Expr::Dict(_) => {
                self.dictionary_or_literal(&binary.right, right)?;
            }
            Expr::Name(_) => {}
            _ => return None,
        }
        let signature = self.nominal_dict_method(left, "__or__", PythonTarget::Py312)?;
        if !self.dictionary_or_signature(&signature) {
            return None;
        }
        let expression = Expr::Call(ExprCall {
            node_index: Default::default(),
            range_start: binary.start(),
            // A pure inference carrier skips source Name/Attribute callee lookup.
            // The bound callable below already contains the receiver arguments.
            func: Box::new(Expr::NoneLiteral(ExprNoneLiteral {
                node_index: Default::default(),
                range: binary.left.range(),
            })),
            arguments: Arguments {
                node_index: Default::default(),
                range: binary.range(),
                args: vec![*binary.right.clone()].into(),
                keywords: Default::default(),
            },
        });
        let Expr::Call(call) = &expression else {
            unreachable!()
        };
        // The final live allocations carry explicit already-inferred values.
        // No source range, clone equality, or old pointer supplies these receipts.
        let mut observed = self
            .observed_expression_types
            .borrow()
            .as_ref()
            .map(|values| values.lock().expect("observed expression types").clone())
            .unwrap_or_default();
        observed.insert(
            call.func.as_ref() as *const Expr as usize,
            Some(self.callable_type(signature)),
        );
        observed.insert(&call.arguments.args[0] as *const Expr as usize, Some(right));
        let observed = Arc::new(Mutex::new(observed));
        let returned =
            self.infer_with_observed(&expression, environment, call_return, &observed)?;
        let arguments = self.dictionary_or_arguments(returned)?;
        let arguments = arguments
            .into_iter()
            .map(|argument| {
                // expandtype.py remove_trivial absorbs object at substitution.
                // It does not generally remove subtypes or numeric promotions.
                if matches!(self.node(argument).as_deref(), Some(Node::Union(items))
                    if items.contains(&Type::Object))
                {
                    self.union_join_simplify(vec![argument])
                } else {
                    Some(argument)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(self.intern(Node::Container(Container::Dict, arguments)))
    }

    pub(super) fn binary_result(
        &self,
        left: Type,
        right: Type,
        operator: Operator,
    ) -> Option<Type> {
        if operator == Operator::Add {
            if let Some((returned, _)) = self.template_addition(left, right) {
                return Some(returned);
            }
            return self
                .addition_result(left, right)
                .and_then(|(returned, _)| returned);
        }
        if operator == Operator::Div {
            let left = self.expanded_alias_instance(left);
            let nominal_instance = self.node(left).is_some_and(|node| {
                matches!(node.as_ref(), Node::Instance(id) if self.class(*id).generic.is_empty())
            });
            if !nominal_instance
                || !matches!(
                    right,
                    Type::Int
                        | Type::Float
                        | Type::Complex
                        | Type::Str
                        | Type::Bytes
                        | Type::Bool
                        | Type::None
                        | Type::Object
                )
                || self
                    .method_receiver_error(left, "__truediv__", PythonTarget::Py312)
                    .is_some()
            {
                return None;
            }
            let signature = self.class_method(left, "__truediv__")?;
            let (returned, incompatible) = self.addition_call(&signature, right)?;
            return (!incompatible && returned == Some(left)).then_some(left);
        }
        let name = match operator {
            Operator::BitOr => "__or__",
            Operator::BitAnd => "__and__",
            Operator::BitXor => "__xor__",
            Operator::Sub => "__sub__",
            _ => return None,
        };
        let signature = self.class_method(left, name)?;
        if operator == Operator::Sub
            && !signature.overloads.is_empty()
            && self
                .method_receiver_error(left, name, PythonTarget::Py312)
                .is_none()
        {
            let left_id = self.node(left).and_then(|node| match node.as_ref() {
                Node::Instance(id) => Some(*id),
                _ => None,
            });
            let right_id = self.node(right).and_then(|node| match node.as_ref() {
                Node::Instance(id) => Some(*id),
                _ => None,
            });
            if let (Some(left_id), Some(right_id)) = (left_id, right_id) {
                if left_id == right_id && self.trusted_operator_class(left_id) {
                    let mut selected = None;
                    let mut proof_complete = true;
                    for variant in signature.overloads.iter() {
                        if !variant.overloads.is_empty() {
                            proof_complete = false;
                            break;
                        }
                        let parameter = variant.parameters.iter().find(|parameter| {
                            matches!(
                                parameter.kind,
                                Kind::PositionalOnly
                                    | Kind::PositionalOrKeyword
                                    | Kind::VariadicPositional
                            )
                        });
                        let (Some(expected), Some(returned)) = (
                            parameter.and_then(|parameter| parameter.ty),
                            variant.returns,
                        ) else {
                            proof_complete = false;
                            break;
                        };
                        if !self.trusted_flat_operator_type(expected)
                            || !self.trusted_flat_operator_type(returned)
                        {
                            proof_complete = false;
                            break;
                        }
                        if !self.accepts(expected, right) {
                            continue;
                        }
                        let Some((Some(result), incompatible)) = self.addition_call(variant, right)
                        else {
                            proof_complete = false;
                            break;
                        };
                        if incompatible || !self.trusted_flat_operator_type(result) {
                            proof_complete = false;
                            break;
                        }
                        if selected.replace(result).is_some() {
                            proof_complete = false;
                            break;
                        }
                    }
                    if proof_complete {
                        if let Some(selected) = selected {
                            return Some(selected);
                        }
                    }
                }
            }
        }
        let expected = signature.parameters.first()?.ty?;
        let solution = self.solve_call(&signature.generic, &[(expected, right)], None);
        Some(self.substitute_inferred_alias(signature.returns?, &solution.types))
    }

    /// The known result, if any, and whether all typed operator calls failed.
    pub(in crate::infrastructure::mypy) fn addition_result(
        &self,
        left: Type,
        right: Type,
    ) -> Option<(Option<Type>, bool)> {
        let left = self.expanded_alias_instance(left);
        let right = self.expanded_alias_instance(right);
        if [left, right].iter().any(|&ty| {
            matches!(ty, Type::Any | Type::AnySpecial | Type::Never)
                || self.is_alias_instance(ty)
                || self.node(ty).is_some_and(|node| {
                    matches!(
                        node.as_ref(),
                        Node::Union(_) | Node::Variable(_) | Node::Deleted(_)
                    )
                })
        }) {
            return None;
        }
        let forward = self.class_method(left, "__add__");
        let same = self.addition_same_type(left, right, &mut BTreeSet::new())?;
        let reverse = (!same)
            .then(|| self.class_method(right, "__radd__"))
            .flatten();
        let class_id = |ty| {
            self.class_id(ty).or_else(|| {
                self.builtin_scalar(ty)
                    .map(|scalar| self.builtin_class(scalar))
            })
        };
        let (left_id, right_id) = (class_id(left), class_id(right));
        let reverse_first = match (left_id, right_id) {
            (Some(left), Some(right)) if left != right => {
                let definer = |id, name: &str| {
                    self.class_lineage(id)
                        .into_iter()
                        .find(|&id| self.class(id).methods.contains_key(name))
                };
                self.class_lineage(right).contains(&left)
                    && definer(right, "__radd__").is_some()
                    && definer(right, "__radd__") != definer(left, "__add__")
            }
            _ => false,
        };
        let variants = if reverse_first {
            [(reverse, left), (forward, right)]
        } else {
            [(forward, right), (reverse, left)]
        };
        let mut rejected = Vec::new();
        for (signature, argument) in variants {
            let Some(signature) = signature else {
                continue;
            };
            let (returned, incompatible) = self.addition_call(&signature, argument)?;
            if !incompatible {
                return Some((returned, false));
            }
            rejected.push(returned);
        }
        // A missing/erased declaration or an Any base is not rejection evidence.
        for id in [left_id, right_id].into_iter().flatten() {
            if self.class_lineage(id).iter().any(|&id| {
                let class = self.class(id);
                class.base_types.iter().any(|ty| ty.is_any())
                    || class.erased_members.contains("__add__")
                    || class.erased_members.contains("__radd__")
            }) {
                return None;
            }
        }
        match rejected.as_slice() {
            [returned] => Some((*returned, true)),
            // Error-origin Any is not represented separately in this type graph.
            [_, _, ..] => Some((None, true)),
            _ => None,
        }
    }

    // This is exact equality for these represented shapes, not assignment acceptance.
    fn addition_same_type(
        &self,
        left: Type,
        right: Type,
        active: &mut BTreeSet<(Type, Type)>,
    ) -> Option<bool> {
        let left = self.expanded_alias_instance(left);
        let right = self.expanded_alias_instance(right);
        if self.is_alias_instance(left) || self.is_alias_instance(right) {
            return None;
        }
        if left == right {
            return Some(true);
        }
        if !active.insert((left, right)) {
            return None;
        }
        let same = match (self.node(left).as_deref(), self.node(right).as_deref()) {
            (None, None) => Some(false), // Distinct scalar types; no Any or promotion rule.
            (Some(Node::Union(left)), Some(Node::Union(right))) => {
                self.addition_same_union(left, right, active)
            }
            (
                Some(Node::AppliedClass {
                    id: left,
                    arguments: left_args,
                    value: left_value,
                }),
                Some(Node::AppliedClass {
                    id: right,
                    arguments: right_args,
                    value: right_value,
                }),
            ) => {
                if left_value != right_value {
                    Some(false)
                } else if left == right {
                    self.addition_same_arguments(left_args, right_args, active)
                } else {
                    (!self.class(*left).protocol && !self.class(*right).protocol).then_some(false)
                }
            }
            (Some(Node::Instance(left)), Some(Node::Instance(right)))
            | (Some(Node::Class(left)), Some(Node::Class(right))) => {
                (!self.class(*left).protocol && !self.class(*right).protocol).then_some(false)
            }
            (Some(Node::Container(left_kind, left)), Some(Node::Container(right_kind, right)))
                if left_kind == right_kind =>
            {
                self.addition_same_arguments(left, right, active)
            }
            (Some(Node::Tuple(left)), Some(Node::Tuple(right))) => {
                self.addition_same_arguments(left, right, active)
            }
            (Some(Node::VariadicTuple(left)), Some(Node::VariadicTuple(right))) => {
                self.addition_same_type(*left, *right, active)
            }
            (Some(Node::Literal(left)), Some(Node::Literal(right)))
            | (Some(Node::LiteralValue(left)), Some(Node::LiteralValue(right))) => {
                Some(left == right)
            }
            _ => None,
        };
        active.remove(&(left, right));
        same
    }

    fn addition_same_arguments(
        &self,
        left: &[Type],
        right: &[Type],
        active: &mut BTreeSet<(Type, Type)>,
    ) -> Option<bool> {
        if left.len() != right.len() {
            return None;
        }
        let mut unknown = false;
        for (&left, &right) in left.iter().zip(right) {
            match self.addition_same_type(left, right, active) {
                Some(false) => return Some(false),
                None => unknown = true,
                Some(true) => {}
            }
        }
        (!unknown).then_some(true)
    }

    fn addition_same_union(
        &self,
        left: &[Type],
        right: &[Type],
        active: &mut BTreeSet<(Type, Type)>,
    ) -> Option<bool> {
        let mut unknown = false;
        // Member order affects display, but not semantic coverage in either direction.
        for (items, candidates) in [(left, right), (right, left)] {
            for &item in items {
                let mut matched = false;
                for &candidate in candidates {
                    match self.addition_same_type(item, candidate, active) {
                        Some(true) => {
                            matched = true;
                            break;
                        }
                        None => {}
                        Some(false) => {}
                    }
                }
                if !matched {
                    // Redundant subtype members need a proper-subtype proof.
                    unknown = true;
                }
            }
        }
        (!unknown).then_some(true)
    }

    fn addition_call(&self, signature: &Signature, argument: Type) -> Option<(Option<Type>, bool)> {
        if !signature.overloads.is_empty() {
            for variant in signature.overloads.iter() {
                let (returned, incompatible) = self.addition_call(variant, argument)?;
                if !incompatible {
                    return Some((returned, false));
                }
            }
            // Preserve known rejection without inventing an ordinary Any result.
            return Some((None, true));
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
        let returned = signature.returns?;
        let solution = self.solve_call(&signature.generic, &[(expected, argument)], None);
        if !solution.unresolved.is_empty() || !solution.failed.is_empty() {
            return None;
        }
        let expected = self.substitute_inferred_alias(expected, &solution.types);
        let returned = self.substitute_inferred_alias(returned, &solution.types);
        Some((
            Some(returned),
            !solution.invalid.is_empty() || !self.accepts(expected, argument),
        ))
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn template_addition(
        &self,
        left: Type,
        right: Type,
    ) -> Option<(Type, bool)> {
        if self.is_template_instance(left) {
            if self.accepts(left, right) {
                return Some((left, false));
            }
            if let Some(returned) = self.template_operator_return(right, "__radd__", left) {
                return Some((returned, false));
            }
            let reverse = matches!(right, Type::Int | Type::Bool | Type::Float | Type::Complex)
                || self.class_method(right, "__radd__").is_some();
            return Some((if reverse { Type::Any } else { left }, true));
        }
        if self.is_template_instance(right) {
            if left.is_any() {
                return Some((Type::Any, false));
            }
            if let Some(returned) = self.template_operator_return(left, "__add__", right) {
                return Some((returned, false));
            }
            let returned = match left {
                Type::Bool => Type::Int,
                Type::Int | Type::Float | Type::Complex | Type::Str | Type::Bytes => left,
                _ => Type::Any,
            };
            return Some((returned, true));
        }
        None
    }

    fn template_operator_return(
        &self,
        receiver: Type,
        method: &str,
        argument: Type,
    ) -> Option<Type> {
        let signature = self.class_method(receiver, method)?;
        let expected = signature.parameters.first()?.ty?;
        let solution = self.solve_call(&signature.generic, &[(expected, argument)], None);
        let expected = self.substitute_inferred_alias(expected, &solution.types);
        self.accepts(expected, argument)
            .then(|| {
                signature
                    .returns
                    .map(|returned| self.substitute_inferred_alias(returned, &solution.types))
            })
            .flatten()
    }
}

impl Context {
    fn trusted_operator_class(&self, root: u32) -> bool {
        let Ok(lineage) = self.class_mro(root) else {
            return false;
        };
        !lineage.is_empty()
            && lineage.into_iter().all(|id| {
                let class = self.class(id);
                !class.protocol
                    && !class.plugin
                    && class.generic.is_empty()
                    && class.any_base_origins.is_empty()
                    && !class.base_types.iter().any(|ty| ty.is_any())
                    && !class.scalar_bases.iter().any(|ty| ty.is_any())
                    && self.class_mro(id).is_ok()
            })
    }

    fn trusted_flat_operator_type(&self, ty: Type) -> bool {
        if matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::None
        ) {
            return true;
        }
        matches!(self.node(ty).as_deref(), Some(Node::Instance(id)) if self.trusted_operator_class(*id))
    }
}
