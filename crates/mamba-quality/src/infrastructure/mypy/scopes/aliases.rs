//! Alias names bind separately from values and retain declaration identities.
use super::*;
use mamba_language::application::quality_syntax;
use quality_syntax::python_ast::TypeParams;

fn declaration<'a>(
    statement: &'a Stmt,
    context: &types::Context,
    variables: &types::Variables,
    namespace: &str,
) -> Option<(
    &'a quality_syntax::python_ast::ExprName,
    Option<&'a TypeParams>,
    &'a Expr,
    bool,
)> {
    match statement {
        Stmt::TypeAlias(alias) => {
            let Expr::Name(name) = alias.name.as_ref() else {
                return None;
            };
            Some((name, alias.type_params.as_deref(), &alias.value, true))
        }
        Stmt::AnnAssign(assignment)
            if context
                .typing_form(&assignment.annotation, variables)
                .as_deref()
                == Some("TypeAlias") =>
        {
            let Expr::Name(name) = assignment.target.as_ref() else {
                return None;
            };
            Some((name, None, assignment.value.as_deref()?, false))
        }
        Stmt::Assign(assignment) => {
            let [Expr::Name(name)] = assignment.targets.as_slice() else {
                return None;
            };
            if !matches!(
                assignment.value.as_ref(),
                Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::BinOp(_)
            ) {
                return None;
            }
            if matches!(
                assignment.value.as_ref(),
                Expr::Name(_) | Expr::Attribute(_)
            ) && context
                .annotation_binding(&assignment.value, variables)
                .and_then(|ty| context.node(ty))
                .is_some_and(|node| {
                    matches!(
                        node.as_ref(),
                        types::Node::Container(types::Container::TypeObject, _)
                    )
                })
            {
                return None;
            }
            let id = context.declare_alias(
                namespace,
                name.id.as_str(),
                name.start().to_u32(),
                false,
                true,
            );
            let mut probe = variables.clone();
            probe.insert(name.id.to_string(), context.alias_marker(id));
            if context
                .alias_target_annotation(&assignment.value, &probe)
                .is_some()
            {
                Some((name, None, &assignment.value, false))
            } else {
                None
            }
        }
        _ => None,
    }
}

impl Scopes {
    pub(super) fn callable_aliases(&self, body: &[Stmt], variables: &mut types::Variables) {
        let mut declarations = Vec::new();
        // Protect only explicit body-local aliases; implicit Assign keeps its old heuristic.
        let mut explicit_owners = BTreeMap::new();
        for statement in body {
            if let Some((name, parameters, value, modern)) =
                declaration(statement, &self.context, variables, &self.namespace)
            {
                let id = self.context.declare_alias(
                    &self.namespace,
                    name.id.as_str(),
                    name.start().to_u32(),
                    modern,
                    matches!(statement, Stmt::Assign(_)),
                );
                if explicit_owners
                    .get(name.id.as_str())
                    .is_some_and(|&owner| owner != id)
                {
                    continue;
                }
                if !self.context.alias(id).implicit {
                    explicit_owners.insert(name.id.to_string(), id);
                }
                variables.insert(name.id.to_string(), self.context.alias_marker(id));
                declarations.push((name, parameters, value, id));
            }
        }
        for _ in 0..=declarations.len() {
            let mut changed = false;
            for &(name, parameters, value, id) in &declarations {
                let previous = self.context.alias(id);
                let source_class = self
                    .context
                    .annotation_binding(value, &self.environment())
                    .and_then(|ty| self.context.node(ty))
                    .and_then(|node| {
                        if let types::Node::Class(id) = node.as_ref() {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .or_else(|| self.collection_alias_class(value, variables))
                    .or_else(|| {
                        let ty = self.module_member(value)?.ty?;
                        let id = self.context.class_id(ty)?;
                        (self.context.is_class_value(ty)
                            && self.context.class(id).namespace.starts_with("typing."))
                        .then_some(id)
                    });
                self.context
                    .populate_alias(id, parameters, value, variables, source_class);
                let current = self.context.alias(id);
                changed |=
                    current.target != previous.target || current.bare_class != previous.bare_class;
                changed |= current.explicit_arguments != previous.explicit_arguments;
                variables.insert(name.id.to_string(), self.context.alias_marker(id));
            }
            if !changed {
                break;
            }
        }
    }

    pub(super) fn refresh_type_aliases(&mut self, body: &[Stmt]) {
        let mut variables = self.live_annotation_variables(&self.current().variables);
        self.callable_aliases(body, &mut variables);
        self.stack.last_mut().unwrap().variables = variables;
        let annotation_writes = self.annotation_value_writes(body);
        for statement in body {
            if let Some((name, parameters, value, modern)) = declaration(
                statement,
                &self.context,
                &self.current().variables,
                &self.namespace,
            ) {
                let Some(&ty) = self.current().variables.get(name.id.as_str()) else {
                    continue;
                };
                if !self.matches_alias_declaration(statement, name, modern, ty) {
                    continue;
                }
                let annotation_form = (matches!(statement, Stmt::Assign(_))
                    && parameters.is_none()
                    && annotation_writes.as_ref().is_some_and(|writes| {
                        writes.get(name.id.as_str()) == Some(&Some(name.start()))
                    }))
                .then(|| self.alias_callable_form(value))
                .flatten();
                self.stack.last_mut().unwrap().bindings.insert(
                    name.id.to_string(),
                    Binding {
                        ty: Some(ty),
                        first_definition: name.start(),
                        infer_from_assignment: false,
                        callable: None,
                        module: None,
                        explicit: true,
                        module_hidden: Some(false),
                        annotation_form,
                        annotation_module: None,
                    },
                );
            }
        }
    }

    pub(in crate::infrastructure::mypy) fn alias_definition(&self, statement: &Stmt) -> bool {
        declaration(
            statement,
            &self.context,
            &self.current().variables,
            &self.namespace,
        )
        .is_some_and(|(name, _, _, modern)| {
            self.current()
                .variables
                .get(name.id.as_str())
                .is_some_and(|&ty| self.matches_alias_declaration(statement, name, modern, ty))
        })
    }

    fn matches_alias_declaration(
        &self,
        statement: &Stmt,
        name: &quality_syntax::python_ast::ExprName,
        modern: bool,
        ty: Type,
    ) -> bool {
        let Some(owner) = self.context.alias_id(ty) else {
            return false;
        };
        // Do not freeze ordinary function/class value assignments as true aliases.
        if self.context.alias(owner).implicit {
            return true;
        }
        let id = self.context.declare_alias(
            &self.namespace,
            name.id.as_str(),
            name.start().to_u32(),
            modern,
            matches!(statement, Stmt::Assign(_)),
        );
        owner == id
    }
}
