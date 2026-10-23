//! Lexical binding lookup skips class namespaces from enclosed functions.
use super::*;

pub(in crate::infrastructure::mypy) type ExceptionAliasMutation =
    (usize, TextSize, Option<Type>, Option<Type>, bool);

impl Scopes {
    pub(super) fn add_binding(
        &mut self,
        target: &Expr,
        mut ty: Option<Type>,
        infer_from_assignment: bool,
    ) {
        if let Expr::Name(name) = target {
            // Mypy gives a newly introduced inferred local `_` dynamic Any.
            // Keep that type for later inferred writes in the same local scope.
            if infer_from_assignment
                && name.id.as_str() == "_"
                && self.current().kind == Kind::Function
                && !self.current().redirects.contains_key(name.id.as_str())
                && self
                    .current()
                    .bindings
                    .get(name.id.as_str())
                    .is_none_or(|binding| binding.infer_from_assignment)
            {
                ty = Some(Type::Any);
            }
            let index = self.binding_scope(name.id.as_str());
            let earlier = self.earlier_local_assignment(name)
                && (ty.is_some()
                    || self.current().ordinary_assignment_starts.get(&name.start()) == Some(&true));
            self.stack[index]
                .bindings
                .entry(name.id.to_string())
                .and_modify(|binding| {
                    binding.annotation_form = None;
                    binding.annotation_module = None;
                    if earlier {
                        binding.first_definition = name.start();
                        binding.ty = ty;
                        binding.infer_from_assignment = infer_from_assignment;
                        binding.callable = None;
                        binding.module = None;
                    }
                    if infer_from_assignment
                        && binding.infer_from_assignment
                        && binding.first_definition == name.start()
                    {
                        binding.ty = ty;
                    }
                    if binding.ty.is_none()
                        && (binding.infer_from_assignment || !infer_from_assignment)
                    {
                        binding.ty = ty;
                    }
                })
                .or_insert(Binding {
                    ty,
                    first_definition: name.start(),
                    infer_from_assignment,
                    callable: None,
                    module: None,
                    explicit: true,
                    module_hidden: Some(false),
                    annotation_form: None,
                    annotation_module: None,
                });
        }
    }

    pub(in crate::infrastructure::mypy) fn fresh_exception_alias(
        &self,
        name: &str,
        definition: TextSize,
    ) -> Option<usize> {
        let scope = self.stack.last()?;
        let binding = scope.bindings.get(name)?;
        (scope.kind == Kind::Function
            && !scope.redirects.contains_key(name)
            && binding.first_definition == definition
            && binding.infer_from_assignment
            && binding.ty.is_none()
            && binding.callable.is_none()
            && binding.module.is_none()
            && binding.annotation_form.is_none()
            && binding.annotation_module.is_none())
        .then_some(self.stack.len() - 1)
    }

    pub(in crate::infrastructure::mypy) fn set_exception_alias(
        &mut self,
        name: &str,
        ty: Option<Type>,
    ) -> Option<ExceptionAliasMutation> {
        let index = self.binding_scope(name);
        let scope = &self.stack[index];
        let plain = scope.kind == Kind::Function
            && !scope.redirects.contains_key(name)
            && scope.bindings.get(name).is_some_and(|binding| {
                binding.infer_from_assignment
                    && binding.callable.is_none()
                    && binding.module.is_none()
                    && binding.annotation_form.is_none()
                    && binding.annotation_module.is_none()
            });
        self.clear_annotation_form(name);
        // A checked nested function must not rewrite its enclosing declaration.
        // Global/nonlocal writes are represented by that function's live flow.
        if index + 1 != self.stack.len() {
            return None;
        }
        let binding = self.stack[index]
            .bindings
            .get_mut(name)
            .expect("exception alias was declared in its lexical scope");
        let before = binding.ty;
        binding.ty = ty;
        binding.callable = None;
        binding.module = None;
        Some((index, binding.first_definition, before, binding.ty, plain))
    }

