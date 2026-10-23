//! Unbound instance methods retain captured class type parameters.
use super::*;
use crate::infrastructure::mypy::{
    classes::{Method, MethodKind},
    signatures::{Kind, Signature},
};
use mamba_language::application::quality_syntax::PythonTarget;

pub(super) struct BoundMethod {
    pub signature: Arc<Signature>,
    pub error: Option<String>,
}

impl BoundMethod {
    pub(super) fn plain(signature: Arc<Signature>) -> Self {
        Self {
            signature,
            error: None,
        }
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn class_method_kind(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<MethodKind> {
        self.class_lineage(self.class_id(ty)?)
            .into_iter()
            .find_map(|id| self.class(id).methods.get(name).map(|method| method.kind))
    }

    pub(in crate::infrastructure::mypy) fn invalid_class_index(&self, receiver: Type) -> bool {
        self.class_id(receiver).is_some_and(|id| {
            self.is_class_value(receiver)
                && self.class(id).namespace != "builtins.type"
                && self.class(id).generic.is_empty()
                && self.class_method(receiver, "__class_getitem__").is_none()
                && self.class_method(receiver, "__getitem__").is_none()
        })
    }

    pub(super) fn has_declared_initializer(&self, ty: Type) -> bool {
        self.class_id(ty).is_some_and(|id| {
            self.class_lineage(id)
                .iter()
                .any(|&parent| self.class(parent).methods.contains_key("__init__"))
        })
    }

    pub(super) fn unbound_class_parameters(
        &self,
        mut signature: Signature,
        receiver: Type,
        owner: u32,
        kind: MethodKind,
    ) -> Signature {
        let class = self.class(self.class_id(receiver).unwrap_or(owner));
        if !matches!(kind, MethodKind::Instance | MethodKind::Property)
            || !self.is_class_value(receiver)
            || class.generic.is_empty()
            || !matches!(self.node(receiver).as_deref(), Some(Node::Class(_)))
        {
            return signature;
        }
        let mut generic = class.generic.clone();
        generic.extend(
            signature
                .generic
                .iter()
                .filter(|ty| !class.generic.contains(ty))
                .copied(),
        );
        signature.generic = generic.into();
        if let Some(first) = std::sync::Arc::make_mut(&mut signature.parameters).first_mut() {
            if first.ty.is_some_and(|ty| self.class_id(ty) == Some(owner)) {
                first.ty = Some(self.applied_class(owner, class.generic.clone(), false));
            }
        }
        signature
    }
    pub(super) fn bind_class_method_at(
        &self,
        ty: Type,
        self_receiver: Type,
        id: u32,
        name: &str,
        target: PythonTarget,
    ) -> Option<BoundMethod> {
        let class = self.class(id);
        let method = class.methods.get(name)?;
        let bound = !self.is_class_value(ty);
        if name == "__new__" {
            if let Some(scalar) = self.builtin_scalar(class.instance) {
                return self
                    .builtin_allocation_member(scalar, target)
                    .map(BoundMethod::plain);
            }
        }
        if method.kind == MethodKind::Property && bound {
            return None;
        }
        let implicit = match method.kind {
            MethodKind::Class => 1,
            MethodKind::Instance if bound => 1,
            _ => 0,
        };
        let receiver = if method.kind == MethodKind::Class {
            self.intern(Node::Container(
                Container::TypeObject,
                vec![self.instance_type(self_receiver)?],
            ))
        } else {
            self_receiver
        };
        let (signature, error) =
            self.bind_method_receiver(method, receiver, implicit, name, id, target);
        // Receiver admission and method-variable inference precede owner expansion.
        let mut solutions = self.class_solutions(ty, id);
        if implicit == 1 && (bound || method.kind == MethodKind::Class) {
            if let Some(mut receiver) = method
                .signature
                .parameters
                .first()
                .and_then(|parameter| parameter.ty)
            {
                if method.kind == MethodKind::Class {
                    if let Some(Node::Container(Container::TypeObject, items)) =
                        self.node(receiver).as_deref()
                    {
                        receiver = items[0];
                    }
                }
                if method.signature.generic.contains(&receiver)
                    && self.node(receiver).is_some_and(|node| matches!(node.as_ref(), Node::Variable(variable) if variable.name == "Self"))
                {
                    solutions.insert(receiver, if method.kind == MethodKind::Class {
                        self.instance_type(ty).unwrap_or(ty)
                    } else { self_receiver });
                }
            }
        }
        if class
            .dataclass
            .as_ref()
            .is_some_and(|definition| definition.order)
            && super::super::classes::dataclass::order_symbol(name).is_some()
        {
            if bound {
                solutions.insert(self.dataclass_order_variable(id, name), ty);
            } else {
                solutions.retain(|parameter, value| parameter != value);
            }
        }
        if !bound
            && name == "__replace__"
            && class
                .dataclass
                .as_ref()
                .is_some_and(|definition| definition.generated_replace)
        {
            solutions.retain(|parameter, value| parameter != value);
        }
        Some(BoundMethod {
            signature: Arc::new(self.unbound_class_parameters(
                self.substitute_callable(&signature, &solutions),
                ty,
                id,
                method.kind,
            )),
            error,
        })
    }
    fn bind_method_receiver(
        &self,
        method: &Method,
        receiver: Type,
        implicit: usize,
        name: &str,
        owner: u32,
        target: PythonTarget,
    ) -> (Signature, Option<String>) {
        let signature = method.signature.as_ref();
        if implicit == 0 {
            return (signature.clone(), None);
        }
        let original: Vec<_> = if signature.overloads.is_empty() {
            vec![Arc::new(signature.clone())]
        } else {
            signature.overloads.to_vec()
        };
        let no_formal = original.iter().find(|item| {
            !item.parameters.first().is_some_and(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::VariadicPositional
                )
            })
        });
        let mut error = no_formal.map(|item| {
            format!(
                "Attribute function \"{name}\" with type \"{}\" does not accept self argument",
                self.callable_name(item),
            )
        });
        let mut items = original.clone();
        if error.is_none() {
            let overlapping: Vec<_> = items
                .iter()
                .filter(|item| {
                    let Some(template) = item.parameters[0].ty else {
                        return true;
                    };
                    if self.canonical_method_receiver(
                        template,
                        receiver,
                        owner,
                        method.kind,
                        target,
                    ) {
                        return true;
                    }
                    let nominal = |ty| match self.node(ty).as_deref() {
                        Some(
                            Node::Instance(id)
                            | Node::AppliedClass {
                                id, value: false, ..
                            },
                        ) => Some(*id),
                        _ => None,
                    };
                    nominal(template)
                        .zip(nominal(receiver))
                        .is_none_or(|(expected, actual)| {
                            expected != actual
                                || self.class_arguments(template).is_empty()
                                || self.receiver_overlap(
                                    receiver,
                                    template,
                                    &mut Default::default(),
                                ) != Some(false)
                        })
                })
                .cloned()
                .collect();
            // Pinned check_self_arg restores the original items if phase one removes all.
            if !overlapping.is_empty() {
                items = overlapping;
            }
            let admitted: Vec<_> = items
                .iter()
                .filter(|item| {
                    let Some(template) = item.parameters[0].ty else {
                        return true;
                    };
                    if self.canonical_method_receiver(
                        template,
                        receiver,
                        owner,
                        method.kind,
                        target,
                    ) {
                        return true;
                    }
                    if name == "__call__"
                        && matches!(self.node(template).as_deref(), Some(Node::Callable(_)))
                    {
                        return true;
                    }
                    self.erased_method_receiver(template)
                        .is_none_or(|expected| self.accepts(expected, receiver))
                })
                .cloned()
                .collect();
            if admitted.is_empty() {
                let category = if method.kind == MethodKind::Class {
                    "class attribute function"
                } else {
                    "attribute function"
                };
                error = Some(format!(
                    "Invalid self argument \"{}\" to {category} \"{name}\" with type \"{}\"",
                    self.name(receiver),
                    self.callable_name(&items[0])
                ));
                items = original;
            } else {
                items = admitted;
            }
        }
        let mut bound: Vec<_> = items
            .iter()
            .map(|item| Arc::new(self.bind_receiver_variables(item, receiver, owner)))
            .collect();
        if bound.len() == 1 {
            return ((*bound.remove(0)).clone(), error);
        }
        let mut result = bound.last().expect("receiver variants").as_ref().clone();
        result.overloads = bound.into();
        (result, error)
    }

