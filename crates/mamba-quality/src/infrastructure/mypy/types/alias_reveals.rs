//! Reveal output expands recursive aliases until it reaches an active definition.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

impl Context {
    pub(in crate::infrastructure::mypy) fn revealed_name(&self, ty: Type) -> String {
        self.revealed_name_for(ty, PythonTarget::Py312)
    }

    pub(in crate::infrastructure::mypy) fn revealed_name_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> String {
        let origin = self.aliases.borrow().origins.get(&ty).cloned();
        if let Some((id, _)) = origin {
            if !self.aliases.borrow_mut().reveals.insert(id) {
                return "...".into();
            }
            let name = self.revealed_structure(ty, target);
            self.aliases.borrow_mut().reveals.remove(&id);
            name
        } else {
            self.revealed_structure(ty, target)
        }
    }

    fn revealed_structure(&self, ty: Type, target: PythonTarget) -> String {
        let Some(node) = self.node(ty) else {
            return self.name(ty);
        };
        match node.as_ref() {
            Node::AliasInstance(id, arguments) => {
                if !self.aliases.borrow_mut().reveals.insert(*id) {
                    return "...".into();
                }
                let expanded = self.revealed_alias_target(*id, arguments);
                let name = self.revealed_structure(expanded, target);
                self.aliases.borrow_mut().reveals.remove(id);
                name
            }
            Node::Container(kind, arguments) => {
                let prefix = if matches!(
                    kind,
                    Container::Sequence
                        | Container::Iterable
                        | Container::Mapping
                        | Container::Awaitable
                        | Container::Coroutine
                        | Container::Generator
                        | Container::Iterator
                        | Container::AsyncGenerator
                        | Container::AsyncIterator
                        | Container::AsyncIterable
                ) {
                    "typing."
                } else {
                    ""
                };
                format!(
                    "{prefix}{}[{}]",
                    kind.name(),
                    self.revealed_items(arguments, ", ", target)
                )
            }
            Node::Tuple(items) if items.is_empty() => "tuple[()]".into(),
            Node::Tuple(items) => format!("tuple[{}]", self.revealed_items(items, ", ", target)),
            Node::Unpack(item) => format!("*{}", self.revealed_name_for(*item, target)),
            Node::VariadicTuple(item) => {
                format!("tuple[{}, ...]", self.revealed_name_for(*item, target))
            }
            Node::Union(items) => items
                .iter()
                .map(|&item| {
                    let text = self.revealed_name_for(item, target);
                    if matches!(
                        self.node(self.expanded_alias_instance(item)).as_deref(),
                        Some(Node::Callable(_))
                    ) {
                        format!("({text})")
                    } else {
                        text
                    }
                })
                .collect::<Vec<_>>()
                .join(" | "),
            Node::Callable(signature) => self.revealed_callable(signature, target),
            Node::Instance(id) => self.revealed_class(*id, &[], false),
            Node::Class(id) => {
                let original = self
                    .classes
                    .borrow()
                    .embedded_constructor_reveals
                    .get(id)
                    .cloned();
                original
                    .or_else(|| self.class_constructor_for(ty, target))
                    .map(|signature| self.revealed_callable(&signature, target))
                    .unwrap_or_else(|| self.revealed_class(*id, &[], true))
            }
            Node::AppliedClass {
                id,
                arguments,
                value,
            } if *value => self
                .class_constructor_for(ty, target)
                .map(|signature| self.revealed_callable(&signature, target))
                .unwrap_or_else(|| self.revealed_class(*id, arguments, true)),
            Node::AppliedClass { id, arguments, .. } => self.revealed_class(*id, arguments, false),
            Node::Parameters(parameters, ellipsis) => {
                self.revealed_parameters(parameters, *ellipsis, target)
            }
            Node::InferredAlias { target: value, .. } => self.revealed_structure(*value, target),
            Node::LiteralValue(value) => format!("Literal[{}]?", value.display()),
            _ => self.name(ty),
        }
    }

