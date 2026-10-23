//! Recursive targets keep finite references instead of an infinite type tree.
use super::*;
use mamba_language::application::quality_syntax;

impl Context {
    pub(super) fn alias_annotation(
        &self,
        base: &Expr,
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Type> {
        let name = self.annotation_binding(base, variables)?;
        let id = self.alias_id(name)?;
        let arguments = self.generic_arguments(&self.alias(id).parameters, arguments, variables)?;
        if self.aliases.borrow().building != 0 {
            Some(self.intern(Node::AliasInstance(id, arguments)))
        } else {
            self.expand_alias(id, arguments)
        }
    }

    pub(super) fn forward_annotation(&self, source: &str, variables: &VariableMap) -> Option<Type> {
        let source = format!("({source})\n");
        let document =
            quality_syntax::parse(&source, quality_syntax::PythonTarget::Py314, false).ok()?;
        if !document.is_valid() {
            return None;
        }
        let [quality_syntax::python_ast::Stmt::Expr(statement)] =
            document.parsed.syntax().body.as_slice()
        else {
            return None;
        };
        // Another string literal does not make progress as a forward expression.
        if matches!(statement.value.as_ref(), Expr::StringLiteral(_)) {
            return None;
        }
        self.annotation_with_variables(&statement.value, variables)
    }

    pub(super) fn is_alias_instance(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(|node| {
            matches!(
                node.as_ref(),
                Node::AliasInstance(_, _) | Node::InferredAlias { .. }
            )
        })
    }

    pub(in crate::infrastructure::mypy) fn expanded_alias_instance(&self, ty: Type) -> Type {
        let Some(node) = self.node(ty) else { return ty };
        let (id, expanded) = match node.as_ref() {
            Node::InferredAlias { id, target, .. } => (*id, *target),
            Node::AliasInstance(id, arguments) => (
                *id,
                self.expand_alias(*id, arguments.clone())
                    .unwrap_or(Type::Any),
            ),
            _ => return ty,
        };
        let building = self.aliases.borrow().building != 0;
        if !building && self.recursive_alias(id) {
            // Each recursive value expansion needs the same finite collection view.
            // Stored alias targets and class-base resolution remain nominal.
            self.compact_applied_collection_annotation(expanded)
        } else {
            expanded
        }
    }

    pub(super) fn expand_alias_nodes(&self, ty: Type) -> Type {
        let Some(node) = self.node(ty) else { return ty };
        let expanded = match node.as_ref() {
            Node::AliasInstance(id, arguments) => {
                if self.aliases.borrow().active.contains(id) || self.recursive_alias(*id) {
                    return ty;
                }
                return self.expand_alias(*id, arguments.clone()).unwrap_or(ty);
            }
            Node::Container(kind, arguments) => Node::Container(
                *kind,
                arguments
                    .iter()
                    .map(|&ty| self.expand_alias_nodes(ty))
                    .collect(),
            ),
            Node::Tuple(arguments) => {
                return self.normalize_tuple(
                    arguments
                        .iter()
                        .map(|&ty| self.expand_alias_nodes(ty))
                        .collect(),
                )
            }
            Node::VariadicTuple(item) => Node::VariadicTuple(self.expand_alias_nodes(*item)),
            Node::Unpack(item) => Node::Unpack(self.expand_alias_nodes(*item)),
            Node::Union(items) => {
                let mut expanded = Vec::new();
                for &item in items {
                    let value = self.expand_alias_nodes(item);
                    let node = self.node(value);
                    let keep_group =
                        matches!(self.node(item).as_deref(), Some(Node::AliasInstance(_, _)))
                            && matches!(node.as_deref(), Some(Node::Union(values))
                        if !values.is_empty() && values.iter().all(|&value|
                            matches!(self.node(value).as_deref(), Some(Node::Literal(_)))));
                    let values = match node.as_deref() {
                        Some(Node::Union(values)) if !keep_group => values.as_slice(),
                        _ => std::slice::from_ref(&value),
                    };
                    for &value in values {
                        if value != Type::Never && !expanded.contains(&value) {
                            expanded.push(value);
                        }
                    }
                }
                return match expanded.len() {
                    0 => Type::Never,
                    1 => expanded[0],
                    _ => self.intern(Node::Union(expanded)),
                };
            }
            _ => return ty,
        };
        self.intern(expanded)
    }

    pub(super) fn alias_instance_name(&self, id: u32, arguments: &[Type]) -> String {
        let name = self.alias(id).name.clone();
        if arguments.is_empty() {
            name
        } else {
            format!(
                "{name}[{}]",
                arguments
                    .iter()
                    .map(|&ty| self.name(ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    pub(in crate::infrastructure::mypy) fn assignment_type_name(&self, ty: Type) -> String {
        if let Some((id, arguments)) = self.alias_application(ty) {
            return self.alias_instance_name(id, &arguments);
        }
        let origin = self.aliases.borrow().origins.get(&ty).cloned();
        origin.map_or_else(
            || self.name(ty),
            |(id, arguments)| self.alias_instance_name(id, &arguments),
        )
    }

    pub(in crate::infrastructure::mypy) fn alias_target_annotation(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        self.aliases.borrow_mut().building += 1;
        let result = self.annotation_with_variables(expression, variables);
        self.aliases.borrow_mut().building -= 1;
        result
    }

    pub(super) fn alias_application(&self, ty: Type) -> Option<(u32, Vec<Type>)> {
        match self.node(ty).as_deref() {
            Some(
                Node::AliasInstance(id, arguments) | Node::InferredAlias { id, arguments, .. },
            ) => Some((*id, arguments.clone())),
            _ => self.aliases.borrow().origins.get(&ty).cloned(),
        }
    }
}
