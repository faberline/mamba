//! Applied class types share declaration identities and substitute their members.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

impl Context {
    pub(in crate::infrastructure::mypy) fn specialize_method(
        &self,
        method: &super::super::classes::Method,
        receiver: Type,
        owner: u32,
    ) -> super::super::classes::Method {
        let solutions = self.class_solutions(receiver, owner);
        super::super::classes::Method {
            signature: std::sync::Arc::new(self.substitute_callable(&method.signature, &solutions)),
            setter: method.setter.as_ref().map(|signature| {
                std::sync::Arc::new(self.substitute_callable(signature, &solutions))
            }),
            ..method.clone()
        }
    }
    pub(in crate::infrastructure::mypy) fn applied_class(
        &self,
        id: u32,
        arguments: Vec<Type>,
        value: bool,
    ) -> Type {
        if arguments.is_empty() {
            return if value {
                self.class(id).object
            } else {
                self.class(id).instance
            };
        }
        self.intern(Node::AppliedClass {
            id,
            arguments,
            value,
        })
    }

    pub(in crate::infrastructure::mypy) fn class_value_type(&self, ty: Type) -> Type {
        if self.node(ty).is_none() && self.builtin_scalar(ty).is_some() {
            return self.class(self.builtin_class(ty)).object;
        }
        match self.node(ty).as_deref() {
            Some(Node::AppliedClass { id, arguments, .. }) => {
                self.applied_class(*id, arguments.clone(), true)
            }
            Some(Node::Instance(id)) => self.class(*id).object,
            _ => ty,
        }
    }

    pub(in crate::infrastructure::mypy) fn class_arguments(&self, ty: Type) -> Vec<Type> {
        match self.node(ty).as_deref() {
            Some(Node::Container(Container::TypeObject, items)) => self.class_arguments(items[0]),
            Some(Node::AppliedClass { arguments, .. }) => arguments.clone(),
            Some(Node::Class(id) | Node::Instance(id)) => self.class(*id).generic.clone(),
            Some(Node::Variable(variable)) if self.is_source_self_variable(ty) => {
                self.class_arguments(variable.bound)
            }
            _ => Vec::new(),
        }
    }

    pub(in crate::infrastructure::mypy) fn class_solutions(
        &self,
        ty: Type,
        owner: u32,
    ) -> BTreeMap<Type, Type> {
        let mut solutions: BTreeMap<_, _> = self
            .class(owner)
            .generic
            .iter()
            .copied()
            .zip(self.class_arguments_for(ty, owner).unwrap_or_default())
            .collect();
        if let Some(variable) = self.source_self_variable(owner) {
            if let Some(receiver) = self.instance_type(ty) {
                solutions.insert(variable, receiver);
            }
        }
        solutions
    }

    pub(in crate::infrastructure::mypy) fn class_arguments_for(
        &self,
        ty: Type,
        owner: u32,
    ) -> Option<Vec<Type>> {
        if self.class_id(ty).is_none() {
            return self.builtin_collection_arguments(ty, owner);
        }
        self.inherited_class_arguments(ty, owner, &mut std::collections::BTreeSet::new())
    }