    fn canonical_method_receiver(
        &self,
        template: Type,
        receiver: Type,
        owner: u32,
        kind: MethodKind,
        target: PythonTarget,
    ) -> bool {
        let class = self.class(owner);
        let instance = |ty| {
            if kind != MethodKind::Class {
                Some(ty)
            } else if ty == class.object {
                Some(class.instance)
            } else if self.is_class_value(ty) {
                self.instance_type(ty)
            } else {
                None
            }
        };
        let (Some(template), Some(receiver)) = (instance(template), instance(receiver)) else {
            return false;
        };
        // A locked dict's implicit self can also be a compact annotation.
        // Preserve explicit/specialized self checks and reject unproven source bases.
        if matches!(
            self.node(template).as_deref(),
            Some(Node::Container(Container::Dict, arguments))
                if arguments == &class.generic && arguments.len() == 2
        ) && self
            .nominal_dict_base(receiver, target)
            .is_some_and(|(dict, _)| dict == owner)
        {
            return true;
        }
        // Locked Mapping annotations use the compact form of the nominal owner.
        // Only its unchanged class parameters form an implicit self annotation.
        let compact_owner = matches!(
            self.node(template).as_deref(),
            Some(Node::Container(Container::Mapping, arguments))
                if class.namespace == "typing.Mapping" && arguments == &class.generic
        );
        (template == class.instance
            || template == self.applied_class(owner, class.generic.clone(), false)
            || compact_owner)
            && !self.is_class_value(receiver)
            && self
                .class_id(receiver)
                .is_some_and(|id| self.class_lineage(id).contains(&owner))
    }

