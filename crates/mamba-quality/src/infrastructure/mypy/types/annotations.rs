use super::*;
use graph::{Container, Node};

impl Context {
    // The normal parser owns this vocabulary. Import proof may identify a role,
    // but cannot add a constructor or infer a type from its runtime value.
    pub(in crate::infrastructure::mypy) fn annotation_constructor(
        name: &str,
    ) -> Option<(&'static str, Option<Container>)> {
        Some(match name {
            "tuple" | "Tuple" => ("tuple", None),
            "Union" => ("Union", None),
            "Optional" => ("Optional", None),
            "list" | "List" => ("list", Some(Container::List)),
            "set" | "Set" => ("set", Some(Container::Set)),
            "frozenset" | "FrozenSet" => ("frozenset", Some(Container::FrozenSet)),
            "dict" | "Dict" => ("dict", Some(Container::Dict)),
            "Sequence" => ("Sequence", Some(Container::Sequence)),
            "Iterable" => ("Iterable", Some(Container::Iterable)),
            "Mapping" => ("Mapping", Some(Container::Mapping)),
            "dict_keys" => ("dict_keys", Some(Container::DictKeys)),
            "dict_values" => ("dict_values", Some(Container::DictValues)),
            "dict_items" => ("dict_items", Some(Container::DictItems)),
            "type" => ("type", Some(Container::TypeObject)),
            _ => return None,
        })
    }

    pub(in crate::infrastructure::mypy) fn annotation_form_name(
        expression: &Expr,
    ) -> Option<String> {
        annotation_name(expression)
    }

    // This recognizes a marker only. Scopes supplies current binding proof.
    pub(in crate::infrastructure::mypy) fn is_callable_form(&self, ty: Type) -> bool {
        matches!(self.node(ty).as_deref(), Some(Node::TypingForm(form)) if form == "Callable")
    }

    fn callable_form_annotation(&self, ty: Type) -> Option<Type> {
        // Alias targets retain ordinary Callable type semantics, not proof.
        self.is_callable_form(ty)
            .then(|| self.bare_callable(&VariableMap::new()))
    }