    pub(in crate::infrastructure::mypy) fn binding_location(&self, name: &str) -> Option<TextSize> {
        self.resolved_binding(name)
            .map(|binding| binding.first_definition)
    }
    pub(in crate::infrastructure::mypy) fn inferred_binding_identity(
        &self,
        name: &str,
    ) -> Option<(usize, TextSize)> {
        let binding = self
            .resolved_binding(name)
            .filter(|binding| binding.infer_from_assignment)?;
        self.stack
            .iter()
            .position(|scope| {
                scope
                    .bindings
                    .get(name)
                    .is_some_and(|candidate| std::ptr::eq(candidate, binding))
            })
            .map(|owner| (owner, binding.first_definition))
    }
    pub(in crate::infrastructure::mypy) fn direct_inferred_local_binding(
        &self,
        name: &str,
    ) -> Option<(usize, TextSize)> {
        let scope = self.current();
        if scope.kind != Kind::Function || scope.redirects.contains_key(name) {
            return None;
        }
        let binding = scope
            .bindings
            .get(name)
            .filter(|binding| binding.infer_from_assignment)?;
        Some((self.stack.len() - 1, binding.first_definition))
    }

    pub(in crate::infrastructure::mypy) fn loop_target_annotation_role_absent(
        &self,
        name: &str,
    ) -> bool {
        let variables = &self.current().variables;
        let prefix = format!("{name}.");
        !variables.contains_key(name)
            && variables
                .range::<str, _>((
                    std::ops::Bound::Included(prefix.as_str()),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .is_none_or(|(key, _)| !key.starts_with(prefix.as_str()))
            && self.stack.iter().all(|scope| {
                scope.bound_variables.iter().all(|&ty| {
                    self.context.node(ty).is_some_and(|node| {
                        matches!(node.as_ref(), types::Node::Variable(variable) if variable.name != name)
                    })
                })
            })
    }

    pub(in crate::infrastructure::mypy) fn first_inferred_local_assignment(
        &self,
        target: &Expr,
    ) -> Option<(usize, TextSize)> {
        let Expr::Name(name) = target else {
            return None;
        };
        let scope = self.current();
        if scope.kind != Kind::Function || scope.redirects.contains_key(name.id.as_str()) {
            return None;
        }
        if let Some(binding) = scope.bindings.get(name.id.as_str()) {
            // Retain the existing first-bound definition admission exactly.
            return (binding.infer_from_assignment && binding.first_definition == name.start())
                .then_some((self.stack.len() - 1, binding.first_definition));
        }
        if !self.loop_target_annotation_role_absent(name.id.as_str())
            || scope.ordinary_assignment_starts.get(&name.start()) != Some(&true)
            || scope.first_unbound_local_assignments.get(name.id.as_str()) != Some(&name.start())
        {
            return None;
        }
        // The syntax receipt proves a plain local definition, not an unknown type.
        Some((self.stack.len() - 1, name.start()))
    }

    pub(in crate::infrastructure::mypy) fn resolved_binding(&self, name: &str) -> Option<&Binding> {
        for (index, scope) in self.stack.iter().enumerate().rev() {
            if scope.kind == Kind::Class && index + 1 != self.stack.len() {
                continue;
            }
            if let Some(&target) = scope.redirects.get(name) {
                return self.stack[target].bindings.get(name);
            }
            if let Some(binding) = scope.bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    pub(in crate::infrastructure::mypy) fn prior_inferred_local_definition(
        &self,
        target: &Expr,
        annotation: &Expr,
    ) -> Option<TextSize> {
        let Expr::Name(name) = target else {
            return None;
        };
        let scope = self.current();
        if scope.kind != Kind::Function || scope.redirects.contains_key(name.id.as_str()) {
            return None;
        }
        let binding = scope.bindings.get(name.id.as_str())?;
        if !binding.infer_from_assignment || binding.first_definition >= name.start() {
            return None;
        }
        let form = if let Expr::Subscript(subscript) = annotation {
            subscript.value.as_ref()
        } else {
            annotation
        };
        if self
            .context
            .typing_form(form, &scope.variables)
            .is_some_and(|form| matches!(form.as_str(), "Final" | "ClassVar" | "TypeAlias"))
        {
            return None;
        }
        Some(binding.first_definition)
    }

    pub(in crate::infrastructure::mypy) fn inferred_local_any_assignment(
        &self,
        target: &Expr,
    ) -> bool {
        let Expr::Name(name) = target else {
            return false;
        };
        let scope = self.current();
        if scope.kind != Kind::Function || scope.redirects.contains_key(name.id.as_str()) {
            return false;
        }
        scope.bindings.get(name.id.as_str()).is_some_and(|binding| {
            binding.infer_from_assignment
                && binding.first_definition < name.start()
                && binding.ty.is_some_and(|ty| ty.is_any())
        })
    }

    pub(in crate::infrastructure::mypy) fn resolve_partial_none(
        &mut self,
        target: &Expr,
        actual: Option<Type>,
    ) -> Option<(usize, TextSize)> {
        let Expr::Name(name) = target else {
            return None;
        };
        let index = self.binding_scope(name.id.as_str());
        // Only the current lexical function owns this partial binding.
        if index + 1 != self.stack.len() || self.stack[index].kind != Kind::Function {
            return None;
        }
        let actual = actual.filter(|&ty| !matches!(ty, Type::None | Type::Never))?;
        if self
            .context
            .node(actual)
            .is_some_and(|node| matches!(node.as_ref(), types::Node::Deleted(_)))
        {
            return None;
        }
        let binding = self.stack[index].bindings.get(name.id.as_str())?;
        if !binding.infer_from_assignment
            || binding.first_definition == name.start()
            || binding.ty != Some(Type::None)
        {
            return None;
        }
        let origin = binding.first_definition;
        let inferred = self.context.union(vec![actual, Type::None]);
        self.stack[index]
            .bindings
            .get_mut(name.id.as_str())
            .unwrap()
            .ty = Some(inferred);
        // This proves the mutation, not the original PartialNone lifetime.
        Some((index, origin))
    }

    pub(in crate::infrastructure::mypy) fn unproved_inferred_none_declaration(
        &self,
        name: &str,
    ) -> bool {
        // A readable None is not proof of a pending partial. Refuse that unknown
        // role using the actual inferred binding; never promote it to a view.
        self.inferred_binding_identity(name)
            .is_some_and(|(owner, _)| {
                self.stack[owner].kind == Kind::Function
                    && self.stack[owner]
                        .bindings
                        .get(name)
                        .is_some_and(|binding| binding.ty == Some(Type::None))
            })
    }

    pub(in crate::infrastructure::mypy) fn inferred_declaration(
        &self,
        name: &str,
        owner: usize,
        definition: TextSize,
    ) -> Option<Type> {
        if self.inferred_binding_identity(name) != Some((owner, definition)) {
            return None;
        }
        self.stack
            .get(owner)?
            .bindings
            .get(name)
            .filter(|binding| binding.module.is_none())?
            .ty
    }

    pub(in crate::infrastructure::mypy) fn refine_inferred_binding_receipt(
        &mut self,
        name: &str,
        definition: TextSize,
        ty: Type,
    ) -> Option<(usize, Option<Type>, Type)> {
        if self.binding_location(name) != Some(definition) {
            return None;
        }
        for (owner, scope) in self.stack.iter_mut().enumerate().rev() {
            if let Some(binding) = scope
                .bindings
                .get_mut(name)
                .filter(|binding| binding.first_definition == definition)
            {
                if !binding.infer_from_assignment {
                    return None;
                }
                let before = binding.ty;
                binding.ty = Some(ty);
                binding.annotation_form = None;
                binding.annotation_module = None;
                return Some((owner, before, ty));
            }
        }
        None
    }
}