    pub(super) fn return_superclass_arguments(
        &self,
        template: Type,
        owner: u32,
        target: PythonTarget,
    ) -> Option<Vec<Type>> {
        let root = self.class_id(template)?;
        if root == owner {
            return None;
        }
        let lineage = self.class_mro(root).ok()?;
        if !lineage.contains(&owner) {
            return None;
        }
        for id in lineage {
            let class = self.class(id);
            if !class.any_base_origins.is_empty()
                || class.plugin
                || class.base_types.iter().any(|ty| ty.is_any())
                || class.scalar_bases.iter().any(|ty| ty.is_any())
                || self.class_mro(id).is_err()
            {
                return None;
            }
        }
        let mut current = template;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let node = self.node(current)?;
            let (id, arguments) = match node.as_ref() {
                Node::AppliedClass {
                    id,
                    arguments,
                    value: false,
                } => (*id, arguments.clone()),
                Node::Instance(id) if self.class(*id).generic.is_empty() => (*id, Vec::new()),
                _ => return None,
            };
            if !seen.insert(id) {
                return None;
            }
            let class = self.class(id);
            if class.protocol
                || class.typed_dict.is_some()
                || arguments.len() != class.generic.len()
                || class
                    .generic
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != class.generic.len()
                || class.generic.iter().any(|&parameter| {
                    !matches!(self.node(parameter).as_deref(), Some(Node::Variable(variable))
                        if !variable.parameter_spec && !variable.type_var_tuple)
                })
            {
                return None;
            }
            if id == owner {
                return Some(arguments);
            }
            let registry = self.classes.borrow();
            if registry.populated_class_bases.get(&id) != Some(&true)
                || registry.module_targets.get(&class.namespace) != Some(&target)
            {
                return None;
            }
            drop(registry);
            let ([base], [base_type], []) = (
                class.bases.as_slice(),
                class.base_types.as_slice(),
                class.scalar_bases.as_slice(),
            ) else {
                return None;
            };
            if self.class_id(*base_type) != Some(*base) || self.is_class_value(*base_type) {
                return None;
            }
            let substitutions = class.generic.iter().copied().zip(arguments).collect();
            current = self.substitute(*base_type, &substitutions);
        }
    }

    fn inherited_class_arguments(
        &self,
        ty: Type,
        owner: u32,
        active: &mut std::collections::BTreeSet<u32>,
    ) -> Option<Vec<Type>> {
        let id = self.class_id(ty)?;
        if id == owner {
            return Some(self.class_arguments(ty));
        }
        if !active.insert(id) {
            return None;
        }
        let class = self.class(id);
        let substitutions = class
            .generic
            .iter()
            .copied()
            .zip(self.class_arguments(ty))
            .collect();
        let result = class.base_types.iter().find_map(|&base| {
            self.inherited_class_arguments(self.substitute(base, &substitutions), owner, active)
        });
        active.remove(&id);
        result
    }

    pub(super) fn bare_class_annotation(&self, ty: Type) -> Type {
        if matches!(
            self.node(ty).as_deref(),
            Some(Node::Container(Container::TypeObject, _))
        ) {
            return ty;
        }
        if let Some(scalar) = self.builtin_scalar(ty) {
            return scalar;
        }
        if let Some(node) = self.node(ty) {
            if let Node::AppliedClass { id, arguments, .. } = node.as_ref() {
                return self.applied_class(*id, arguments.clone(), false);
            }
        }
        let Some(id) = self.class_id(ty) else {
            return ty;
        };
        let arguments = self.effective_class_arguments(id, Vec::new());
        self.applied_class(id, arguments, false)
    }

    pub(in crate::infrastructure::mypy) fn effective_class_arguments(
        &self,
        id: u32,
        mut arguments: Vec<Type>,
    ) -> Vec<Type> {
        let class = self.class(id);
        let minimum = class.generic.iter().filter(|&&ty| self.node(ty).is_some_and(|node| matches!(node.as_ref(), Node::Variable(variable) if variable.default.is_none()))).count();
        if arguments.len() < minimum || arguments.len() > class.generic.len() {
            arguments.clear();
        }
        let mut solutions = BTreeMap::new();
        for (index, &parameter) in class.generic.iter().enumerate() {
            let ty = arguments.get(index).copied().unwrap_or_else(|| {
                self.node(parameter)
                    .and_then(|node| {
                        if let Node::Variable(variable) = node.as_ref() {
                            variable.default
                        } else {
                            None
                        }
                    })
                    .map(|default| self.substitute(default, &solutions))
                    .unwrap_or_else(|| self.default_generic_argument(parameter))
            });
            if index >= arguments.len() {
                arguments.push(ty);
            }
            solutions.insert(parameter, ty);
        }
        arguments
    }

    pub(super) fn applied_class_accepts(
        &self,
        expected: u32,
        arguments: &[Type],
        value: bool,
        actual: Type,
    ) -> bool {
        let Some(id) = self.class_id(actual) else {
            return false;
        };
        if value != self.is_class_value(actual) || !self.nominal_accepts(expected, id) {
            return false;
        }
        let Some(actual_arguments) = self.class_arguments_for(actual, expected) else {
            return false;
        };
        arguments.len() == actual_arguments.len()
            && arguments.iter().zip(actual_arguments).enumerate().all(
                |(index, (&expected_argument, actual))| match self
                    .class_parameter_variance(expected, index)
                {
                    ParameterVariance::Covariant => self.accepts(expected_argument, actual),
                    ParameterVariance::Contravariant => self.accepts(actual, expected_argument),
                    ParameterVariance::Invariant | ParameterVariance::Inferred => {
                        self.accepts(expected_argument, actual)
                            && self.accepts(actual, expected_argument)
                    }
                },
            )
    }

    pub(in crate::infrastructure::mypy) fn class_annotation(
        &self,
        base: &Expr,
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Type> {
        let id = self
            .annotation_with_variables(base, variables)
            .and_then(|ty| self.class_id(ty))?;
        let types = self.generic_arguments(&self.class(id).generic, arguments, variables)?;
        Some(self.applied_class(id, self.effective_class_arguments(id, types), false))
    }
}