    // Base positions need the declared owner even when value annotations are compact.
    pub(in crate::infrastructure::mypy) fn class_base_annotation(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        if let Expr::Subscript(subscript) = expression {
            let arguments: Vec<_> = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                argument => vec![argument],
            };
            let nominal = self
                .alias_annotation(&subscript.value, &arguments, variables)
                .or_else(|| self.class_annotation(&subscript.value, &arguments, variables));
            if let Some(nominal) =
                nominal.filter(|&ty| self.canonical_collection_kind(ty).is_some())
            {
                return Some(nominal);
            }
        }
        self.annotation_with_variables(expression, variables)
    }

    pub(super) fn canonical_collection_kind(&self, ty: Type) -> Option<Container> {
        let (id, argument_count) = match self.node(ty).as_deref() {
            Some(Node::AppliedClass {
                id,
                arguments,
                value: false,
            }) => (*id, arguments.len()),
            _ => return None,
        };
        let class = self.class(id);
        // The source lookup verifies project, target, fullname, and declaration ID.
        // Native declarations must own the exact canonical zero-scope registry key.
        let fullname = self.embedded_source_class_name(id).or_else(|| {
            (self
                .classes
                .borrow()
                .keys
                .get(&(class.namespace.clone(), 0))
                == Some(&id))
            .then(|| class.namespace.clone())
        })?;
        let (kind, arity) = match fullname.as_str() {
            "typing.Generator" => (Container::Generator, 3),
            "typing.Coroutine" => (Container::Coroutine, 3),
            "typing.Awaitable" => (Container::Awaitable, 1),
            "typing.Sequence" => (Container::Sequence, 1),
            "typing.Mapping" => (Container::Mapping, 2),
            _ => return None,
        };
        (class.generic.len() == arity && argument_count == arity).then_some(kind)
    }

    pub(super) fn compact_applied_collection_annotation(&self, ty: Type) -> Type {
        // Alias targets retain the nominal owner for later class-base resolution.
        if self.aliases.borrow().building != 0 {
            return ty;
        }
        let compact = if let Some(kind) = self.canonical_collection_kind(ty) {
            let arguments = self
                .class_arguments(ty)
                .into_iter()
                .map(|argument| self.compact_applied_collection_annotation(argument))
                .collect();
            self.intern(Node::Container(kind, arguments))
        } else {
            match self.node(ty).as_deref() {
                Some(Node::Union(items)) => self.intern(Node::Union(
                    items
                        .iter()
                        .map(|&item| self.compact_applied_collection_annotation(item))
                        .collect(),
                )),
                Some(Node::Container(kind, arguments)) => self.intern(Node::Container(
                    *kind,
                    arguments
                        .iter()
                        .map(|&argument| self.compact_applied_collection_annotation(argument))
                        .collect(),
                )),
                // Recursive alias instances stay finite references, not new expansions.
                _ => ty,
            }
        };
        if compact != ty {
            let origin = self.aliases.borrow().origins.get(&ty).cloned();
            if let Some(origin) = origin {
                self.aliases.borrow_mut().origins.insert(compact, origin);
            }
        }
        compact
    }

    pub(crate) fn annotation_with_variables(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        if self.predicate_annotation(expression, variables).is_some() {
            return Some(Type::Bool);
        }
        if let Some(ty) = self.builtin_alias_annotation(expression, variables) {
            return Some(ty);
        }
        if matches!(
            self.typing_form(expression, variables).as_deref(),
            Some("Final" | "Self")
        ) {
            return None;
        }
        if let Some(ty) = self.asynchronous_annotation(expression, variables) {
            return Some(ty);
        }
        match expression {
            Expr::Starred(starred) => Some(self.intern(Node::Unpack(
                self.annotation_with_variables(&starred.value, variables)?,
            ))),
            Expr::NoneLiteral(_) => Some(Type::None),
            Expr::Name(name) => variables
                .get(name.id.as_str())
                .copied()
                .and_then(|ty| {
                    self.callable_form_annotation(ty)
                        .or_else(|| self.annotation_alias_value(ty))
                })
                .or_else(|| scalar(name.id.as_str()))
                .or_else(|| {
                    (name.id.as_str() == "Callable").then(|| self.bare_callable(variables))
                }),
            Expr::Attribute(attribute) => self
                .parameter_spec_projection(attribute, variables)
                .or_else(|| {
                    annotation_name(expression)
                        .and_then(|name| variables.get(&name).copied())
                        .and_then(|ty| {
                            self.callable_form_annotation(ty)
                                .or_else(|| self.annotation_alias_value(ty))
                        })
                        .or_else(|| scalar(attribute.attr.as_str()))
                        .or_else(|| {
                            (attribute.attr.as_str() == "Callable")
                                .then(|| self.bare_callable(variables))
                        })
                }),
            Expr::StringLiteral(string) => {
                self.forward_annotation(string.value.to_str(), variables)
            }
            Expr::BinOp(binary) if binary.op == Operator::BitOr => Some(self.union(vec![
                self.annotation_with_variables(&binary.left, variables)?,
                self.annotation_with_variables(&binary.right, variables)?,
            ])),
            Expr::Subscript(subscript) => {
                let dynamic = annotation_name(&subscript.value)
                    .and_then(|name| variables.get(&name).copied());
                if dynamic.is_some_and(|ty| ty.is_any()) {
                    return Some(Type::Any);
                }
                let name = match subscript.value.as_ref() {
                    Expr::Name(name) => name.id.as_str(),
                    Expr::Attribute(attribute) => attribute.attr.as_str(),
                    _ => return None,
                };
                let args: Vec<_> = if let Expr::Tuple(tuple) = subscript.slice.as_ref() {
                    tuple.elts.iter().collect()
                } else {
                    vec![subscript.slice.as_ref()]
                };
                if self.typing_form(&subscript.value, variables).as_deref() == Some("Annotated") {
                    return self.annotation_with_variables(*args.first()?, variables);
                }
                if self.typing_form(&subscript.value, variables).as_deref() == Some("Callable")
                    || name == "Callable"
                {
                    return self.callable_annotation(&args, variables);
                }
                if self.typing_form(&subscript.value, variables).as_deref() == Some("Type")
                    && args.len() == 1
                {
                    return Some(self.intern(Node::Container(
                        Container::TypeObject,
                        vec![self.annotation_with_variables(args[0], variables)?],
                    )));
                }
                if self.typing_form(&subscript.value, variables).as_deref() == Some("Unpack")
                    && args.len() == 1
                {
                    return Some(self.intern(Node::Unpack(
                        self.annotation_with_variables(args[0], variables)?,
                    )));
                }
                if let Some(ty) = self.alias_annotation(&subscript.value, &args, variables) {
                    return Some(self.compact_applied_collection_annotation(ty));
                }
                if let Some(ty) = self.class_annotation(&subscript.value, &args, variables) {
                    return Some(self.compact_applied_collection_annotation(ty));
                }
                if self.is_literal_form(&subscript.value, variables) {
                    return self.literal_annotation(&args, variables);
                }
                if matches!(
                    self.typing_form(&subscript.value, variables).as_deref(),
                    Some(
                        "Required" | "NotRequired" | "ReadOnly" | "ClassVar" | "Final" | "InitVar"
                    )
                ) && args.len() == 1
                {
                    return self.annotation_with_variables(args[0], variables);
                }
                let (constructor, kind) = Self::annotation_constructor(name)?;
                if constructor == "tuple" {
                    if args.len() == 2 && args[1].is_ellipsis_literal_expr() {
                        return Some(self.intern(Node::VariadicTuple(
                            self.annotation_with_variables(args[0], variables)?,
                        )));
                    }
                    return Some(
                        self.normalize_tuple(
                            args.into_iter()
                                .map(|argument| self.annotation_with_variables(argument, variables))
                                .collect::<Option<Vec<_>>>()?,
                        ),
                    );
                }
                if matches!(constructor, "Union" | "Optional") {
                    let mut types = args
                        .into_iter()
                        .map(|argument| self.annotation_with_variables(argument, variables))
                        .collect::<Option<Vec<_>>>()?;
                    if constructor == "Optional" {
                        types.push(Type::None);
                    }
                    return Some(self.union(types));
                }
                let kind = kind?;
                let types = args
                    .into_iter()
                    .map(|argument| self.annotation_with_variables(argument, variables))
                    .collect::<Option<Vec<_>>>()?;
                if types.len()
                    != if matches!(
                        kind,
                        Container::Dict
                            | Container::Mapping
                            | Container::DictKeys
                            | Container::DictValues
                            | Container::DictItems
                    ) {
                        2
                    } else {
                        1
                    }
                {
                    return None;
                }
                if kind == Container::TypeObject
                    && self.node(types[0]).is_some_and(|node| {
                        matches!(node.as_ref(), Node::Container(Container::TypeObject, _))
                    })
                {
                    return Some(Type::Any);
                }
                Some(self.intern(Node::Container(kind, types)))
            }
            _ => None,
        }
    }
}

pub(super) fn annotation_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            annotation_name(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}

fn scalar(name: &str) -> Option<Type> {
    Some(match name {
        "int" => Type::Int,
        "float" => Type::Float,
        "complex" => Type::Complex,
        "str" => Type::Str,
        "bytes" => Type::Bytes,
        "bool" => Type::Bool,
        "Any" => Type::Any,
        "type" => Type::Any,
        "object" => Type::Object,
        "Never" | "NoReturn" => Type::Never,
        _ => return None,
    })
}