    fn erased_method_receiver(&self, template: Type) -> Option<Type> {
        let template = self.expanded_alias_instance(template);
        let template = match self.node(template).as_deref() {
            Some(Node::Variable(variable))
                if variable.parameter_spec || variable.type_var_tuple =>
            {
                return None;
            }
            Some(Node::Variable(variable)) => variable.bound,
            Some(Node::Container(Container::TypeObject, items)) => {
                match self.node(items[0]).as_deref() {
                    Some(Node::Variable(variable)) => {
                        self.intern(Node::Container(Container::TypeObject, vec![variable.bound]))
                    }
                    _ => template,
                }
            }
            _ => template,
        };
        if self.is_alias_instance(template) {
            return None;
        }
        let mut variables = Vec::new();
        self.collect_variables(template, &mut variables);
        if variables
            .iter()
            .any(|&variable| self.is_parameter_spec(variable) || self.is_tuple_variable(variable))
        {
            return None;
        }
        let erased = variables
            .into_iter()
            .map(|variable| (variable, Type::Any))
            .collect();
        Some(self.substitute(template, &erased))
    }

    fn bind_receiver_variables(
        &self,
        signature: &Signature,
        receiver: Type,
        owner: u32,
    ) -> Signature {
        let Some(parameter) = signature.parameters.first() else {
            return signature.clone();
        };
        if matches!(
            parameter.kind,
            Kind::VariadicPositional | Kind::VariadicKeyword
        ) {
            return signature.clone();
        }
        let Some(template) = parameter.ty else {
            return signature.bound(1);
        };
        if !self.supported_method_receiver(template, signature.name.as_deref() != Some("__call__"))
        {
            return signature.bound(1);
        }
        let mut variables = Vec::new();
        self.collect_variables(template, &mut variables);
        variables.retain(|variable| {
            signature.generic.contains(variable) && !self.class(owner).generic.contains(variable)
        });
        if variables.is_empty() {
            return signature.bound(1);
        }
        let mut solution = self.solve_method_receiver(&variables, template, receiver);
        // bind_self uses Never for failed inference, not the solver's error Any.
        for variable in solution.unresolved.iter().chain(&solution.failed) {
            solution.types.insert(*variable, Type::Never);
        }
        self.substitute_callable(signature, &solution.types)
            .bound(1)
    }
}