    fn revealed_alias_target(&self, id: u32, arguments: &[Type]) -> Type {
        let alias = self.alias(id);
        if arguments.len() != alias.parameters.len() {
            return self
                .expand_alias(id, arguments.to_vec())
                .unwrap_or(Type::Any);
        }
        let solutions = alias
            .parameters
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect();
        let Some(target) = alias.target else {
            return Type::Any;
        };
        if let Some(node) = self.node(target) {
            if let Node::Union(items) = node.as_ref() {
                return self.intern(Node::Union(
                    items
                        .iter()
                        .map(|&ty| self.substitute(ty, &solutions))
                        .collect(),
                ));
            }
        }
        self.substitute(target, &solutions)
    }

    fn revealed_items(&self, items: &[Type], separator: &str, target: PythonTarget) -> String {
        items
            .iter()
            .map(|&ty| self.revealed_name_for(ty, target))
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn revealed_callable(
        &self,
        signature: &super::super::signatures::Signature,
        target: PythonTarget,
    ) -> String {
        use super::super::signatures::Kind;
        if !signature.overloads.is_empty() {
            return format!(
                "Overload({})",
                signature
                    .overloads
                    .iter()
                    .map(|variant| self.revealed_callable(variant, target))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        let mut arguments = Vec::new();
        let mut star = false;
        for parameter in signature.parameters.iter() {
            if parameter.kind == Kind::KeywordOnly && !star {
                arguments.push("*".into());
                star = true;
            }
            let prefix = match parameter.kind {
                Kind::VariadicPositional => {
                    star = true;
                    "*"
                }
                Kind::VariadicKeyword => "**",
                _ => "",
            };
            let parameter_type = parameter.ty.unwrap_or(Type::Any);
            let ty = self.revealed_name_for(parameter_type, target);
            let ty = if parameter.kind == Kind::VariadicKeyword
                && self.typed_dict(parameter_type).is_some()
            {
                format!("**{ty}")
            } else {
                ty
            };
            let mut argument =
                if parameter.name.is_empty() || parameter.kind == Kind::PositionalOnly {
                    format!("{prefix}{ty}")
                } else {
                    format!("{prefix}{}: {ty}", parameter.name)
                };
            if !parameter.required
                && matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::KeywordOnly
                )
            {
                argument.push_str(" =");
            }
            arguments.push(argument);
        }
        let generic = if signature.generic.is_empty() {
            String::new()
        } else {
            format!(
                "[{}] ",
                signature
                    .generic
                    .iter()
                    .map(|&ty| {
                        let node = self.node(ty);
                        let mut name = match node.as_deref() {
                            Some(Node::Variable(variable)) if !variable.values.is_empty() => {
                                format!(
                                    "{} in ({})",
                                    variable.name,
                                    self.revealed_items(&variable.values, ", ", target)
                                )
                            }
                            Some(Node::Variable(variable))
                                if variable.bound != Type::Object
                                    && !variable.parameter_spec
                                    && !variable.type_var_tuple =>
                            {
                                format!(
                                    "{} <: {}",
                                    variable.name,
                                    self.revealed_name_for(variable.bound, target)
                                )
                            }
                            _ => self.name(ty),
                        };
                        if let Some(Node::Variable(variable)) = node.as_deref() {
                            if let Some(default) = variable.default {
                                name.push_str(&format!(
                                    " = {}",
                                    self.revealed_name_for(default, target)
                                ));
                            }
                        }
                        name
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let returned = signature.returns.unwrap_or(Type::Any);
        let result = if let Some((kind, ty)) = signature.narrowing {
            format!(
                " -> {}[{}]",
                kind.name(),
                self.revealed_name_for(ty, target)
            )
        } else if returned == Type::None {
            String::new()
        } else {
            format!(" -> {}", self.revealed_name_for(returned, target))
        };
        format!("def {generic}({}){result}", arguments.join(", "))
    }
}
